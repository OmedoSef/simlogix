use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::component::Component;
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
    /// Each net's current contribution per driving component, so re-evaluating a
    /// component replaces only its own contribution, not every other driver's.
    drivers: HashMap<NetId, HashMap<ComponentId, Signal>>,
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

    /// Merges `from` into `to`: every pin currently wired to `from` (across every
    /// registered component) is rewired to `to` instead — this is what "drawing a
    /// wire" between two previously separate pins means at the model level.
    ///
    /// Any driver contribution already recorded on `from` carries over to `to`,
    /// and `to`'s resolved signal is recomputed immediately from the combined
    /// drivers (same rule as [`Circuit::signal_at`]). If that changes `to`'s
    /// value, components reading it are scheduled to react, same as a normal
    /// output change — call [`Circuit::run`] afterward to process those. No-op
    /// if `from == to`.
    pub fn merge_nets(&mut self, from: NetId, to: NetId) {
        if from == to {
            return;
        }

        for registered in self.components.values_mut() {
            for pin in &mut registered.pins {
                if pin.net == from {
                    pin.net = to;
                }
            }
        }

        if let Some(from_drivers) = self.drivers.remove(&from) {
            self.drivers.entry(to).or_default().extend(from_drivers);
        }
        self.settled.remove(&from);

        self.resettle(to);
    }

    /// Gives `component`'s pin at `pin_index` its own fresh, unconnected net —
    /// the inverse of [`Circuit::merge_nets`], used to delete a wire. Any other
    /// pins that were sharing the old net stay connected to each other; only
    /// this one pin is pulled out. Returns the freshly allocated net.
    ///
    /// If this pin was itself driving the old net (i.e. it's an `Output`/`InOut`
    /// pin that has been evaluated at least once), that contribution moves with
    /// it. Both the old and new net are recomputed immediately, same as
    /// [`Circuit::merge_nets`] — call [`Circuit::run`] afterward to process any
    /// resulting schedule.
    ///
    /// Returns `None` if there's no such pin, because `component` has already
    /// been removed or doesn't have that many pins. That's an ordinary case,
    /// not a caller mistake: deleting a component in the GUI also deletes the
    /// wires attached to it, and each of those still names the pin it used to
    /// end on.
    pub fn disconnect_pin(&mut self, component: ComponentId, pin_index: usize) -> Option<NetId> {
        let new_net = self.add_net();

        let old_net = {
            let pin = self
                .components
                .get_mut(&component)?
                .pins
                .get_mut(pin_index)?;
            std::mem::replace(&mut pin.net, new_net)
        };

        if let Some(drivers_on_old) = self.drivers.get_mut(&old_net) {
            if let Some(signal) = drivers_on_old.remove(&component) {
                self.drivers
                    .entry(new_net)
                    .or_default()
                    .insert(component, signal);
            }
        }

        self.resettle(old_net);
        self.resettle(new_net);

        Some(new_net)
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
                drivers.remove(&component);
            }
            self.resettle(net);
        }

        self.periodic.remove(&component);
    }

    /// The net's current resolved signal: `Unknown` if undriven, the shared value
    /// if every active driver agrees (ignoring `HighZ`), or `Error` if they disagree.
    pub fn signal_at(&self, net: NetId) -> Signal {
        self.settled.get(&net).copied().unwrap_or(Signal::Unknown)
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

    fn schedule_at(&mut self, component: ComponentId, tick: u64) {
        self.events
            .push(Reverse(ScheduledEvent { tick, component }));
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
            .filter(|pin| pin.direction != PinDirection::Input);

        for (pin, signal) in output_pins.zip(outputs) {
            self.drivers
                .entry(pin.net)
                .or_default()
                .insert(component, signal);
            let resolved = Self::resolve(&self.drivers[&pin.net]);
            let previous = self
                .settled
                .get(&pin.net)
                .copied()
                .unwrap_or(Signal::Unknown);

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
            .unwrap_or(Signal::Unknown);
        let previous = self.settled.get(&net).copied().unwrap_or(Signal::Unknown);
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
    fn resolve(drivers: &HashMap<ComponentId, Signal>) -> Signal {
        let mut active = drivers
            .values()
            .copied()
            .filter(|&signal| signal != Signal::HighZ);

        let Some(first) = active.next() else {
            return Signal::Unknown;
        };
        if active.all(|signal| signal == first) {
            first
        } else {
            Signal::Error
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(circuit.signal_at(net), Signal::Unknown);
    }

    struct NotGate;

    impl Component for NotGate {
        fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
            match inputs {
                [Signal::High] => vec![Signal::Low],
                [Signal::Low] => vec![Signal::High],
                _ => vec![Signal::Unknown],
            }
        }
    }

    /// A source with no inputs: always drives `High` once scheduled.
    struct AlwaysHigh;

    impl Component for AlwaysHigh {
        fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
            vec![Signal::High]
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

        assert_eq!(circuit.signal_at(net), Signal::High);
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

        assert_eq!(circuit.signal_at(source_net), Signal::High);
        assert_eq!(circuit.signal_at(inverted_net), Signal::Low);
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
            match inputs {
                [Signal::High] => vec![Signal::Low],
                _ => vec![Signal::High],
            }
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
        assert_eq!(circuit.signal_at(net), Signal::Low);

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        assert_eq!(circuit.signal_at(net), Signal::High);
    }

    #[test]
    fn merging_nets_lets_a_previously_unconnected_button_and_led_react() {
        let mut circuit = Circuit::new();
        let button_net = circuit.add_net();
        let led_net = circuit.add_net();

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

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        // Not connected yet: the LED's own net never sees the button's value.
        assert_eq!(circuit.signal_at(led_net), Signal::Unknown);

        circuit.merge_nets(button_net, led_net);
        assert_eq!(circuit.pins(button)[0].net, led_net);
        assert_eq!(circuit.pins(led)[0].net, led_net);

        // The button's already-driven value carries over the merge immediately,
        // without needing a fresh press.
        assert_eq!(circuit.signal_at(led_net), Signal::High);
    }

    #[test]
    fn disconnecting_a_pin_undoes_a_merge() {
        let mut circuit = Circuit::new();
        let button_net = circuit.add_net();
        let led_net = circuit.add_net();

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

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        circuit.merge_nets(button_net, led_net);
        assert_eq!(circuit.signal_at(led_net), Signal::High);

        let new_net = circuit.disconnect_pin(button, 0).expect("pin exists");

        // The button keeps its already-driven value on its new, private net...
        assert_eq!(circuit.pins(button)[0].net, new_net);
        assert_eq!(circuit.signal_at(new_net), Signal::High);
        // ...while the LED's net (still shared with nothing else now) reverts.
        assert_eq!(circuit.pins(led)[0].net, led_net);
        assert_eq!(circuit.signal_at(led_net), Signal::Unknown);
    }

    #[test]
    fn disconnecting_a_pin_of_an_already_removed_component_reports_nothing_to_do() {
        // What the GUI does when a component is deleted: the component goes
        // first, then every wire that was attached to it is torn down — and
        // each of those still names the pin it used to end on. That has to
        // read as "nothing to disconnect", not bring the program down.
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

        assert_eq!(circuit.disconnect_pin(button, 0), None);
        // Out of range on a component that *is* still there, likewise.
        assert_eq!(circuit.disconnect_pin(button, 99), None);
    }

    #[test]
    fn disconnecting_one_pin_leaves_the_others_on_a_shared_net_connected() {
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

        // Merge everyone onto button_net: button, led_a, and led_b all share it.
        circuit.merge_nets(led_a_net, button_net);
        circuit.merge_nets(led_b_net, button_net);

        pressed.set(true);
        circuit.schedule_now(button);
        circuit.run().unwrap();
        assert_eq!(circuit.signal_at(button_net), Signal::High);

        // Disconnect led_a (as if its wire to the group were deleted): the
        // button and led_b must remain connected to each other on button_net.
        let led_a_new_net = circuit.disconnect_pin(led_a, 0).expect("pin exists");
        assert_eq!(circuit.pins(button)[0].net, button_net);
        assert_eq!(circuit.pins(led_b)[0].net, button_net);
        assert_eq!(circuit.signal_at(button_net), Signal::High);
        // led_a's own net is now undriven.
        assert_eq!(circuit.pins(led_a)[0].net, led_a_new_net);
        assert_eq!(circuit.signal_at(led_a_new_net), Signal::Unknown);
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
        assert_eq!(circuit.signal_at(net), Signal::High);

        circuit.remove_component(button);
        assert_eq!(circuit.signal_at(net), Signal::Unknown);
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
        assert_eq!(circuit.signal_at(net), Signal::High);

        circuit.advance(5).unwrap();
        assert_eq!(circuit.signal_at(net), Signal::Low);

        circuit.advance(5).unwrap();
        assert_eq!(circuit.signal_at(net), Signal::High);
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
        assert_eq!(circuit.signal_at(net), Signal::High);

        // Advancing by far less than the period must not fast-forward it.
        circuit.advance(1).unwrap();
        assert_eq!(circuit.signal_at(net), Signal::High);
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
