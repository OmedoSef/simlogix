//! A CMOS NAND built from four transistors — two PMOS in parallel to VDD,
//! two NMOS in series to ground.
//!
//! Reproduces what Romain hit in `rv_base:gates/nand`: with `A` high and `B`
//! low, the output came out `Error`.
//!
//! The cause was not the circuit. With `B` low the lower NMOS is off, so the
//! node between the two is driven by nobody — `Unknown`, which is what a net
//! with no driver resolves to. The upper NMOS *is* conducting, and it used to
//! pass that `Unknown` on to the output, where it met the `High` the PMOS was
//! putting there and the two resolved to `Error`. A switch connected to
//! nothing now conducts nothing: it reports `HighZ` instead.
//!
//! Nothing about this is specific to a NAND. Any structure with transistors
//! in series is hit whenever the one nearer the rail is off and the one
//! nearer the output is on — hence the NOR below, which is the mirror.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{
    Button, Circuit, ComponentId, Level, NetId, Pin, PinDirection, Rail, Transistor,
};

struct Nand {
    circuit: Circuit,
    inputs: Vec<ComponentId>,
    levers: Vec<Rc<Cell<bool>>>,
    y: NetId,
    /// The node between the two series NMOS — nothing but transistors touch
    /// it, which turns out to be the whole story.
    mid: NetId,
}

impl Nand {
    fn drive(&mut self, a: bool, b: bool) {
        self.levers[0].set(a);
        self.levers[1].set(b);
        for &id in &self.inputs {
            self.circuit.schedule_now(id);
        }
        self.circuit.run().expect("a NAND settles");
    }
}

fn build() -> Nand {
    let mut circuit = Circuit::new();

    let vdd = circuit.add_net();
    let gnd = circuit.add_net();
    let a = circuit.add_net();
    let b = circuit.add_net();
    let mid = circuit.add_net();
    let y = circuit.add_net();

    let out = |net| Pin {
        direction: PinDirection::Output,
        net,
    };
    let inp = |net| Pin {
        direction: PinDirection::Input,
        net,
    };

    // Adding a component doesn't evaluate it; the rails have to be told to
    // drive once, after which nothing disturbs them.
    let power = circuit.add_component(Box::new(Rail::power()), vec![out(vdd)]);
    let ground = circuit.add_component(Box::new(Rail::ground()), vec![out(gnd)]);
    circuit.schedule_now(power);
    circuit.schedule_now(ground);

    let mut inputs = Vec::new();
    let mut levers = Vec::new();
    for net in [a, b] {
        let (button, lever) = Button::new();
        inputs.push(circuit.add_component(Box::new(button), vec![out(net)]));
        levers.push(lever);
    }

    // Pins are gate, source, drain.
    let fet = |circuit: &mut Circuit, kind: Transistor, gate, source, drain| {
        let id = circuit.add_component(Box::new(kind), vec![inp(gate), inp(source), out(drain)]);
        circuit.schedule_now(id);
    };
    // Pull-up: either PMOS on its own can raise Y.
    fet(&mut circuit, Transistor::p_type(), a, vdd, y);
    fet(&mut circuit, Transistor::p_type(), b, vdd, y);
    // Pull-down: both NMOS in series, so both inputs must be high.
    fet(&mut circuit, Transistor::n_type(), b, gnd, mid);
    fet(&mut circuit, Transistor::n_type(), a, mid, y);

    Nand {
        circuit,
        inputs,
        levers,
        y,
        mid,
    }
}

#[test]
fn the_four_rows_of_a_nand() {
    let mut nand = build();
    for (a, b, expected) in [
        (false, false, Level::High),
        (false, true, Level::High),
        (true, false, Level::High),
        (true, true, Level::Low),
    ] {
        nand.drive(a, b);
        assert_eq!(nand.circuit.signal_at(nand.y), expected, "A={a}, B={b}");
    }
}

#[test]
fn the_series_node_is_undriven_when_the_lower_transistor_is_off() {
    // The mechanism, isolated. With B low the lower NMOS is off, so nothing
    // drives the node between the two — and `Unknown` is exactly what a net
    // with no driver resolves to.
    let mut nand = build();
    nand.drive(true, false);
    assert_eq!(nand.circuit.signal_at(nand.mid), Level::Unknown);
}

/// The mirror: two PMOS in series to VDD, two NMOS in parallel to ground.
///
/// Here it is the *pull-up* chain that breaks, so the same fault appeared on
/// the row where a NAND was fine.
fn build_nor() -> Nand {
    let mut circuit = Circuit::new();

    let vdd = circuit.add_net();
    let gnd = circuit.add_net();
    let a = circuit.add_net();
    let b = circuit.add_net();
    let mid = circuit.add_net();
    let y = circuit.add_net();

    let out = |net| Pin {
        direction: PinDirection::Output,
        net,
    };
    let inp = |net| Pin {
        direction: PinDirection::Input,
        net,
    };

    let power = circuit.add_component(Box::new(Rail::power()), vec![out(vdd)]);
    let ground = circuit.add_component(Box::new(Rail::ground()), vec![out(gnd)]);
    circuit.schedule_now(power);
    circuit.schedule_now(ground);

    let mut inputs = Vec::new();
    let mut levers = Vec::new();
    for net in [a, b] {
        let (button, lever) = Button::new();
        inputs.push(circuit.add_component(Box::new(button), vec![out(net)]));
        levers.push(lever);
    }

    let fet = |circuit: &mut Circuit, kind: Transistor, gate, source, drain| {
        let id = circuit.add_component(Box::new(kind), vec![inp(gate), inp(source), out(drain)]);
        circuit.schedule_now(id);
    };
    // Pull-up in series: both inputs must be low to raise Y.
    fet(&mut circuit, Transistor::p_type(), a, vdd, mid);
    fet(&mut circuit, Transistor::p_type(), b, mid, y);
    // Pull-down in parallel: either input alone drags Y down.
    fet(&mut circuit, Transistor::n_type(), a, gnd, y);
    fet(&mut circuit, Transistor::n_type(), b, gnd, y);

    Nand {
        circuit,
        inputs,
        levers,
        y,
        mid,
    }
}

#[test]
fn the_four_rows_of_a_nor() {
    let mut nor = build_nor();
    for (a, b, expected) in [
        (false, false, Level::High),
        (false, true, Level::Low),
        (true, false, Level::Low),
        (true, true, Level::Low),
    ] {
        nor.drive(a, b);
        assert_eq!(nor.circuit.signal_at(nor.y), expected, "A={a}, B={b}");
    }
}
