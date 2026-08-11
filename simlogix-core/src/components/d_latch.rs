use std::cell::Cell;

use crate::component::{across_bits, Component};
use crate::components::storage::{complement, forced, resize};
use crate::level::Level;
use crate::signal::Signal;

/// A level-triggered D latch: while its enable is high it is **transparent**,
/// `Q` following `D` continuously; while the enable is low it holds whatever
/// `D` last carried.
///
/// Pins, in order: `D`, the enable, then `S` and `R` **if it was given
/// asynchronous inputs**, then `Q` and `Q̄` — the same shape as
/// [`crate::DFlipFlop`], which it is the level-triggered counterpart of.
///
/// # Transparent is the whole difference, and it is deliberate
///
/// A flip-flop captures the value `D` had *before* the edge, so that a chain
/// of them on one clock shifts by one stage per edge. A latch does the
/// opposite on purpose: it reads `D` as it stands, so a chain of latches on
/// one enable passes a value straight through all of them while the enable
/// is high. That is not a flaw of either — it is what tells them apart, and
/// it is why a shift register is built from flip-flops.
///
/// Its symbol carries no clock triangle for the same reason: that mark means
/// edge-triggered, and this is not.
///
/// # On a bus it is a register
///
/// `D`, `Q` and `Q̄` widen together, one latch per bit, all sharing the one
/// enable — and `S`/`R` are one wire each, setting or clearing every bit.
#[derive(Default)]
pub struct DLatch {
    /// What `Q` currently is, one level per bit. `Q̄` is derived from it.
    ///
    /// Empty until the first evaluation, which is the absence of a value
    /// rather than one: inventing a power-on state would misrepresent
    /// hardware that genuinely comes up either way.
    state: Cell<Signal>,
}

impl DLatch {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for DLatch {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        let (data, enable, set, reset) = match inputs {
            [data, enable] => (data, enable.only_level(), None, None),
            [data, enable, set, reset] => (
                data,
                enable.only_level(),
                Some(set.only_level()),
                Some(reset.only_level()),
            ),
            _ => return vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)],
        };

        let held = resize(self.state.take(), data.width());
        let forced = forced(enable, set, reset);
        // `forced` answered `None` only for a definite enable, so this is the
        // whole of the remaining question: high is transparent, low holds.
        let transparent = enable == Level::High;

        let outputs = across_bits(&[data, &held], |bits| match bits {
            [data, held] => {
                let next = match (forced, transparent) {
                    (Some(level), _) => level,
                    (None, true) => *data,
                    (None, false) => *held,
                };
                vec![next, complement(next)]
            }
            _ => vec![Level::Unknown, Level::Unknown],
        });

        self.state.set(outputs.first().cloned().unwrap_or_default());
        outputs
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `Q`, for a latch with no asynchronous inputs.
    fn q(latch: &DLatch, data: Level, enable: Level) -> Vec<Level> {
        let inputs = [Signal::bit(data), Signal::bit(enable)];
        latch
            .eval(&inputs, &[])
            .first()
            .map(|signal| signal.levels().to_vec())
            .unwrap_or_default()
    }

    /// `Q`, for one with them.
    fn q_async(latch: &DLatch, data: Level, enable: Level, s: Level, r: Level) -> Vec<Level> {
        let inputs = [
            Signal::bit(data),
            Signal::bit(enable),
            Signal::bit(s),
            Signal::bit(r),
        ];
        latch
            .eval(&inputs, &[])
            .first()
            .map(|signal| signal.levels().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn it_follows_its_input_while_enabled() {
        use Level::{High, Low};
        let latch = DLatch::new();
        assert_eq!(q(&latch, High, High), vec![High]);
        // No edge anywhere: it is transparent, which is the point.
        assert_eq!(q(&latch, Low, High), vec![Low]);
        assert_eq!(q(&latch, High, High), vec![High]);
    }

    #[test]
    fn it_holds_what_it_had_while_disabled() {
        use Level::{High, Low};
        let latch = DLatch::new();
        q(&latch, High, High);
        assert_eq!(q(&latch, Low, Low), vec![High], "held, not followed");
        assert_eq!(q(&latch, High, Low), vec![High]);
        // Enabled again, it catches up straight away — no edge needed.
        assert_eq!(q(&latch, Low, High), vec![Low]);
    }

    #[test]
    fn it_starts_out_holding_nothing() {
        // Not `Low`: a latch never enabled has had nothing to catch, and
        // inventing a power-on value would misrepresent real hardware.
        let latch = DLatch::new();
        assert_eq!(q(&latch, Level::High, Level::Low), vec![Level::Unknown]);
    }

    #[test]
    fn an_undriven_enable_is_not_the_same_as_a_low_one() {
        use Level::{High, Low, Unknown};
        for enable in [Unknown, Level::HighZ] {
            let latch = DLatch::new();
            q(&latch, High, High);
            assert_eq!(
                q(&latch, Low, enable),
                vec![Unknown],
                "an enable that is merely {enable:?} says nothing about holding"
            );
        }
    }

    #[test]
    fn a_faulted_enable_faults_what_is_stored() {
        let latch = DLatch::new();
        q(&latch, Level::High, Level::High);
        assert_eq!(q(&latch, Level::High, Level::Error), vec![Level::Error]);
    }

    #[test]
    fn set_and_reset_win_over_the_enable() {
        use Level::{High, Low};
        let latch = DLatch::new();
        // Disabled *and* holding nothing, so only the asynchronous pins can
        // put anything there.
        assert_eq!(q_async(&latch, Low, Low, High, Low), vec![High], "set");
        assert_eq!(q_async(&latch, High, Low, Low, High), vec![Low], "reset");
        assert_eq!(
            q_async(&latch, High, Low, Low, Low),
            vec![Low],
            "released, it holds what it was forced to"
        );
        assert_eq!(
            q_async(&latch, High, High, Low, Low),
            vec![High],
            "and enabled it goes back to following D"
        );
    }

    #[test]
    fn asserting_both_is_reported_rather_than_resolved() {
        let latch = DLatch::new();
        assert_eq!(
            q_async(&latch, Level::Low, Level::High, Level::High, Level::High),
            vec![Level::Error]
        );
    }

    #[test]
    fn on_a_bus_it_is_one_latch_per_bit() {
        use Level::{High, Low};
        let latch = DLatch::new();
        let value = Signal::from_levels(vec![High, Low, Low, High]);
        assert_eq!(
            latch.eval(&[value.clone(), Signal::bit(High)], &[]).first(),
            Some(&value)
        );
        assert_eq!(
            latch
                .eval(&[Signal::splat(Low, 4), Signal::bit(Low)], &[])
                .first(),
            Some(&value),
            "disabled, every bit holds"
        );
    }

    #[test]
    fn one_reset_clears_the_whole_register() {
        use Level::{High, Low};
        let latch = DLatch::new();
        let wide = |level| Signal::splat(level, 4);
        let bit = Signal::bit;
        latch.eval(&[wide(High), bit(High), bit(Low), bit(Low)], &[]);
        assert_eq!(
            latch
                .eval(&[wide(High), bit(Low), bit(Low), bit(High)], &[])
                .first(),
            Some(&wide(Low)),
            "one wire clears every bit"
        );
    }
}
