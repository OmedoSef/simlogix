//! The feedback-loop behaviour the whole discrete-event engine exists for.
//!
//! This is the project's founding argument, from `CLAUDE.md`: a circuit with
//! combinational feedback — an SR-NAND latch, a ring oscillator — must not
//! hang or crash the simulator the way it does in Logisim. Every component
//! has a propagation delay, so an input change *schedules* an output change
//! instead of recursing into one, and a loop is processed in time order like
//! any other event.
//!
//! Integration tests rather than unit ones: they exercise `Circuit` and
//! several concrete components together, through the public API only. That's
//! why they sit here rather than in a `#[cfg(test)] mod tests` at the bottom
//! of one file — there isn't a single file they belong to.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{
    Button, Circuit, ComponentId, Level, Nand, NetGroup, Nor, Not, Pin, PinDirection,
};

/// A cross-coupled NAND latch, with a button on each of its two active-low
/// inputs so a test can drive them.
struct Latch {
    circuit: Circuit,
    set: Rc<Cell<bool>>,
    reset: Rc<Cell<bool>>,
    set_button: ComponentId,
    reset_button: ComponentId,
    /// The NAND whose output is `Q`.
    q_gate: ComponentId,
    q_bar_gate: ComponentId,
}

impl Latch {
    /// Drives both inputs and lets the circuit settle.
    ///
    /// A button drives `High` while pressed, and these inputs are active
    /// *low*, so `true` here means "not asserted".
    fn drive(&mut self, set: bool, reset: bool) -> Result<(), simlogix_core::UnstableCircuit> {
        self.set.set(set);
        self.reset.set(reset);
        self.circuit.schedule_now(self.set_button);
        self.circuit.schedule_now(self.reset_button);
        self.circuit.run()
    }

    fn q(&self) -> Level {
        self.circuit
            .signal_at(self.circuit.pins(self.q_gate)[2].net)
            .only_level()
    }

    fn q_bar(&self) -> Level {
        self.circuit
            .signal_at(self.circuit.pins(self.q_bar_gate)[2].net)
            .only_level()
    }
}

/// ```text
///  S̄ ──┬── NAND ──┬── Q
///      │    ↑      │
///      │    └──────┼───┐
///      │           │   │
///  R̄ ──┼── NAND ───┘   │   (each NAND's second input is the other's output)
///      │    ↑          │
///      └────┴──────────┘
/// ```
fn build_latch() -> Latch {
    let mut circuit = Circuit::new();
    // `rewire` reassigns every one of these below; they only have to exist.
    let nets: Vec<_> = (0..8).map(|_| circuit.add_net()).collect();

    let (set_component, set) = Button::new();
    let set_button = circuit.add_component(
        Box::new(set_component),
        vec![Pin {
            direction: PinDirection::Output,
            net: nets[0],
        }],
    );
    let (reset_component, reset) = Button::new();
    let reset_button = circuit.add_component(
        Box::new(reset_component),
        vec![Pin {
            direction: PinDirection::Output,
            net: nets[1],
        }],
    );

    // Inputs at 0 and 1, output at 2 — the order the GUI registers a
    // 2-input gate in, so the test exercises the same shape.
    let q_gate = circuit.add_component(
        Box::new(Nand),
        vec![
            Pin {
                direction: PinDirection::Input,
                net: nets[2],
            },
            Pin {
                direction: PinDirection::Input,
                net: nets[3],
            },
            Pin {
                direction: PinDirection::Output,
                net: nets[4],
            },
        ],
    );
    let q_bar_gate = circuit.add_component(
        Box::new(Nand),
        vec![
            Pin {
                direction: PinDirection::Input,
                net: nets[5],
            },
            Pin {
                direction: PinDirection::Input,
                net: nets[6],
            },
            Pin {
                direction: PinDirection::Output,
                net: nets[7],
            },
        ],
    );

    circuit.rewire(&[
        NetGroup::wire(vec![(set_button, 0), (q_gate, 0)]),
        NetGroup::wire(vec![(reset_button, 0), (q_bar_gate, 0)]),
        // Q feeds the other gate's second input, and vice versa. These two
        // groups are the feedback loop.
        NetGroup::wire(vec![(q_gate, 2), (q_bar_gate, 1)]),
        NetGroup::wire(vec![(q_bar_gate, 2), (q_gate, 1)]),
    ]);

    Latch {
        circuit,
        set,
        reset,
        set_button,
        reset_button,
        q_gate,
        q_bar_gate,
    }
}

#[test]
fn an_sr_nand_latch_holds_its_value_when_both_inputs_are_released() {
    let mut latch = build_latch();

    // Assert set (active low), leave reset alone.
    latch.drive(false, true).expect("a latch settles");
    assert_eq!(latch.q(), Level::High, "asserting set should set Q");

    // Release it: both inputs now read the same, and the only thing that can
    // decide Q is what the loop is already holding.
    latch.drive(true, true).expect("a latch settles");
    assert_eq!(
        latch.q(),
        Level::High,
        "Q should hold after set is released"
    );

    latch.drive(true, false).expect("a latch settles");
    assert_eq!(latch.q(), Level::Low, "asserting reset should clear Q");

    // The point of the whole test: identical inputs to two lines above, and
    // the opposite output. That is memory, produced by nothing but a
    // combinational loop and a propagation delay -- which is exactly what an
    // engine that propagates instantly cannot do.
    latch.drive(true, true).expect("a latch settles");
    assert_eq!(
        latch.q(),
        Level::Low,
        "Q should hold after reset is released"
    );
}

