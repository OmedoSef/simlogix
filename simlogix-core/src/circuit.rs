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

/// A net toggling more than this many times within a single [`Circuit::run`] call
/// is considered unstable (e.g. a ring oscillator) rather than left to loop forever.
const MAX_TOGGLES_PER_NET: u32 = 1_000;

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
    pub fn pins(&self, component: ComponentId) -> &[Pin] {
        &self.components[&component].pins
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

    /// Processes scheduled events in logical-time order until none remain (the
    /// circuit has settled). Returns an error if a net toggles more than
    /// [`MAX_TOGGLES_PER_NET`] times, meaning the circuit doesn't converge.
    pub fn run(&mut self) -> Result<(), UnstableCircuit> {
        let mut toggles: HashMap<NetId, u32> = HashMap::new();

        while let Some(Reverse(event)) = self.events.pop() {
            self.clock = event.tick;
            self.evaluate(event.component, &mut toggles)?;
        }

        Ok(())
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

        Ok(())
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
}
