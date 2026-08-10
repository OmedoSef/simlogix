//! A splitter in a real circuit, which is the only place its one hard
//! question can be asked.
//!
//! A splitter's pins both read and drive the very nets they are attached to,
//! so it hears whatever it has just said. Repeating that would leave it
//! holding a value after whatever really drove it has let go — a latch
//! nobody drew. `Component::reads_own_contribution` is the answer, and this
//! file is what says the answer works: the unit tests in `splitter.rs` hand
//! the component its inputs directly and so cannot see the echo at all.

use simlogix_core::{
    Circuit, CircuitPort, ComponentId, Level, NetGroup, Pin, PinDirection, PortDrive, PortHandles,
    Signal, Splitter,
};

/// A 4-bit bus split into two 2-bit branches, with a port able to drive the
/// bus and let go of it again.
struct Split {
    circuit: Circuit,
    port: ComponentId,
    handles: PortHandles,
    splitter: ComponentId,
}

impl Split {
    fn new() -> Self {
        let mut circuit = Circuit::new();
        let (bus, low, high) = (circuit.add_net(), circuit.add_net(), circuit.add_net());

        let splitter = circuit.add_component(
            Box::new(Splitter),
            vec![
                Pin {
                    direction: PinDirection::InOut,
                    net: bus,
                },
                Pin {
                    direction: PinDirection::InOut,
                    net: low,
                },
                Pin {
                    direction: PinDirection::InOut,
                    net: high,
                },
            ],
        );

        // A bidirectional port, so letting go really is `HighZ` — an input
        // port would drive `Unknown` and fight the splitter instead of
        // stepping out of the way.
        let (component, handles) = CircuitPort::bidirectional();
        handles.width.set(4);
        let port = circuit.add_component(
            Box::new(component),
            vec![Pin {
                direction: PinDirection::Output,
                net: bus,
            }],
        );

        circuit.rewire(&[
            NetGroup::bus(vec![(splitter, 0), (port, 0)], 4),
            NetGroup::bus(vec![(splitter, 1)], 2),
            NetGroup::bus(vec![(splitter, 2)], 2),
        ]);
        circuit.schedule_now(splitter);
        circuit.schedule_now(port);
        circuit.run().expect("a splitter settles");

        Self {
            circuit,
            port,
            handles,
            splitter,
        }
    }

    fn drive(&mut self, drive: PortDrive) {
        self.handles.drive.set(drive);
        self.circuit.schedule_now(self.port);
        self.circuit.run().expect("a splitter settles");
    }

    fn at(&self, pin: usize) -> Signal {
        self.circuit
            .signal_at(self.circuit.pins(self.splitter)[pin].net)
    }
}

#[test]
fn a_value_on_the_bus_arrives_on_the_branches_lowest_bits_first() {
    let mut split = Split::new();
    split.drive(PortDrive::Driving(0b1001));

    assert_eq!(split.at(1).levels(), [Level::High, Level::Low], "bits 0-1");
    assert_eq!(split.at(2).levels(), [Level::Low, Level::High], "bits 2-3");
}

#[test]
fn the_bus_is_let_go_of_when_whatever_drove_it_does() {
    // The whole reason a splitter must not hear itself. Without that, it
    // reads back its own contribution on the branches, relays it to the
    // bus, and goes on driving the old value for ever — a latch made of
    // two pieces of wire.
    let mut split = Split::new();
    split.drive(PortDrive::Driving(0b1001));
    assert_eq!(split.at(1).levels(), [Level::High, Level::Low]);

    split.drive(PortDrive::Undriven);

    for pin in 0..3 {
        let signal = split.at(pin);
        assert!(
            signal.levels().iter().all(|level| *level == Level::Unknown),
            "pin {pin} should be undriven once the port let go, and reads {signal:?}",
        );
    }
}

#[test]
fn a_value_travels_from_a_branch_to_the_bus_as_readily_as_the_other_way() {
    // Nothing says which way a splitter works: it falls out of what is
    // connected, which is why there is no separate merger.
    let mut circuit = Circuit::new();
    let (bus, low, high) = (circuit.add_net(), circuit.add_net(), circuit.add_net());

    let splitter = circuit.add_component(
        Box::new(Splitter),
        vec![
            Pin {
                direction: PinDirection::InOut,
                net: bus,
            },
            Pin {
                direction: PinDirection::InOut,
                net: low,
            },
            Pin {
                direction: PinDirection::InOut,
                net: high,
            },
        ],
    );

    let mut source = |net, width, value| {
        let (component, handles) = CircuitPort::bidirectional();
        handles.width.set(width);
        handles.drive.set(PortDrive::Driving(value));
        circuit.add_component(
            Box::new(component),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        )
    };
    let a = source(low, 2, 0b01);
    let b = source(high, 2, 0b10);

    circuit.rewire(&[
        NetGroup::bus(vec![(splitter, 0)], 4),
        NetGroup::bus(vec![(splitter, 1), (a, 0)], 2),
        NetGroup::bus(vec![(splitter, 2), (b, 0)], 2),
    ]);
    for component in [splitter, a, b] {
        circuit.schedule_now(component);
    }
    circuit.run().expect("a splitter settles");

    assert_eq!(
        circuit.signal_at(circuit.pins(splitter)[0].net).levels(),
        // 0b01 in the low half, 0b10 in the high half.
        [Level::High, Level::Low, Level::Low, Level::High],
    );
}

#[test]
fn a_splitter_between_two_live_drivers_faults_by_the_ordinary_net_rule() {
    // Nothing special happens here, and that is the point: a splitter
    // relaying one value onto a net that already carries another is two
    // drivers disagreeing, which `resolve` has always called `Error`.
    let mut split = Split::new();
    split.drive(PortDrive::Driving(0b0000));

    let branch = split.circuit.pins(split.splitter)[1].net;
    let (component, handles) = CircuitPort::bidirectional();
    handles.width.set(2);
    handles.drive.set(PortDrive::Driving(0b11));
    let fighting = split.circuit.add_component(
        Box::new(component),
        vec![Pin {
            direction: PinDirection::Output,
            net: branch,
        }],
    );
    split.circuit.rewire(&[
        NetGroup::bus(vec![(split.splitter, 0), (split.port, 0)], 4),
        NetGroup::bus(vec![(split.splitter, 1), (fighting, 0)], 2),
        NetGroup::bus(vec![(split.splitter, 2)], 2),
    ]);
    split.circuit.schedule_now(fighting);
    split
        .circuit
        .run()
        .expect("a disagreement is not instability");

    assert_eq!(split.at(1).levels(), [Level::Error, Level::Error]);
}
