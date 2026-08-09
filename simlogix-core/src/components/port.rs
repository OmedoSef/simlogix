//! A circuit's boundary: the pins it exposes to whatever contains it.
//!
//! These have to be useful *before* anything instantiates the circuit — it
//! is opened and tested on its own long before it is reused, and a port that
//! only made sense inside a parent would make the circuit untestable exactly
//! while it's being written. So a driving port is something you set by hand,
//! and every port reads back what its net carries.
//!
//! When instantiation arrives, the parent supplies the values instead;
//! nothing here needs to change shape for that.

use std::cell::Cell;
use std::rc::Rc;

use crate::component::Component;
use crate::signal::Signal;

/// What a driving port is currently putting on its net.
///
/// Three positions rather than a `bool` because "not driving" is a real
/// third choice, not the absence of one — and it's the case worth testing,
/// since it's what an unconnected parent pin gives you.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PortLevel {
    /// Driving nothing. What that *means* depends on the port — see
    /// [`CircuitPort::input`] and [`CircuitPort::bidirectional`].
    #[default]
    Undriven,
    Low,
    High,
}

impl PortLevel {
    /// The next position of the click cycle: undriven, high, low, round
    /// again. `tri_state` off skips the undriven position entirely, so a
    /// port declared two-state can't be clicked into a third.
    pub fn next(self, tri_state: bool) -> Self {
        match (self, tri_state) {
            (Self::Undriven, _) => Self::High,
            (Self::High, _) => Self::Low,
            (Self::Low, true) => Self::Undriven,
            (Self::Low, false) => Self::High,
        }
    }
}

/// A port that drives its net, and can be told to stop.
///
/// The two constructors differ only in what "undriven" puts on the wire, and
/// that difference matters more than it looks — see each.
pub struct CircuitPort {
    /// What [`PortLevel::Undriven`] resolves to for this port.
    undriven: Signal,
    level: Rc<Cell<PortLevel>>,
}

impl CircuitPort {
    /// A value entering the circuit. Undriven is [`Signal::Unknown`]:
    /// nothing outside is supplying it, so its value genuinely isn't known.
    pub fn input() -> (Self, Rc<Cell<PortLevel>>) {
        Self::new(Signal::Unknown)
    }

    /// A port carrying values both ways. Undriven is [`Signal::HighZ`],
    /// *not* `Unknown`: it has to actually let go so the circuit inside can
    /// drive the net. `Unknown` counts as a driver, so it would put every
    /// net a bidirectional port touches into conflict instead of stepping
    /// aside.
    pub fn bidirectional() -> (Self, Rc<Cell<PortLevel>>) {
        Self::new(Signal::HighZ)
    }

    fn new(undriven: Signal) -> (Self, Rc<Cell<PortLevel>>) {
        let level = Rc::new(Cell::new(PortLevel::default()));
        (
            Self {
                undriven,
                level: Rc::clone(&level),
            },
            level,
        )
    }
}

impl Component for CircuitPort {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        vec![match self.level.get() {
            PortLevel::Undriven => self.undriven,
            PortLevel::Low => Signal::Low,
            PortLevel::High => Signal::High,
        }]
    }
}

/// A value leaving the circuit.
///
/// A pure sink, like [`crate::Led`]: it drives nothing, and what it carries
/// is read from the net its pin sits on. Nothing to evaluate — the value is
/// already there. It has no "three-state" choice of its own for the same
/// reason: it reads whatever is on the net, including nothing at all.
#[derive(Debug, Default, Clone, Copy)]
pub struct CircuitOutput;

impl Component for CircuitOutput {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        Vec::new()
    }
}

/// The pins an instance of a sub-circuit exposes to the circuit containing
/// it — one per port, driving nothing.
///
/// It exists only to give the parent's wires something to attach to, and to
/// be a point the net rebuild can union with the sub-circuit's internals.
/// The sub-circuit's own port components are *not* instantiated: a port's
/// pin is only ever a member of an inner net, so unioning this pin with that
/// net is the whole connection — and an input port that stayed alive would
/// fight whatever the parent drives into it.
pub struct CircuitAnchor {
    pins: usize,
}

impl CircuitAnchor {
    pub fn new(pins: usize) -> Self {
        Self { pins }
    }
}

impl Component for CircuitAnchor {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        vec![Signal::HighZ; self.pins]
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_holds_the_level_it_was_set_to() {
        let (port, level) = CircuitPort::input();
        // Undriven from the outside is exactly what "not known" means here.
        assert_eq!(port.eval(&[]), vec![Signal::Unknown]);

        level.set(PortLevel::High);
        assert_eq!(port.eval(&[]), vec![Signal::High]);
        // Latching, not momentary: nothing releases it.
        assert_eq!(port.eval(&[]), vec![Signal::High]);
    }

    #[test]
    fn an_undriven_bidirectional_port_steps_aside_rather_than_clouding_its_net() {
        let (port, level) = CircuitPort::bidirectional();
        // The difference from an input, and the reason the two exist:
        // `HighZ` is ignored when a net resolves, so the circuit inside can
        // drive it. `Unknown` would count as a driver and cause a conflict.
        assert_eq!(port.eval(&[]), vec![Signal::HighZ]);

        level.set(PortLevel::Low);
        assert_eq!(port.eval(&[]), vec![Signal::Low]);
    }

    #[test]
    fn the_click_cycle_skips_undriven_unless_the_port_is_three_state() {
        let mut level = PortLevel::Undriven;
        for expected in [PortLevel::High, PortLevel::Low, PortLevel::Undriven] {
            level = level.next(true);
            assert_eq!(level, expected);
        }

        // Two-state: it never reaches undriven again once it has left.
        let mut level = PortLevel::High;
        for expected in [PortLevel::Low, PortLevel::High, PortLevel::Low] {
            level = level.next(false);
            assert_eq!(level, expected);
        }
    }

    #[test]
    fn a_two_state_port_still_leaves_undriven_when_it_starts_there() {
        // Otherwise a port set to two-state *after* being left undriven
        // would be stuck there with no way to click out of it.
        assert_eq!(PortLevel::Undriven.next(false), PortLevel::High);
    }

    #[test]
    fn an_anchor_contributes_nothing_to_any_of_its_pins() {
        // Every pin is `InOut`, so the engine writes a value back to each;
        // all of them have to be `HighZ` or the instance would drive its own
        // ports.
        assert_eq!(
            CircuitAnchor::new(3).eval(&[Signal::High, Signal::Low, Signal::Unknown]),
            vec![Signal::HighZ; 3]
        );
    }

    #[test]
    fn an_output_drives_nothing() {
        assert!(CircuitOutput.eval(&[Signal::High]).is_empty());
    }
}
