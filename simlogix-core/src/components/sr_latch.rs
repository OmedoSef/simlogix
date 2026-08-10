use std::cell::Cell;

use crate::component::Component;
use crate::level::Level;

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
/// Both inputs high is the invalid combination. It drives [`Level::Error`]
/// on both outputs rather than picking a value: that state has no defined
/// answer, and `Error` exists precisely so a fault shows up instead of
/// being quietly resolved into a plausible one.
pub struct SrLatch {
    /// What `Q` currently is. `Q̄` is derived from it.
    state: Cell<Level>,
}

impl Default for SrLatch {
    fn default() -> Self {
        Self {
            // Nothing has set or reset it yet, and inventing a power-on value
            // would be a lie about hardware that genuinely comes up either way.
            state: Cell::new(Level::Unknown),
        }
    }
}

impl SrLatch {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for SrLatch {
    fn eval(&self, inputs: &[Level]) -> Vec<Level> {
        let next = match inputs {
            [set, reset] => next_state(*set, *reset, self.state.get()),
            _ => Level::Unknown,
        };
        self.state.set(next);
        vec![next, complement(next)]
    }
}

/// The latch's whole truth table, including what uncertainty does to it.
fn next_state(set: Level, reset: Level, held: Level) -> Level {
    match (set, reset) {
        // A fault on either input is a fault on the output — same dominance
        // the gates use.
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::Low, Level::Low) => held,
        (Level::High, Level::Low) => Level::High,
        (Level::Low, Level::High) => Level::Low,
        (Level::High, Level::High) => Level::Error,
        // One of them isn't driven, or isn't known yet. Holding would be a
        // guess that it's `Low`; the honest answer is that `Q` is no longer
        // known either.
        _ => Level::Unknown,
    }
}

/// `Q̄`, which is only a real complement once `Q` is a definite level —
/// an unknown or faulted latch drives the same thing on both outputs
/// rather than pretending one of them is good.
fn complement(state: Level) -> Level {
    match state {
        Level::High => Level::Low,
        Level::Low => Level::High,
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
    fn drive(latch: &SrLatch, set: Level, reset: Level) -> Vec<Level> {
        latch.eval(&[set, reset])
    }

    #[test]
    fn set_then_hold_keeps_the_output_high() {
        let latch = SrLatch::new();

        assert_eq!(
            drive(&latch, Level::High, Level::Low),
            vec![Level::High, Level::Low]
        );
        // Neither input asserted: the whole point of a latch.
        assert_eq!(
            drive(&latch, Level::Low, Level::Low),
            vec![Level::High, Level::Low]
        );
    }

    #[test]
    fn reset_then_hold_keeps_the_output_low() {
        let latch = SrLatch::new();
        drive(&latch, Level::High, Level::Low);

        assert_eq!(
            drive(&latch, Level::Low, Level::High),
            vec![Level::Low, Level::High]
        );
        // Same inputs as the test above's second line, opposite output:
        // that difference is the memory.
        assert_eq!(
            drive(&latch, Level::Low, Level::Low),
            vec![Level::Low, Level::High]
        );
    }

    #[test]
    fn it_starts_out_unknown_rather_than_inventing_a_power_on_value() {
        let latch = SrLatch::new();
        assert_eq!(
            drive(&latch, Level::Low, Level::Low),
            vec![Level::Unknown, Level::Unknown]
        );
    }

    #[test]
    fn asserting_both_inputs_is_reported_as_an_error_on_both_outputs() {
        let latch = SrLatch::new();
        drive(&latch, Level::High, Level::Low);

        assert_eq!(
            drive(&latch, Level::High, Level::High),
            vec![Level::Error, Level::Error]
        );
    }

    #[test]
    fn an_uncertain_input_makes_the_stored_value_uncertain_too() {
        let latch = SrLatch::new();
        drive(&latch, Level::High, Level::Low);

        // Holding here would be guessing that the undriven input is `Low`.
        assert_eq!(
            drive(&latch, Level::HighZ, Level::Low),
            vec![Level::Unknown, Level::Unknown]
        );
        // And it stays lost: the latch has nothing left to hold.
        assert_eq!(
            drive(&latch, Level::Low, Level::Low),
            vec![Level::Unknown, Level::Unknown]
        );
    }

    #[test]
    fn a_faulted_input_dominates() {
        let latch = SrLatch::new();
        assert_eq!(
            drive(&latch, Level::Error, Level::Low),
            vec![Level::Error, Level::Error]
        );
    }
}
