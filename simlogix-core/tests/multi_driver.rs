//! A shared bus: several drivers on one net, only one of them driving.
//!
//! `Circuit::resolve` has combined multiple drivers since early on, and
//! `Transistor` leans on it by going `HighZ` when it isn't conducting — but
//! nothing exercised the case the rule was actually written for: two
//! outputs deliberately wired together, taking turns. That's what a
//! tri-state buffer is for, and it's why `Signal::HighZ` is a state of its
//! own rather than a flavour of `Unknown`.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{Button, Circuit, ComponentId, Pin, PinDirection, Signal, TriStateBuffer};

/// Two tri-state buffers with their outputs tied together, each with a
/// button on its data input and another on its enable.
struct Bus {
    circuit: Circuit,
    buttons: Vec<ComponentId>,
    /// `[data_a, enable_a, data_b, enable_b]`.
    levers: Vec<Rc<Cell<bool>>>,
    shared: ComponentId,
}

impl Bus {
    /// Sets all four inputs and lets the circuit settle.
    fn drive(&mut self, values: [bool; 4]) {
        for (lever, value) in self.levers.iter().zip(values) {
            lever.set(value);
        }
        for &button in &self.buttons {
            self.circuit.schedule_now(button);
        }
        self.circuit.run().expect("a bus of buffers settles");
    }

    /// What the shared net resolves to.
    fn level(&self) -> Signal {
        self.circuit
            .signal_at(self.circuit.pins(self.shared)[2].net)
    }
}

fn build_bus() -> Bus {
    let mut circuit = Circuit::new();
    // `rewire` reassigns all of these; they only have to exist.
    let nets: Vec<_> = (0..10).map(|_| circuit.add_net()).collect();

    let mut buttons = Vec::new();
    let mut levers = Vec::new();
    for net in nets.iter().take(4) {
        let (component, lever) = Button::new();
        buttons.push(circuit.add_component(
            Box::new(component),
            vec![Pin {
                direction: PinDirection::Output,
                net: *net,
            }],
        ));
        levers.push(lever);
    }

    let buffer = |circuit: &mut Circuit, data, enable, out| {
        circuit.add_component(
            Box::new(TriStateBuffer),
            vec![
                Pin {
                    direction: PinDirection::Input,
                    net: data,
                },
                Pin {
                    direction: PinDirection::Input,
                    net: enable,
                },
                Pin {
                    direction: PinDirection::Output,
                    net: out,
                },
            ],
        )
    };
    let a = buffer(&mut circuit, nets[4], nets[5], nets[6]);
    let b = buffer(&mut circuit, nets[7], nets[8], nets[9]);

    circuit.rewire(&[
        vec![(buttons[0], 0), (a, 0)],
        vec![(buttons[1], 0), (a, 1)],
        vec![(buttons[2], 0), (b, 0)],
        vec![(buttons[3], 0), (b, 1)],
        // The bus itself: both outputs on one net.
        vec![(a, 2), (b, 2)],
    ]);

    Bus {
        circuit,
        buttons,
        levers,
        shared: a,
    }
}

#[test]
fn only_the_enabled_driver_decides_what_a_shared_net_carries() {
    let mut bus = build_bus();

    // A drives High, B is switched off.
    bus.drive([true, true, false, false]);
    assert_eq!(bus.level(), Signal::High);

    // Hand over: A off, B drives Low. The net follows B even though A's
    // data input hasn't changed — this is the whole trick.
    bus.drive([true, false, false, true]);
    assert_eq!(bus.level(), Signal::Low);
}

#[test]
fn a_net_nobody_is_driving_reads_as_unknown_rather_than_low() {
    let mut bus = build_bus();
    bus.drive([true, true, false, false]);

    // Both disabled: every driver reports `HighZ`, which resolution ignores,
    // leaving none. A floating bus is not the same thing as a bus held low,
    // and reporting `Low` here would invent a pull-down that isn't drawn.
    bus.drive([true, false, false, false]);
    assert_eq!(bus.level(), Signal::Unknown);
}

#[test]
fn two_drivers_fighting_over_a_net_is_reported_as_an_error() {
    let mut bus = build_bus();

    // Both enabled, disagreeing: in hardware this is a short between a
    // driver pulling up and one pulling down.
    bus.drive([true, true, false, true]);
    assert_eq!(bus.level(), Signal::Error);
}

#[test]
fn two_drivers_agreeing_is_not_an_error() {
    let mut bus = build_bus();

    // Redundant, wasteful, and harmless — there's nothing to report, so
    // the net carries the value both of them agree on.
    bus.drive([true, true, true, true]);
    assert_eq!(bus.level(), Signal::High);
}
