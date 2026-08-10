use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::component::Component;
use crate::level::Level;
use crate::net::NetId;
use crate::pin::{Pin, PinDirection};
use crate::signal::Signal;

/// Identifies a component instance registered in a `Circuit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentId(usize);

/// A component scheduled to be (re-)evaluated at a future logical tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ScheduledEvent {
    tick: u64,
    component: ComponentId,
}

/// A net toggling more than this many times within a single [`Circuit::advance`]
/// call is considered unstable (e.g. a ring oscillator) rather than left to loop
/// forever.
const MAX_TOGGLES_PER_NET: u32 = 1_000;

/// [`Circuit::run`] is [`Circuit::advance`] with this many ticks — enormously more
/// than any settling (non-periodic) circuit needs, but still a hard bound so a
/// periodic component (a `Clock`) can't make it hang forever.
const MAX_RUN_TICKS: u64 = 1_000_000;

/// Returned by [`Circuit::run`] when a net toggles more than [`MAX_TOGGLES_PER_NET`]
/// times without settling — e.g. a combinational feedback loop that oscillates
/// instead of converging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnstableCircuit {
    pub net: NetId,
}

/// A component instance as registered on a `Circuit`: the component itself, plus
/// the pins (and therefore nets) it's wired to.
struct RegisteredComponent {
    component: Box<dyn Component>,
    pins: Vec<Pin>,
}

/// A discrete-event circuit: components wired through nets, simulated with a
/// logical clock and per-component propagation delay.
///
/// An input change to a component doesn't update its output immediately — it
/// schedules that component's own re-evaluation at `now + its propagation_delay`
/// (see [`Component::propagation_delay`]). This is what lets a feedback loop
/// (e.g. an SR-NAND latch) settle over a few ticks instead of recursing forever.
#[derive(Default)]
pub struct Circuit {
    next_net_id: usize,
    next_component_id: usize,
    components: HashMap<ComponentId, RegisteredComponent>,
    /// Each net's current contribution per driving **pin** — `(component,
    /// pin index)` — so re-evaluating a component replaces only its own
    /// contributions, not every other driver's.
    ///
    /// Keyed by pin rather than by component for two reasons: a component
    /// with two output pins on one net would otherwise have the second
    /// overwrite the first, and a contribution has to be able to follow its
    /// pin when [`Circuit::rewire`] moves that pin to a different net.
    drivers: HashMap<NetId, HashMap<(ComponentId, usize), Signal>>,
    /// Each net's last resolved signal, used to detect real changes.
    settled: HashMap<NetId, Signal>,
    clock: u64,
    events: BinaryHeap<Reverse<ScheduledEvent>>,
    /// Components that reschedule themselves forever after every evaluation
    /// (e.g. a `Clock`), and how many ticks between each reschedule.
    periodic: HashMap<ComponentId, u64>,
}

impl Circuit {
    /// Creates an empty circuit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a new, currently undriven net.
    pub fn add_net(&mut self) -> NetId {
        let id = NetId(self.next_net_id);
        self.next_net_id += 1;
        id
    }

    /// Registers a component instance wired to the given pins. The order of `pins`
    /// determines how inputs/outputs map to `Component::eval`: input signals are
    /// read in pin order (keeping only `Input` and `InOut` pins), and returned
    /// outputs are written back in the same filtered order for `Output`/`InOut` pins.
    pub fn add_component(&mut self, component: Box<dyn Component>, pins: Vec<Pin>) -> ComponentId {
        let id = ComponentId(self.next_component_id);
        self.next_component_id += 1;
        self.components
            .insert(id, RegisteredComponent { component, pins });
        id
    }

    /// The pins a registered component is wired to.
    ///
    /// # Panics
    /// If `component` isn't registered. Use [`Circuit::try_pins`] when the
    /// component may already have been removed.
    pub fn pins(&self, component: ComponentId) -> &[Pin] {
        &self.components[&component].pins
    }

    /// The pins a component is wired to, or `None` if it isn't registered —
    /// the answer to "what is this pin on?" for callers holding an id that
    /// may since have been removed.
    pub fn try_pins(&self, component: ComponentId) -> Option<&[Pin]> {
        self.components
            .get(&component)
            .map(|registered| registered.pins.as_slice())
    }

