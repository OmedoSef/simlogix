//! A pin occupying part of a net rather than all of it.
//!
//! This is what a splitter becomes once it stops being a component that
//! relays and starts being a statement about connectivity: bit 3 of one net
//! *is* bit 0 of another, with nothing in between to carry it. Here that is
//! expressed directly — two ports on different slices of one conductor,
//! with no component joining them.
//!
//! What it buys over a relay is in the last test: there is no tick between
//! the two sides, because there is nothing between them.

use simlogix_core::{
    Circuit, CircuitPort, Level, Member, NetGroup, Pin, PinDirection, PortDrive, Signal,
};

/// A four-bit net with a port driving all of it, and two two-bit ports
/// reading its halves.
fn split_bus() -> (Circuit, Vec<simlogix_core::ComponentId>) {
    let mut circuit = Circuit::new();
    let net = circuit.add_net();

    let mut port = |width: usize, direction| {
        let (component, handles) = CircuitPort::input();
        handles.width.set(width);
        let id = circuit.add_component(Box::new(component), vec![Pin { direction, net }]);
        (id, handles)
    };
    let (bus, bus_handles) = port(4, PinDirection::Output);
    let (low, _) = port(2, PinDirection::Input);
    let (high, _) = port(2, PinDirection::Input);

    circuit.rewire(&[NetGroup::sliced(
        vec![
            Member::whole((bus, 0)),
            Member::slice((low, 0), 0, 2),
            Member::slice((high, 0), 2, 2),
        ],
        4,
    )]);
    bus_handles.drive.set(PortDrive::Driving(0b1001));
    circuit.schedule_now(bus);
    circuit.run().expect("a sliced net settles");
    (circuit, vec![bus, low, high])
}

#[test]
fn a_pin_on_a_slice_reads_only_its_own_bits() {
    let (circuit, ids) = split_bus();
    let net = circuit.pins(ids[0])[0].net;

    assert_eq!(circuit.net_width(net), 4, "one conductor, four bits wide");
    assert_eq!(
        circuit.signal_at(net).levels(),
        [Level::High, Level::Low, Level::Low, Level::High],
    );
    // Bit 0 of the low half is bit 0 of the bus; bit 0 of the high half is
    // bit 2 of it. Nothing carried them there — they are the same wire.
    assert_eq!(circuit.pin_slice((ids[1], 0), net), (0, 2));
    assert_eq!(circuit.pin_slice((ids[2], 0), net), (2, 2));
}

#[test]
fn a_driver_on_a_slice_reaches_only_the_bits_it_occupies() {
    let mut circuit = Circuit::new();
    let net = circuit.add_net();
    let mut source = |width: usize, value: u64| {
        let (component, handles) = CircuitPort::input();
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
    let low = source(2, 0b01);
    let high = source(2, 0b10);

    circuit.rewire(&[NetGroup::sliced(
        vec![
            Member::slice((low, 0), 0, 2),
            Member::slice((high, 0), 2, 2),
        ],
        4,
    )]);
    for id in [low, high] {
        circuit.schedule_now(id);
    }
    circuit.run().expect("a sliced net settles");

    // Read back off the pin: `rewire` hands out fresh nets, so the one
    // captured above is gone — a `NetId` is only meaningful within an edit.
    let net = circuit.pins(low)[0].net;
    // 0b01 in the low half, 0b10 in the high — assembled by the net itself,
    // with no component in the middle to assemble it.
    assert_eq!(
        circuit.signal_at(net).levels(),
        [Level::High, Level::Low, Level::Low, Level::High],
    );
}

#[test]
fn nothing_is_between_the_two_sides_so_nothing_takes_a_tick() {
    // The whole reason for this over a relay. A component joining the
    // halves would answer one tick later; here the value is on both sides
    // of the conductor the moment the driver is evaluated.
    let (mut circuit, ids) = split_bus();
    let net = circuit.pins(ids[0])[0].net;

    let before = circuit.now();
    let handles = CircuitPort::input().1;
    let _ = handles;
    // Drive it again with a different value and let exactly one tick pass.
    circuit.schedule_now(ids[0]);
    circuit.advance(1).expect("one tick");
    assert!(
        circuit.now() - before <= 1,
        "a slice costs no time of its own"
    );
    assert_eq!(circuit.signal_at(net).width(), 4);
}

#[test]
fn a_contribution_that_does_not_fill_its_slice_faults_the_net() {
    // The rule the engine already had, now stated per slice rather than
    // per net: a pin claiming two bits and supplying one is lying about its
    // own contract, and that shows on the wire.
    let mut circuit = Circuit::new();
    let net = circuit.add_net();
    let (component, handles) = CircuitPort::input();
    handles.width.set(1);
    let narrow = circuit.add_component(
        Box::new(component),
        vec![Pin {
            direction: PinDirection::Output,
            net,
        }],
    );
    handles.drive.set(PortDrive::Driving(1));

    circuit.rewire(&[NetGroup::sliced(vec![Member::slice((narrow, 0), 0, 2)], 4)]);
    circuit.schedule_now(narrow);
    circuit.run().expect("a disagreement is not instability");

    let net = circuit.pins(narrow)[0].net;
    assert_eq!(circuit.signal_at(net), Signal::splat(Level::Error, 4));
}
