//! A ripple counter built out of T flip-flops, which is what the component
//! exists for — and the thing a primitive counter would be built from.
//!
//! Each stage is clocked by the one before it rather than by the common
//! clock, so the stages do not change together: that is what "ripple" means,
//! and it is visible here as the count arriving correct only once every
//! stage has settled. An integration test rather than a unit one, because
//! the claim is about several components and the engine's ordering, not
//! about one truth table.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{
    Button, Circuit, ComponentId, Level, NetGroup, NetId, Pin, PinDirection, Rail, TFlipFlop,
};

const SETTLE: u64 = 32;

struct Counter {
    circuit: Circuit,
    clock: Rc<Cell<bool>>,
    clock_button: ComponentId,
    clear: Rc<Cell<bool>>,
    clear_button: ComponentId,
    stages: Vec<ComponentId>,
}

impl Counter {
    /// `bits` stages, each clocked by the one below it.
    ///
    /// Falling-edge stages, which is what makes this count *up*: a stage
    /// toggles when the one below it goes high-to-low, and that is the carry.
    /// Clock them on the rising edge instead and the same wiring counts down.
    fn new(bits: usize) -> Self {
        let mut circuit = Circuit::new();

        let net = circuit.add_net();
        let high = circuit.add_component(Box::new(Rail::power()), vec![out(net)]);
        let net = circuit.add_net();
        let low = circuit.add_component(Box::new(Rail::ground()), vec![out(net)]);

        let (clock_source, clock) = Button::new();
        let net = circuit.add_net();
        let clock_button = circuit.add_component(Box::new(clock_source), vec![out(net)]);
        let (clear_source, clear) = Button::new();
        let net = circuit.add_net();
        let clear_button = circuit.add_component(Box::new(clear_source), vec![out(net)]);

        // `T`, clock, `S`, `R`, then `Q` and `Q̄`.
        let stages: Vec<ComponentId> = (0..bits)
            .map(|_| {
                let nets: Vec<NetId> = (0..6).map(|_| circuit.add_net()).collect();
                circuit.add_component(
                    Box::new(TFlipFlop::falling()),
                    vec![
                        input(nets[0]),
                        input(nets[1]),
                        input(nets[2]),
                        input(nets[3]),
                        out(nets[4]),
                        out(nets[5]),
                    ],
                )
            })
            .collect();

        let mut groups = vec![
            // Every `T` tied high: the counter counts, always.
            NetGroup::wire(
                std::iter::once((high, 0))
                    .chain(stages.iter().map(|&stage| (stage, 0)))
                    .collect(),
            ),
            // Every `S` tied low, so only the clear can force anything.
            NetGroup::wire(
                std::iter::once((low, 0))
                    .chain(stages.iter().map(|&stage| (stage, 2)))
                    .collect(),
            ),
            NetGroup::wire(
                std::iter::once((clear_button, 0))
                    .chain(stages.iter().map(|&stage| (stage, 3)))
                    .collect(),
            ),
            // The first stage takes the real clock.
            NetGroup::wire(vec![(clock_button, 0), (stages[0], 1)]),
        ];
        // And every other stage is clocked by the `Q` below it — the ripple.
        for pair in stages.windows(2) {
            groups.push(NetGroup::wire(vec![(pair[0], 4), (pair[1], 1)]));
        }
        circuit.rewire(&groups);

        // Wake the sources, so their nets carry a level rather than nothing:
        // a clock never *seen* low has not risen when it goes high.
        for source in [high, low, clock_button, clear_button] {
            circuit.schedule_now(source);
        }
        let _ = circuit.advance(SETTLE);

        Self {
            circuit,
            clock,
            clock_button,
            clear,
            clear_button,
            stages,
        }
    }

    fn set(&mut self, cell: bool, which: Which) {
        match which {
            Which::Clock => {
                self.clock.set(cell);
                self.circuit.schedule_now(self.clock_button);
            }
            Which::Clear => {
                self.clear.set(cell);
                self.circuit.schedule_now(self.clear_button);
            }
        }
        let _ = self.circuit.advance(SETTLE);
    }

    fn pulse(&mut self) {
        self.set(true, Which::Clock);
        self.set(false, Which::Clock);
    }

    fn clear(&mut self) {
        self.set(true, Which::Clear);
        self.set(false, Which::Clear);
    }

    /// The count, least significant stage first, or `None` if any stage is
    /// not holding a definite level.
    fn value(&self) -> Option<u64> {
        let mut total = 0;
        for (index, &stage) in self.stages.iter().enumerate() {
            match self
                .circuit
                .signal_at(self.circuit.pins(stage)[4].net)
                .only_level()
            {
                Level::High => total |= 1 << index,
                Level::Low => {}
                _ => return None,
            }
        }
        Some(total)
    }
}

enum Which {
    Clock,
    Clear,
}

fn out(net: NetId) -> Pin {
    Pin {
        direction: PinDirection::Output,
        net,
    }
}

fn input(net: NetId) -> Pin {
    Pin {
        direction: PinDirection::Input,
        net,
    }
}

#[test]
fn three_t_flip_flops_count_in_binary() {
    let mut counter = Counter::new(3);
    assert_eq!(
        counter.value(),
        None,
        "nothing has been told to any of them yet"
    );

    counter.clear();
    assert_eq!(counter.value(), Some(0));

    // All eight values, then round again: the wrap is the interesting part,
    // since it is the carry rippling through every stage at once.
    for expected in [1, 2, 3, 4, 5, 6, 7, 0, 1, 2] {
        counter.pulse();
        assert_eq!(counter.value(), Some(expected));
    }
}

#[test]
fn the_clear_is_asynchronous() {
    let mut counter = Counter::new(3);
    counter.clear();
    for _ in 0..5 {
        counter.pulse();
    }
    assert_eq!(counter.value(), Some(5));

    // No clock edge anywhere in this: that is what asynchronous means, and
    // it is what makes a reset line usable.
    counter.set(true, Which::Clear);
    assert_eq!(counter.value(), Some(0));
    counter.set(false, Which::Clear);
    assert_eq!(counter.value(), Some(0), "and it stays cleared");
}
