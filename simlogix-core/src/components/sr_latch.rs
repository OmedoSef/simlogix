use std::cell::Cell;

use crate::component::Component;
use crate::signal::Signal;

/// An SR latch: `Set` drives `Q` high, `Reset` drives it low, and with
/// neither asserted it holds whatever it was last told.
///
/// A primitive rather than two cross-coupled NAND gates, even though the
/// gate-level version is drawable by hand and is what `tests/feedback.rs`
/// exercises. Two reasons: it's usable before sub-circuits exist, and its
/// inputs are active *high*, which is the abstraction the symbol promises
/// rather than the accident of building it out of NANDs.
///
/// Like [`crate::Clock`], and unlike every gate, this remembers something
/// between evaluations — `Component::eval` takes `&self`, so the state lives
/// behind a `Cell`.
///
/// Both inputs high is the invalid combination. It drives [`Signal::Error`]
/// on both outputs rather than picking a value: that state has no defined
/// answer, and `Error` exists precisely so a fault shows up instead of
/// being quietly resolved into a plausible one.
pub struct SrLatch {
    /// What `Q` currently is. `Q̄` is derived from it.
    state: Cell<Signal>,
}

impl Default for SrLatch {
    fn default() -> Self {
        Self {
            // Nothing has set or reset it yet, and inventing a power-on value
            // would be a lie about hardware that genuinely comes up either way.
            state: Cell::new(Signal::Unknown),
        }
    }
}

impl SrLatch {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for SrLatch {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        let next = match inputs {
            [set, reset] => next_state(*set, *reset, self.state.get()),
            _ => Signal::Unknown,
        };
        self.state.set(next);
        vec![next, complement(next)]
    }
}

/// The latch's whole truth table, including what uncertainty does to it.
fn next_state(set: Signal, reset: Signal, held: Signal) -> Signal {
    match (set, reset) {
        // A fault on either input is a fault on the output — same dominance
        // the gates use.
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::Low, Signal::Low) => held,
        (Signal::High, Signal::Low) => Signal::High,
        (Signal::Low, Signal::High) => Signal::Low,
        (Signal::High, Signal::High) => Signal::Error,
        // One of them isn't driven, or isn't known yet. Holding would be a
        // guess that it's `Low`; the honest answer is that `Q` is no longer
        // known either.
        _ => Signal::Unknown,
    }
}

/// `Q̄`, which is only a real complement once `Q` is a definite level —
/// an unknown or faulted latch drives the same thing on both outputs
/// rather than pretending one of them is good.
fn complement(state: Signal) -> Signal {
    match state {
        Signal::High => Signal::Low,
        Signal::Low => Signal::High,
        other => other,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `[Q, Q̄]` after driving the two inputs.
    fn drive(latch: &SrLatch, set: Signal, reset: Signal) -> Vec<Signal> {
        latch.eval(&[set, reset])
    }

    #[test]
    fn set_then_hold_keeps_the_output_high() {
        let latch = SrLatch::new();

        assert_eq!(
            drive(&latch, Signal::High, Signal::Low),
            vec![Signal::High, Signal::Low]
        );
        // Neither input asserted: the whole point of a latch.
        assert_eq!(
            drive(&latch, Signal::Low, Signal::Low),
            vec![Signal::High, Signal::Low]
        );
    }

    #[test]
    fn reset_then_hold_keeps_the_output_low() {
        let latch = SrLatch::new();
        drive(&latch, Signal::High, Signal::Low);

        assert_eq!(
            drive(&latch, Signal::Low, Signal::High),
            vec![Signal::Low, Signal::High]
        );
        // Same inputs as the test above's second line, opposite output:
        // that difference is the memory.
        assert_eq!(
            drive(&latch, Signal::Low, Signal::Low),
            vec![Signal::Low, Signal::High]
        );
    }

    #[test]
    fn it_starts_out_unknown_rather_than_inventing_a_power_on_value() {
        let latch = SrLatch::new();
        assert_eq!(
            drive(&latch, Signal::Low, Signal::Low),
            vec![Signal::Unknown, Signal::Unknown]
        );
    }

    #[test]
    fn asserting_both_inputs_is_reported_as_an_error_on_both_outputs() {
        let latch = SrLatch::new();
        drive(&latch, Signal::High, Signal::Low);

        assert_eq!(
            drive(&latch, Signal::High, Signal::High),
            vec![Signal::Error, Signal::Error]
        );
    }

    #[test]
    fn an_uncertain_input_makes_the_stored_value_uncertain_too() {
        let latch = SrLatch::new();
        drive(&latch, Signal::High, Signal::Low);

        // Holding here would be guessing that the undriven input is `Low`.
        assert_eq!(
            drive(&latch, Signal::HighZ, Signal::Low),
            vec![Signal::Unknown, Signal::Unknown]
        );
        // And it stays lost: the latch has nothing left to hold.
        assert_eq!(
            drive(&latch, Signal::Low, Signal::Low),
            vec![Signal::Unknown, Signal::Unknown]
        );
    }

    #[test]
    fn a_faulted_input_dominates() {
        let latch = SrLatch::new();
        assert_eq!(
            drive(&latch, Signal::Error, Signal::Low),
            vec![Signal::Error, Signal::Error]
        );
    }
}
