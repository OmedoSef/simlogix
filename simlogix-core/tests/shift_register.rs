//! Three D flip-flops chained on one clock, which is the test that says the
//! *engine* supports edge-triggered storage rather than just the component.
//!
//! Every stage is clocked at the same instant, so a simulator that let an
//! output reach the next stage's input within the same edge would shift the
//! value all the way down the chain at once — the classic race, and the
//! reason real flip-flops are specified with a hold time. Here the
//! propagation delay is what prevents it: a stage's `Q` changes at `t + 1`,
//! by which time the edge has been and gone, so the stage after it captured
//! the value from *before*.
//!
//! An integration test rather than a unit one: the claim is about `Circuit`
//! and several components together, and handing `eval` its inputs by hand is
//! precisely what removes the thing being tested.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{Button, Circuit, ComponentId, DFlipFlop, Level, NetGroup, Pin, PinDirection};

/// How long to let the circuit settle after each change. Generous: three
/// stages at one tick each, and nothing here is periodic.
const SETTLE: u64 = 16;

struct ShiftRegister {
    circuit: Circuit,
    data: Rc<Cell<bool>>,
    clock: Rc<Cell<bool>>,
    data_button: ComponentId,
    clock_button: ComponentId,
    stages: Vec<ComponentId>,
}

impl ShiftRegister {
    fn new(length: usize) -> Self {
        let mut circuit = Circuit::new();

        let (data_source, data) = Button::new();
        let net = circuit.add_net();
        let data_button = circuit.add_component(Box::new(data_source), vec![out(net)]);
        let (clock_source, clock) = Button::new();
        let net = circuit.add_net();
        let clock_button = circuit.add_component(Box::new(clock_source), vec![out(net)]);

        // Whatever nets these start on, `rewire` below replaces the whole
        // mapping — connectivity is stated once, as the drawing states it.
        let stages: Vec<ComponentId> = (0..length)
            .map(|_| {
                let nets: Vec<_> = (0..4).map(|_| circuit.add_net()).collect();
                circuit.add_component(
                    // No asynchronous inputs: this is about the clock alone.
                    Box::new(DFlipFlop::rising()),
                    vec![input(nets[0]), input(nets[1]), out(nets[2]), out(nets[3])],
                )
            })
            .collect();

        // Every stage's clock on one net, so they are all clocked at the same
        // instant — which is what makes the race possible in the first place.
        let mut groups = vec![NetGroup::wire(
            std::iter::once((clock_button, 0))
                .chain(stages.iter().map(|&stage| (stage, 1)))
                .collect(),
        )];
        // `D` of the first stage from the button, then each `Q` to the next.
        groups.push(NetGroup::wire(vec![(data_button, 0), (stages[0], 0)]));
        for pair in stages.windows(2) {
            groups.push(NetGroup::wire(vec![(pair[0], 2), (pair[1], 0)]));
        }
        circuit.rewire(&groups);
        // Wake both sources, so their nets carry a level rather than nothing.
        // The application does this when a component is placed; without it
        // the clock has never been *seen* low, so its first rise is not a
        // transition and no flip-flop would call it an edge.
        circuit.schedule_now(data_button);
        circuit.schedule_now(clock_button);
        let _ = circuit.advance(SETTLE);

        Self {
            circuit,
            data,
            clock,
            data_button,
            clock_button,
            stages,
        }
    }

    fn set_data(&mut self, high: bool) {
        self.data.set(high);
        self.circuit.schedule_now(self.data_button);
        let _ = self.circuit.advance(SETTLE);
    }

    /// One complete clock cycle: up, then down. The capture happens on the
    /// way up; the way down is what makes the *next* rise an edge.
    fn pulse(&mut self) {
        for level in [true, false] {
            self.clock.set(level);
            self.circuit.schedule_now(self.clock_button);
            let _ = self.circuit.advance(SETTLE);
        }
    }

    /// What each stage is holding, first stage first.
    fn contents(&self) -> Vec<Level> {
        self.stages
            .iter()
            .map(|&stage| {
                self.circuit
                    .signal_at(self.circuit.pins(stage)[2].net)
                    .only_level()
            })
            .collect()
    }
}

fn out(net: simlogix_core::NetId) -> Pin {
    Pin {
        direction: PinDirection::Output,
        net,
    }
}

fn input(net: simlogix_core::NetId) -> Pin {
    Pin {
        direction: PinDirection::Input,
        net,
    }
}

#[test]
fn a_one_walks_down_the_chain_one_stage_per_edge() {
    use Level::{High, Low};
    let mut register = ShiftRegister::new(3);

    // Clear it: three edges with a low input, and every stage holds `Low`.
    // Until then they hold nothing at all, which is not a value to shift.
    register.set_data(false);
    for _ in 0..3 {
        register.pulse();
    }
    assert_eq!(register.contents(), vec![Low, Low, Low], "cleared");

    // One high input, one edge. If the engine let `Q0` reach `D1` within the
    // edge, all three would read `High` here — that is the whole test.
    register.set_data(true);
    register.pulse();
    assert_eq!(register.contents(), vec![High, Low, Low]);

    register.set_data(false);
    register.pulse();
    assert_eq!(
        register.contents(),
        vec![Low, High, Low],
        "it moved on by one"
    );

    register.pulse();
    assert_eq!(register.contents(), vec![Low, Low, High]);

    // And out the end: nothing is fed back in, so the chain empties.
    register.pulse();
    assert_eq!(register.contents(), vec![Low, Low, Low]);
}

#[test]
fn nothing_moves_without_an_edge() {
    use Level::{High, Low};
    let mut register = ShiftRegister::new(3);
    register.set_data(false);
    for _ in 0..3 {
        register.pulse();
    }
    register.set_data(true);
    register.pulse();
    assert_eq!(register.contents(), vec![High, Low, Low]);

    // The data moves about while the clock sits still. A flip-flop is not a
    // latch: none of this reaches `Q`.
    for high in [false, true, false, true] {
        register.set_data(high);
        assert_eq!(
            register.contents(),
            vec![High, Low, Low],
            "the clock never moved, so neither did anything else"
        );
    }
}