#[test]
fn a_latch_released_from_its_forbidden_state_settles_into_a_legal_one() {
    let mut latch = build_latch();

    // Both inputs asserted at once drives both outputs High — the state the
    // truth table calls forbidden, since Q and Q̄ are meant to be
    // complementary.
    latch.drive(false, false).expect("a latch settles");
    assert_eq!(latch.q(), Level::High);
    assert_eq!(latch.q_bar(), Level::High);

    // Releasing both at the same instant is the classic race. Real hardware
    // resolves it on the difference between two gates that are never quite
    // identical, and which way it falls is unpredictable.
    latch
        .drive(true, true)
        .expect("the race should resolve, not hang");

    // What's asserted is that it left the forbidden state for a legal one —
    // deliberately not *which* one. The engine breaks the tie by the order
    // it happens to evaluate the two gates in: deterministic, but arbitrary,
    // and pinning the value here would pin an implementation detail rather
    // than a promise.
    let (q, q_bar) = (latch.q(), latch.q_bar());
    assert!(
        (q == Level::High && q_bar == Level::Low) || (q == Level::Low && q_bar == Level::High),
        "expected complementary outputs, got q={q:?} q_bar={q_bar:?}"
    );
}

/// A three-inversion ring, with a `Nor` as the enable gate.
///
/// A plain ring of inverters would be useless here: `Not(Unknown)` is
/// `Unknown`, so a ring that starts out undriven stays undriven forever and
/// a test on it would pass without ever oscillating. Real silicon starts on
/// noise; this needs a definite value put in on purpose. Holding the enable
/// `High` forces the `Nor` output `Low`, which seeds the ring; dropping it
/// turns the `Nor` into a third inverter and the ring runs.
fn build_ring_oscillator() -> (Circuit, ComponentId, Rc<Cell<bool>>, ComponentId) {
    let mut circuit = Circuit::new();
    let enable_net = circuit.add_net();
    let a = circuit.add_net();
    let b = circuit.add_net();
    let feedback = circuit.add_net();

    let (button, enable) = Button::new();
    let enable_button = circuit.add_component(
        Box::new(button),
        vec![Pin {
            direction: PinDirection::Output,
            net: enable_net,
        }],
    );
    let nor = circuit.add_component(
        Box::new(Nor),
        vec![
            Pin {
                direction: PinDirection::Input,
                net: enable_net,
            },
            Pin {
                direction: PinDirection::Input,
                net: feedback,
            },
            Pin {
                direction: PinDirection::Output,
                net: a,
            },
        ],
    );
    circuit.add_component(
        Box::new(Not),
        vec![
            Pin {
                direction: PinDirection::Input,
                net: a,
            },
            Pin {
                direction: PinDirection::Output,
                net: b,
            },
        ],
    );
    circuit.add_component(
        Box::new(Not),
        vec![
            Pin {
                direction: PinDirection::Input,
                net: b,
            },
            Pin {
                direction: PinDirection::Output,
                net: feedback,
            },
        ],
    );

    // Pins were declared straight onto shared nets, so the ring is already
    // wired -- no `rewire` needed.
    (circuit, enable_button, enable, nor)
}

/// Starts the ring running and returns it, mid-oscillation.
fn started_ring() -> (Circuit, ComponentId) {
    let (mut circuit, enable_button, enable, nor) = build_ring_oscillator();

    enable.set(true);
    circuit.schedule_now(enable_button);
    circuit.run().expect("held in reset, the ring settles");
    let output = circuit.pins(nor)[2].net;
    assert_eq!(
        circuit.signal_at(output).only_level(),
        Level::Low,
        "the enable should have seeded the ring with a definite value"
    );

    enable.set(false);
    circuit.schedule_now(enable_button);
    (circuit, nor)
}

#[test]
fn a_ring_oscillator_oscillates_instead_of_hanging() {
    let (mut circuit, nor) = started_ring();
    let output = circuit.pins(nor)[2].net;

    // Advanced in small steps, which is how the GUI drives it: a few ticks
    // of real elapsed time per frame.
    let mut seen = Vec::new();
    for _ in 0..12 {
        circuit
            .advance(4)
            .expect("small steps should never look unstable");
        seen.push(circuit.signal_at(output).only_level());
    }

    assert!(
        seen.contains(&Level::High) && seen.contains(&Level::Low),
        "the ring should be swinging between both levels, saw {seen:?}"
    );
}

#[test]
fn draining_a_ring_oscillator_is_reported_as_unstable_rather_than_running_forever() {
    let (mut circuit, _) = started_ring();

    // `run()` is `advance(MAX_RUN_TICKS)`, and the toggle counter is per
    // call -- so a million ticks of a perfectly healthy oscillator look
    // exactly like a circuit that refuses to settle. This is the documented
    // trade-off, pinned here so a change to it shows up as a failing test
    // rather than as a surprise: `run()` is the wrong tool for anything that
    // oscillates on purpose, and the GUI uses `advance` for that reason.
    assert!(
        circuit.run().is_err(),
        "draining an oscillator should stop and report, not loop forever"
    );
}