    /// Replaces the entire pin-to-net mapping: each entry of `groups` lists
    /// pins that share one net, and every pin not mentioned gets a net to
    /// itself.
    ///
    /// This is connectivity *derived* rather than accumulated. The caller
    /// owns the drawing — which wires exist and what they touch — and hands
    /// over the resulting groups after every edit, so a net is always a
    /// statement about the drawing as it stands now.
    ///
    /// Merging nets destructively as each wire is drawn — which is what
    /// this replaced — can't express that, because a merge doesn't remember
    /// *why* two pins ended up together: cut one of two parallel wires
    /// between the same pins and there's no way to tell that the other
    /// still holds them connected. Recomputing sidesteps the question
    /// entirely — that case simply comes out as the same group.
    ///
    /// Contributions already driven onto a net follow their pin, so nothing
    /// has to be re-evaluated to get the values back. Only components whose
    /// inputs genuinely changed value are scheduled; a component with no
    /// inputs at all (a `Clock`, a `Button`) is never disturbed, so editing
    /// elsewhere can't shift a clock's phase.
    pub fn rewire(&mut self, groups: &[Vec<(ComponentId, usize)>]) {
        // What each pin drives, and what each reading pin currently sees.
        // Both are indexed by pin so they survive the remap below.
        let contributions: HashMap<(ComponentId, usize), Signal> = self
            .drivers
            .values()
            .flat_map(|on_net| on_net.iter().map(|(&pin, signal)| (pin, signal.clone())))
            .collect();
        let previously_read: HashMap<(ComponentId, usize), Signal> = self
            .reading_pins()
            .map(|(pin, net)| (pin, self.signal_at(net)))
            .collect();

        // One fresh net per group, then one each for the pins left over.
        let mut assignment: HashMap<(ComponentId, usize), NetId> = HashMap::new();
        for group in groups {
            let net = self.add_net();
            for &pin in group {
                assignment.insert(pin, net);
            }
        }
        let unassigned: Vec<(ComponentId, usize)> = self
            .components
            .iter()
            .flat_map(|(&component, registered)| {
                (0..registered.pins.len()).map(move |index| (component, index))
            })
            .filter(|pin| !assignment.contains_key(pin))
            .collect();
        for pin in unassigned {
            let net = self.add_net();
            assignment.insert(pin, net);
        }

        for (&component, registered) in self.components.iter_mut() {
            for (index, pin) in registered.pins.iter_mut().enumerate() {
                if let Some(&net) = assignment.get(&(component, index)) {
                    pin.net = net;
                }
            }
        }

        // Rebuild what each net carries from the contributions that moved.
        self.drivers.clear();
        for (pin, signal) in contributions {
            let Some(&net) = assignment.get(&pin) else {
                continue; // Its component has since been removed.
            };
            self.drivers.entry(net).or_default().insert(pin, signal);
        }
        self.settled.clear();
        for &net in assignment.values() {
            let resolved = self
                .drivers
                .get(&net)
                .map(Self::resolve)
                .unwrap_or_default();
            self.settled.insert(net, resolved);
        }

        // Wake only what actually saw its input move.
        let disturbed: Vec<ComponentId> = self
            .reading_pins()
            .filter(|&(pin, net)| {
                let before = previously_read.get(&pin).cloned().unwrap_or_default();
                self.signal_at(net) != before
            })
            .map(|((component, _), _)| component)
            .collect();
        for component in disturbed {
            let delay = self.components[&component].component.propagation_delay();
            self.schedule_at(component, self.clock + delay);
        }
    }

