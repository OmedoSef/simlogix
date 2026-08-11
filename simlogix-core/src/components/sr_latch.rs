use std::cell::Cell;

use crate::component::{across_bits, Component};
use crate::level::Level;
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
/// # On a bus it is a register
///
/// One latch per bit, side by side: each bit sets, resets and *holds* on its
/// own, which is the whole of what a latch on a bus means. A single shared
/// state could not say it — resetting bit 0 must leave bit 2 where it was.
///
/// Both inputs high is the invalid combination. It drives [`Level::Error`]
/// on both outputs rather than picking a value: that state has no defined
/// answer, and `Error` exists precisely so a fault shows up instead of
/// being quietly resolved into a plausible one.
#[derive(Default)]
pub struct SrLatch {
    /// What `Q` currently is, one level per bit. `Q̄` is derived from it.
    ///
    /// Empty until the first evaluation, which is not a value but the
    /// absence of one: every bit then starts `Unknown`, since inventing a
    /// power-on value would be a lie about hardware that genuinely comes up
    /// either way.
    state: Cell<Signal>,
}

impl SrLatch {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for SrLatch {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        let [set, reset] = inputs else {
            return vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)];
        };
        // What it was holding, brought to the width being asked about. A
        // latch that has just been widened does not know its new bits, and
        // nothing says the old ones line up with them — `Unknown` is what
        // "no value here yet" means, and carrying bit 0 over would be a
        // guess. `across_bits` refuses a ragged set anyway; this makes the
        // refusal a stated rule rather than an accident of the adapter.
        let width = set.width().max(reset.width());
        let held = self.state.take();
        let held = if held.width() == width {
            held
        } else {
            Signal::splat(Level::Unknown, width)
        };

        let outputs = across_bits(&[set, reset, &held], |bits| match bits {
            [set, reset, held] => {
                let next = next_state(*set, *reset, *held);
                vec![next, complement(next)]
            }
            _ => vec![Level::Unknown, Level::Unknown],
        });
        self.state.set(outputs.first().cloned().unwrap_or_default());
        outputs
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
pub(crate) fn complement(state: Level) -> Level {
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
    use crate::component::eval_levels;

    /// `[Q, Q̄]` after driving the two inputs.
    fn drive(latch: &SrLatch, set: Level, reset: Level) -> Vec<Level> {
        eval_levels(latch, &[set, reset])
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

    /// `Q`, for a latch driven bit by bit.
    fn q(latch: &SrLatch, set: &[Level], reset: &[Level]) -> Vec<Level> {
        let inputs = [
            Signal::from_levels(set.to_vec()),
            Signal::from_levels(reset.to_vec()),
        ];
        latch.eval(&inputs, &[]).first().unwrap().levels().to_vec()
    }

    #[test]
    fn on_a_bus_it_is_one_latch_per_bit() {
        use Level::{High, Low, Unknown};
        let latch = SrLatch::new();

        // Every bit starts out holding nothing, not holding `Low`: a bit
        // never told anything has no value to keep.
        assert_eq!(q(&latch, &[Low; 4], &[Low; 4]), vec![Unknown; 4]);
        assert_eq!(q(&latch, &[Low; 4], &[High; 4]), vec![Low; 4]);

        // Set bits 0 and 2, leave 1 and 3 alone.
        assert_eq!(
            q(&latch, &[High, Low, High, Low], &[Low; 4]),
            vec![High, Low, High, Low]
        );

        // Reset bit 0 only. Bit 2 has to *hold* — which is the whole claim,
        // and the one a single shared state could not make.
        assert_eq!(
            q(&latch, &[Low; 4], &[High, Low, Low, Low]),
            vec![Low, Low, High, Low]
        );

        // Neither asserted anywhere: every bit holds.
        assert_eq!(q(&latch, &[Low; 4], &[Low; 4]), vec![Low, Low, High, Low]);
    }

    #[test]
    fn each_bit_faults_on_its_own() {
        use Level::{Error, High, Low};
        let latch = SrLatch::new();
        q(&latch, &[Low; 4], &[High; 4]);
        // Both asserted on bit 1 alone: that bit is invalid and the others
        // are not, so a single answer for the whole bus would be a lie
        // either way round.
        assert_eq!(
            q(&latch, &[High, High, Low, Low], &[Low, High, Low, Low]),
            vec![High, Error, Low, Low]
        );
    }

    #[test]
    fn a_latch_that_has_just_been_widened_does_not_know_its_new_bits() {
        use Level::{High, Low, Unknown};
        let latch = SrLatch::new();
        assert_eq!(q(&latch, &[High], &[Low]), vec![High]);
        // Holding on a wider bus: bit 0's old value is not carried over,
        // because nothing says the new bits line up with the old ones.
        assert_eq!(q(&latch, &[Low; 4], &[Low; 4]), vec![Unknown; 4]);
    }
}