    /// Every pin that reads (`Input` or `InOut`), with the net it's on.
    fn reading_pins(&self) -> impl Iterator<Item = ((ComponentId, usize), NetId)> + '_ {
        self.components.iter().flat_map(|(&component, registered)| {
            registered
                .pins
                .iter()
                .enumerate()
                .filter(|(_, pin)| pin.direction != PinDirection::Output)
                .map(move |(index, pin)| ((component, index), pin.net))
        })
    }

    /// Removes `component` from the circuit entirely (e.g. the user deleted it
    /// in the GUI). No-op if it's already gone. Any net it was driving is
    /// recomputed without its contribution, same as a normal output change —
    /// call [`Circuit::run`] afterward to process any resulting schedule. Nets
    /// it was only reading are unaffected: removing a reader doesn't change
    /// what its net carries.
    pub fn remove_component(&mut self, component: ComponentId) {
        let Some(registered) = self.components.remove(&component) else {
            return;
        };

        let driven_nets: Vec<NetId> = registered
            .pins
            .iter()
            .filter(|pin| pin.direction != PinDirection::Input)
            .map(|pin| pin.net)
            .collect();

        for net in driven_nets {
            if let Some(drivers) = self.drivers.get_mut(&net) {
                drivers.retain(|&(driver, _), _| driver != component);
            }
            self.resettle(net);
        }

        self.periodic.remove(&component);
    }

    /// The net's current resolved signal: `Unknown` if undriven, the shared value
    /// if every active driver agrees (ignoring `HighZ`), or `Error` if they disagree.
    pub fn signal_at(&self, net: NetId) -> Signal {
        self.settled.get(&net).cloned().unwrap_or_default()
    }

    /// The logical clock: how many ticks this circuit has been advanced by.
    ///
    /// Read-only, and deliberately not a wall-clock time — a tick is one
    /// propagation delay, so this is the unit everything else here is
    /// expressed in. What it is *for* is single-stepping: a caller that
    /// advances one tick at a time has no other way to say where it got to,
    /// and a step that changes nothing visible is otherwise indistinguishable
    /// from a step that did not happen.
    pub fn now(&self) -> u64 {
        self.clock
    }

    /// When the next thing is due to happen, or `None` if nothing is.
    ///
    /// For advancing straight to it instead of a tick at a time: between two
    /// beats of a clock there are dozens of ticks where nothing is scheduled
    /// at all, and crossing them one by one tells you nothing.
    ///
    /// Events naming a component that has since been removed are skipped —
    /// [`Circuit::advance`] drops them without evaluating anything, so
    /// reporting one would promise a step that changes nothing, which is the
    /// very thing this exists to avoid.
    pub fn next_event_tick(&self) -> Option<u64> {
        self.events
            .iter()
            .map(|Reverse(event)| event)
            .filter(|event| self.components.contains_key(&event.component))
            .map(|event| event.tick)
            .min()
    }

    /// Components with an `Input` or `InOut` pin connected to `net` — i.e. whose
    /// output may need recomputing when `net`'s signal changes.
    pub fn components_reading(&self, net: NetId) -> Vec<ComponentId> {
        self.components
            .iter()
            .filter(|(_, registered)| {
                registered
                    .pins
                    .iter()
                    .any(|pin| pin.net == net && pin.direction != PinDirection::Output)
            })
            .map(|(&id, _)| id)
            .collect()
    }

    /// Schedules `component` for (re-)evaluation as soon as possible. Used to
    /// inject external stimuli (e.g. a button pressed in the GUI) or to force the
    /// first evaluation of a source component that has no inputs.
    pub fn schedule_now(&mut self, component: ComponentId) {
        self.schedule_at(component, self.clock);
    }

    /// Queues one evaluation of `component` at `tick` — **never a second one
    /// at the same tick**.
    ///
    /// Evaluating a component twice at one instant is two state transitions
    /// in no time at all. For anything combinational it is merely wasted
    /// work, since the same inputs give the same outputs; for a component
    /// that carries state it is wrong. A `Clock` toggles on every
    /// evaluation, so a duplicate made it toggle twice per period and appear
    /// never to move — and it stayed doubled, because each evaluation
    /// reschedules itself.
    ///
    /// So the rule belongs here rather than in the callers. A caller asking
    /// twice means "make sure this is evaluated at that tick", and it is;
    /// asking is not the same as demanding it happen a second time.
    fn schedule_at(&mut self, component: ComponentId, tick: u64) {
        let event = ScheduledEvent { tick, component };
        if self.events.iter().any(|Reverse(queued)| *queued == event) {
            return;
        }
        self.events.push(Reverse(event));
    }

    /// Marks `component` as periodic and schedules its first evaluation now.
    /// After every evaluation, a periodic component reschedules itself
    /// `period` ticks later automatically, regardless of whether its output
    /// changed — this is what lets a [`crate::Clock`] keep ticking forever
    /// instead of settling. Because of that, [`Circuit::run`]/[`Circuit::advance`]
    /// never truly "finish" once a periodic component exists — advance by a
    /// bounded tick count tied to real elapsed time (once per UI frame, say)
    /// rather than relying on the queue emptying out.
    pub fn schedule_periodic(&mut self, component: ComponentId, period: u64) {
        self.periodic.insert(component, period);
        self.schedule_now(component);
    }

    /// Processes scheduled events in logical-time order up through
    /// `self.clock + ticks`, then stops — any events still further out
    /// (including a periodic component's next reschedule) stay queued for a
    /// later call.
    ///
    /// Returns an error if a net toggles more than [`MAX_TOGGLES_PER_NET`] times,
    /// meaning the circuit doesn't converge.
    pub fn advance(&mut self, ticks: u64) -> Result<(), UnstableCircuit> {
        let deadline = self.clock.saturating_add(ticks);
        let mut toggles: HashMap<NetId, u32> = HashMap::new();

        while let Some(Reverse(event)) = self.events.peek().copied() {
            if event.tick > deadline {
                break;
            }
            self.events.pop();
            // A component can be removed (e.g. deleted in the GUI) after being
            // scheduled but before its event is processed; just drop it.
            if !self.components.contains_key(&event.component) {
                continue;
            }
            self.clock = event.tick;
            self.evaluate(event.component, &mut toggles)?;
        }

        self.clock = self.clock.max(deadline);
        Ok(())
    }

    /// [`Circuit::advance`] by [`MAX_RUN_TICKS`] — in practice "settle
    /// completely" for a circuit with no periodic component, since real
    /// circuits converge in a handful of ticks. **Do not rely on this for a
    /// circuit that contains a `Clock`**: it returns promptly rather than
    /// hanging forever, but a fast-enough clock ticking up to a million times
    /// in one call will look indistinguishable from genuine instability and
    /// trip [`MAX_TOGGLES_PER_NET`] — this is the wrong tool for a clocked
    /// circuit either way; use [`Circuit::advance`] tied to real elapsed time
    /// instead.
    pub fn run(&mut self) -> Result<(), UnstableCircuit> {
        self.advance(MAX_RUN_TICKS)
    }

    fn evaluate(
        &mut self,
        component: ComponentId,
        toggles: &mut HashMap<NetId, u32>,
    ) -> Result<(), UnstableCircuit> {
        let pins = self.components[&component].pins.clone();

        let inputs: Vec<Signal> = pins
            .iter()
            .filter(|pin| pin.direction != PinDirection::Output)
            .map(|pin| self.signal_at(pin.net))
            .collect();

        let outputs = self.components[&component].component.eval(&inputs);

        let output_pins = pins
            .into_iter()
            .enumerate()
            .filter(|(_, pin)| pin.direction != PinDirection::Input);

        for ((index, pin), signal) in output_pins.zip(outputs) {
            self.drivers
                .entry(pin.net)
                .or_default()
                .insert((component, index), signal);
            let resolved = Self::resolve(&self.drivers[&pin.net]);
            let previous = self.settled.get(&pin.net).cloned().unwrap_or_default();

            if resolved == previous {
                continue;
            }
            self.settled.insert(pin.net, resolved);

            let toggle_count = toggles.entry(pin.net).or_insert(0);
            *toggle_count += 1;
            if *toggle_count > MAX_TOGGLES_PER_NET {
                return Err(UnstableCircuit { net: pin.net });
            }

            for reader in self.components_reading(pin.net) {
                let delay = self.components[&reader].component.propagation_delay();
                self.schedule_at(reader, self.clock + delay);
            }
        }

        if let Some(&period) = self.periodic.get(&component) {
            self.schedule_at(component, self.clock + period);
        }

        Ok(())
    }

    /// Recomputes `net`'s resolved signal from its current drivers and, if that
    /// changed its settled value, schedules readers to react — same rule as an
    /// output changing during [`Circuit::evaluate`], but for a structural edit
    /// (merge/disconnect) rather than a component re-evaluating. No oscillation
    /// accounting here: a single structural edit can't loop by itself.
    fn resettle(&mut self, net: NetId) {
        let resolved = self
            .drivers
            .get(&net)
            .map(Self::resolve)
            .unwrap_or_default();
        let previous = self.settled.get(&net).cloned().unwrap_or_default();
        if resolved == previous {
            return;
        }
        self.settled.insert(net, resolved);
        for reader in self.components_reading(net) {
            let delay = self.components[&reader].component.propagation_delay();
            self.schedule_at(reader, self.clock + delay);
        }
    }

    /// Resolves a net's signal from its current drivers: ignores `HighZ`, then
    /// 0 remaining -> `Unknown`, 1 (or several agreeing) -> that value,
    /// several disagreeing -> `Error`.
    /// Whether `net`'s level rests only on weak contributions — a level
    /// that is genuinely there, and that any full-strength driver would
    /// override.
    ///
    /// Separate from [`Circuit::signal_at`] on purpose: a lone NMOS passing
    /// a high really does deliver a logic 1, so reporting anything else
    /// would make a working circuit look broken. What's missing without this
    /// is any way to *see* that the 1 has no noise margin left, which is
    /// exactly the thing a pass-transistor mistake costs you.
    pub fn is_weakly_driven(&self, net: NetId) -> bool {
        let Some(drivers) = self.drivers.get(&net) else {
            return false;
        };
        let mut active = drivers
            .values()
            .filter(|signal| !signal.levels().iter().all(|&level| level == Level::HighZ))
            .peekable();
        active.peek().is_some() && active.all(|signal| signal.is_weak())
    }

    /// What a net carries, from everything driving it.
    ///
    /// Bit by bit: a bus is resolved one position at a time by exactly the
    /// rule a plain wire has always used, so nothing about the meaning
    /// changes with the width.
    ///
    /// The width is the widest contribution. Contributions of *different*
    /// widths are a fault in the drawing rather than in the levels — the
    /// bits nobody supplies come out `Error`, which is what makes a
    /// mismatched net visible on the wire.
    fn resolve(drivers: &HashMap<(ComponentId, usize), Signal>) -> Signal {
        let width = drivers
            .values()
            .map(|signal| signal.width())
            .max()
            .unwrap_or(1);
        let ragged = drivers.values().any(|signal| signal.width() != width);
        Signal::from_levels(
            (0..width)
                .map(|bit| {
                    if ragged {
                        return Level::Error;
                    }
                    Self::resolve_bit(
                        drivers
                            .values()
                            .filter_map(|signal| signal.levels().get(bit).copied()),
                    )
                })
                .collect(),
        )
    }

    /// One bit of [`Circuit::resolve`], which is the rule that has been here
    /// since the beginning.
    fn resolve_bit(contributions: impl Iterator<Item = Level>) -> Level {
        let active: Vec<Level> = contributions
            .filter(|&signal| signal != Level::HighZ)
            .collect();

        // A full-strength driver simply wins: that is what a pass
        // transistor's weakened level *means*, and it's what lets a
        // transmission gate work — the PMOS half drives a strong high that
        // overrides the NMOS half's weak one, instead of the two being
        // called a conflict.
        let strong: Vec<Level> = active.iter().copied().filter(|s| !s.is_weak()).collect();
        let deciding = if strong.is_empty() { &active } else { &strong };

        let mut levels = deciding.iter().map(|signal| signal.strengthened());
        let Some(first) = levels.next() else {
            return Level::Unknown;
        };
        if levels.all(|signal| signal == first) {
            // Never weak on the way out: a net carries a level, and only a
            // *contribution* carries a strength.
            first
        } else {
            Level::Error
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::scalar_eval;

    #[test]
    fn add_net_returns_distinct_ids() {
        let mut circuit = Circuit::new();
        let a = circuit.add_net();
        let b = circuit.add_net();
        assert_ne!(a, b);
    }

    #[test]
    fn undriven_net_is_unknown() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        assert_eq!(circuit.signal_at(net).only_level(), Level::Unknown);
    }

    struct NotGate;

    impl Component for NotGate {
        fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
            scalar_eval(inputs, |inputs| match inputs {
                [Level::High] => vec![Level::Low],
                [Level::Low] => vec![Level::High],
                _ => vec![Level::Unknown],
            })
        }
    }

    /// A source with no inputs: always drives `High` once scheduled.
    struct AlwaysHigh;

    impl Component for AlwaysHigh {
        fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
            scalar_eval(_inputs, |_inputs| vec![Level::High])
        }
    }

    #[test]
    fn add_component_returns_distinct_ids() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let pins = vec![Pin {
            direction: PinDirection::Output,
            net,
        }];
        let a = circuit.add_component(Box::new(NotGate), pins.clone());
        let b = circuit.add_component(Box::new(NotGate), pins);
        assert_ne!(a, b);
    }

    #[test]
    fn a_source_component_settles_its_net_once_scheduled() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let source = circuit.add_component(
            Box::new(AlwaysHigh),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_now(source);
        circuit.run().unwrap();

        assert_eq!(circuit.signal_at(net).only_level(), Level::High);
    }

    #[test]
    fn a_change_propagates_through_a_downstream_component() {
        let mut circuit = Circuit::new();
        let source_net = circuit.add_net();
        let inverted_net = circuit.add_net();

        let source = circuit.add_component(
            Box::new(AlwaysHigh),
            vec![Pin {
                direction: PinDirection::Output,
                net: source_net,
            }],
        );
        circuit.add_component(
            Box::new(NotGate),
            vec![
                Pin {
                    direction: PinDirection::Input,
                    net: source_net,
                },
                Pin {
                    direction: PinDirection::Output,
                    net: inverted_net,
                },
            ],
        );

        circuit.schedule_now(source);
        circuit.run().unwrap();

        assert_eq!(circuit.signal_at(source_net).only_level(), Level::High);
        assert_eq!(circuit.signal_at(inverted_net).only_level(), Level::Low);
    }

    #[test]
    fn components_reading_finds_components_with_an_input_on_the_net() {
        let mut circuit = Circuit::new();
        let input_net = circuit.add_net();
        let output_net = circuit.add_net();

        let not_gate = circuit.add_component(
            Box::new(NotGate),
            vec![
                Pin {
                    direction: PinDirection::Input,
                    net: input_net,
                },
                Pin {
                    direction: PinDirection::Output,
                    net: output_net,
                },
            ],
        );

        assert_eq!(circuit.components_reading(input_net), vec![not_gate]);
        assert_eq!(circuit.components_reading(output_net), vec![]);
    }

    /// A NOT gate whose output feeds its own input never settles: it's a
    /// one-stage ring oscillator, exactly the case `run` must detect instead of
    /// looping forever.
    struct Inverter;

    impl Component for Inverter {
        fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
            scalar_eval(inputs, |inputs| match inputs {
                [Level::High] => vec![Level::Low],
                _ => vec![Level::High],
            })
        }
    }

    #[test]
    fn a_self_looped_inverter_is_reported_as_unstable() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let inverter = circuit.add_component(
            Box::new(Inverter),
            vec![
                Pin {
                    direction: PinDirection::Input,
                    net,
                },
                Pin {
                    direction: PinDirection::Output,
                    net,
                },
            ],
        );

        circuit.schedule_now(inverter);

        assert_eq!(circuit.run(), Err(UnstableCircuit { net }));
    }

    use crate::components::{button::Button, led::Led};

    #[test]
    fn a_button_wired_to_a_led_through_a_net_reflects_presses() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();

        let (button_component, pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );
        circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net,
            }],
        );

        circuit.schedule_now(button);
        circuit.run().unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::Low);

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);
    }

    #[test]
    fn rewire_keeps_pins_connected_while_a_second_route_remains() {
        // The case a destructive merge can't get right: two separate wires
        // hold a button and a LED together, one is removed, and the pins
        // must stay connected because the other plainly still does the job.
        // Recomputed connectivity has nothing to undo — the group is simply
        // the same both times.
        let mut circuit = Circuit::new();
        // `rewire` reassigns these straight away; they only have to exist.
        let (button_net, led_net) = (circuit.add_net(), circuit.add_net());
        let (button_component, pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net: button_net,
            }],
        );
        let led = circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net: led_net,
            }],
        );

        // Both wires say the same thing, so one group names both pins.
        let connected = vec![vec![(button, 0), (led, 0)]];
        circuit.rewire(&connected);
        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().expect("settles");
        assert_eq!(
            circuit.signal_at(circuit.pins(led)[0].net).only_level(),
            Level::High
        );

        // Drop one of the two wires: the remaining one still groups them.
        circuit.rewire(&connected);
        circuit.run().expect("settles");
        assert_eq!(
            circuit.signal_at(circuit.pins(led)[0].net).only_level(),
            Level::High,
            "the surviving wire should still carry the button through"
        );
    }

    #[test]
    fn rewire_separates_pins_once_nothing_groups_them() {
        let mut circuit = Circuit::new();
        // `rewire` reassigns these straight away; they only have to exist.
        let (button_net, led_net) = (circuit.add_net(), circuit.add_net());
        let (button_component, pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net: button_net,
            }],
        );
        let led = circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net: led_net,
            }],
        );

        circuit.rewire(&[vec![(button, 0), (led, 0)]]);
        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().expect("settles");
        assert_eq!(
            circuit.signal_at(circuit.pins(led)[0].net).only_level(),
            Level::High
        );

        // Now nothing groups them: each pin gets a net of its own, and the
        // LED's goes back to undriven.
        circuit.rewire(&[]);
        circuit.run().expect("settles");
        assert_ne!(circuit.pins(button)[0].net, circuit.pins(led)[0].net);
        assert_eq!(
            circuit.signal_at(circuit.pins(led)[0].net).only_level(),
            Level::Unknown
        );
        // The button is still driving its own net -- a contribution follows
        // its pin rather than belonging to whichever net it sat on.
        assert_eq!(
            circuit.signal_at(circuit.pins(button)[0].net).only_level(),
            Level::High
        );
    }

    #[test]
    fn rewire_leaves_a_clocks_phase_alone() {
        // A clock has no inputs, so nothing about an edit elsewhere should
        // make it tick: re-evaluating one flips its output.
        let mut circuit = Circuit::new();
        let clock_net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net: clock_net,
            }],
        );
        circuit.schedule_now(clock);
        circuit.advance(1).expect("settles");
        let phase = circuit.signal_at(circuit.pins(clock)[0].net);

        circuit.rewire(&[]);

        assert_eq!(circuit.signal_at(circuit.pins(clock)[0].net), phase);
    }

    #[test]
    fn rewiring_a_pin_of_an_already_removed_component_is_ignored() {
        // What the GUI does when a component is deleted: the component goes
        // first, and a wire that was attached to it may still name the pin
        // it used to end on for one more rebuild. That has to read as
        // "nothing to wire", not bring the program down.
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let (button_component, _pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.remove_component(button);

        circuit.rewire(&[vec![(button, 0)]]);
        assert_eq!(circuit.try_pins(button), None);
    }

    #[test]
    fn rewire_leaves_the_rest_of_a_group_connected_when_one_pin_drops_out() {
        let mut circuit = Circuit::new();
        let button_net = circuit.add_net();
        let led_a_net = circuit.add_net();
        let led_b_net = circuit.add_net();

        let (button_component, pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net: button_net,
            }],
        );
        let led_a = circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net: led_a_net,
            }],
        );
        let led_b = circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net: led_b_net,
            }],
        );

        // One group holds all three: button, led_a and led_b.
        circuit.rewire(&[vec![(button, 0), (led_a, 0), (led_b, 0)]]);
        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().expect("settles");
        assert_eq!(
            circuit.signal_at(circuit.pins(button)[0].net).only_level(),
            Level::High
        );

        // led_a's wire to the group is deleted, so it no longer appears in
        // it: the button and led_b must stay connected to each other.
        circuit.rewire(&[vec![(button, 0), (led_b, 0)]]);
        circuit.run().expect("settles");
        let shared = circuit.pins(button)[0].net;
        assert_eq!(circuit.pins(led_b)[0].net, shared);
        assert_eq!(circuit.signal_at(shared).only_level(), Level::High);
        // led_a is on a net of its own now, driven by nobody.
        let led_a_net = circuit.pins(led_a)[0].net;
        assert_ne!(led_a_net, shared);
        assert_eq!(circuit.signal_at(led_a_net).only_level(), Level::Unknown);
    }

    #[test]
    fn removing_a_component_stops_it_driving_its_net() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();

        let (button_component, pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );
        circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net,
            }],
        );

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);

        circuit.remove_component(button);
        assert_eq!(circuit.signal_at(net).only_level(), Level::Unknown);
    }

    #[test]
    fn run_ignores_a_stale_event_for_a_removed_component() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let source = circuit.add_component(
            Box::new(AlwaysHigh),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_now(source);
        circuit.remove_component(source);

        // Must not panic on an event referencing a component that's now gone.
        assert_eq!(circuit.run(), Ok(()));
    }

    use crate::components::clock::Clock;

    #[test]
    fn a_periodic_clock_keeps_toggling_across_advance_calls() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_periodic(clock, 5);
        circuit.advance(0).unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);

        circuit.advance(5).unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::Low);

        circuit.advance(5).unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);
    }

    #[test]
    fn advance_leaves_a_clocks_next_tick_queued_for_later() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_periodic(clock, 100);
        circuit.advance(0).unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);

        // Advancing by far less than the period must not fast-forward it.
        circuit.advance(1).unwrap();
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);
    }

    #[test]
    fn the_next_event_is_the_soonest_one_that_will_actually_run() {
        let mut circuit = Circuit::new();
        assert_eq!(circuit.next_event_tick(), None, "nothing is pending");

        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );
        circuit.schedule_periodic(clock, 60);
        assert_eq!(circuit.next_event_tick(), Some(0));

        circuit.advance(1).expect("stable");
        assert_eq!(circuit.next_event_tick(), Some(60));

        // A component removed after being scheduled leaves its event behind;
        // `advance` drops it without evaluating anything, so reporting it
        // would promise a step that changes nothing.
        circuit.remove_component(clock);
        assert_eq!(circuit.next_event_tick(), None);
    }

    #[test]
    fn asking_twice_for_the_same_tick_evaluates_once() {
        // A `Clock` toggles on every evaluation, so how many times it was
        // evaluated can be read straight off its net.
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_now(clock);
        circuit.schedule_now(clock);
        circuit.advance(1).expect("stable");

        // Twice would have brought it back where it started, which is
        // indistinguishable from a clock that never ran at all.
        assert_eq!(circuit.signal_at(net).only_level(), Level::High);
    }

    #[test]
    fn the_logical_clock_counts_every_tick_advanced_through() {
        let mut circuit = Circuit::new();
        assert_eq!(circuit.now(), 0);

        // Idle ticks count as much as busy ones: an empty circuit still has
        // a time, and single-stepping through one has to say so.
        circuit.advance(1).expect("stable");
        assert_eq!(circuit.now(), 1);
        circuit.advance(7).expect("stable");
        assert_eq!(circuit.now(), 8);

        // And a tick where something actually happens leaves it in the same
        // place as one where nothing does.
        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );
        circuit.schedule_periodic(clock, 2);
        circuit.advance(1).expect("stable");
        assert_eq!(circuit.now(), 9);
    }

    #[test]
    fn run_terminates_instead_of_hanging_with_a_periodic_component() {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();
        let clock = circuit.add_component(
            Box::new(Clock::new()),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );

        circuit.schedule_periodic(clock, 1);

        // A period this tight will trip MAX_TOGGLES_PER_NET within
        // MAX_RUN_TICKS -- that's fine (run() is documented as the wrong
        // tool for a clocked circuit anyway). What this test actually
        // guards against is a hang: reaching this line at all is the point.
        let _ = circuit.run();
    }
}
