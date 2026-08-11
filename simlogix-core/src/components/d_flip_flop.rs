use std::cell::Cell;

use crate::component::{across_bits, Component};
use crate::components::sr_latch::complement;
use crate::level::Level;
use crate::signal::Signal;

/// An edge-triggered D flip-flop: on the chosen clock edge it captures
/// whatever `D` carries, and between edges it holds — which is the whole
/// difference from a latch, and the reason every register and every state
/// machine is built from one.
///
/// Pins, in order: `D`, the clock, then `S` and `R` **if it was given
/// asynchronous inputs**, then `Q` and `Q̄`. The component reads which shape
/// it has from how many inputs it is handed, so there is no flag here that
/// could come to disagree with the pins it actually has.
///
/// # Optional asynchronous set and reset
///
/// They are absent unless asked for, and that is what settles what an
/// undriven one means: there isn't one. Present, they follow the rule every
/// control pin in this crate follows — undriven is `Unknown`, not "not
/// asserted", because guessing a floating input is `Low` is exactly what
/// [`crate::TriStateBuffer`] and [`crate::BusTransceiver`] refuse to do.
/// The pins are opt-in precisely so that refusal costs nothing: you add them
/// when you mean to wire them.
///
/// They are **one wire each whatever the data is wide**: `S` sets every bit
/// and `R` clears every bit, which is what an asynchronous clear on a real
/// register is. Same shape as a tri-state buffer's enable.
///
/// # It captures the value `D` had *before* the edge
///
/// Not the one it has at the instant of it, which is what a setup time
/// means on a real part — and here it is load-bearing rather than a nicety.
/// A chain of flip-flops on one clock is evaluated within the same tick, and
/// a component's output is visible to the next one straight away, so
/// sampling `D` as it stands would shift a value down the whole chain in one
/// edge. That is the classic race, and a shift register is where it shows.
///
/// **The engine was deliberately not changed instead.** Committing every
/// output at the end of a tick — the delta cycle of a Verilog simulator —
/// would fix this and break something already relied on: an SR latch
/// released from its forbidden state settles here *because* the tie is
/// broken by the order the two gates happen to be evaluated in. Commit them
/// together and it oscillates for ever. So the sampling rule lives where the
/// physics puts it, in the part that has a setup time.
///
/// # On a bus it is a register
///
/// `D`, `Q` and `Q̄` widen together, one flip-flop per bit, all sharing the
/// one clock. There is nothing per-bit to decide — every bit is captured at
/// the same instant, which is the point of a register.
pub struct DFlipFlop {
    /// Which edge captures: `true` for low-to-high.
    rising: bool,
    /// What `Q` currently is, one level per bit. `Q̄` is derived from it.
    ///
    /// Empty until the first evaluation, which is the absence of a value
    /// rather than one: inventing a power-on state would misrepresent
    /// hardware that genuinely comes up either way.
    state: Cell<Signal>,
    /// The clock level at the previous evaluation, which is the only way to
    /// know an edge happened at all.
    ///
    /// Starts `Unknown`, so the first evaluation sees no edge however the
    /// clock stands: an edge is a *transition*, and nothing that was never
    /// observed low can be said to have risen.
    previous_clock: Cell<Level>,
    /// What `D` carried at the previous evaluation — the value an edge
    /// captures. See the type's own note on why it is this one and not the
    /// value `D` has at the instant of the edge.
    previous_data: Cell<Signal>,
}

impl DFlipFlop {
    /// Captures on the low-to-high edge.
    pub fn rising() -> Self {
        Self::new(true)
    }

    /// Captures on the high-to-low edge. Its symbol draws the inversion
    /// bubble on the clock input, which is the entire visible difference.
    pub fn falling() -> Self {
        Self::new(false)
    }

    fn new(rising: bool) -> Self {
        Self {
            rising,
            state: Cell::default(),
            previous_clock: Cell::new(Level::Unknown),
            previous_data: Cell::default(),
        }
    }

    /// Whether the clock just made the transition this flip-flop triggers on.
    ///
    /// Both levels have to be definite. A clock that was `Unknown` and is now
    /// `High` may or may not have risen — the honest answer is that nothing
    /// is known about it, which [`DFlipFlop::forced`] turns into an unknown
    /// stored value rather than a silent hold.
    fn edge(&self, clock: Level) -> bool {
        let (before, after) = if self.rising {
            (Level::Low, Level::High)
        } else {
            (Level::High, Level::Low)
        };
        self.previous_clock.get() == before && clock == after
    }

    /// The level every bit is forced to, when something other than the data
    /// decides — or `None` when the clocked behaviour applies.
    ///
    /// `set`/`reset` are `None` when this flip-flop has no asynchronous
    /// inputs, which is not the same as their being low: absent means the
    /// question does not arise.
    fn forced(&self, clock: Level, set: Option<Level>, reset: Option<Level>) -> Option<Level> {
        if let (Some(set), Some(reset)) = (set, reset) {
            match (set, reset) {
                // Asynchronous, so they win over the clock — including over a
                // faulted one: a chip being held clear is held clear whatever
                // its clock is doing.
                (Level::High, Level::High) => return Some(Level::Error),
                (Level::High, Level::Low) => return Some(Level::High),
                (Level::Low, Level::High) => return Some(Level::Low),
                (Level::Error, _) | (_, Level::Error) => return Some(Level::Error),
                (Level::Low, Level::Low) => {}
                // One of them is undriven or not yet known, so whether the
                // flip-flop is being held is unknown, and so is what it holds.
                _ => return Some(Level::Unknown),
            }
        }
        match clock {
            // A faulted clock is a faulted flip-flop: there is no reading of
            // "this wire is in conflict" under which the stored value is good.
            Level::Error => Some(Level::Error),
            Level::High | Level::Low => None,
            // Undriven, or not known yet. Holding would be claiming no edge
            // happened, which is more than is known.
            _ => Some(Level::Unknown),
        }
    }
}

impl Component for DFlipFlop {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        let (data, clock, set, reset) = match inputs {
            [data, clock] => (data, clock.only_level(), None, None),
            [data, clock, set, reset] => (
                data,
                clock.only_level(),
                Some(set.only_level()),
                Some(reset.only_level()),
            ),
            _ => {
                return vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)];
            }
        };

        // What it was holding, brought to the width being asked about. A
        // register just widened does not know its new bits: nothing says they
        // line up with the old ones, so carrying bit 0 across would be a
        // guess. Same rule as `SrLatch`.
        let held = self.state.take();
        let held = if held.width() == data.width() {
            held
        } else {
            Signal::splat(Level::Unknown, data.width())
        };

        // What `D` carried before this evaluation, brought to the same width
        // for the same reason the held value is.
        let sampled = self.previous_data.take();
        let sampled = if sampled.width() == data.width() {
            sampled
        } else {
            Signal::splat(Level::Unknown, data.width())
        };

        let forced = self.forced(clock, set, reset);
        let capture = self.edge(clock);
        let outputs = across_bits(&[&sampled, &held], |bits| match bits {
            [sampled, held] => {
                let next = match (forced, capture) {
                    (Some(level), _) => level,
                    (None, true) => *sampled,
                    (None, false) => *held,
                };
                vec![next, complement(next)]
            }
            _ => vec![Level::Unknown, Level::Unknown],
        });

        // Recorded after the edge has been read, and every time rather than
        // only on a transition: they are "the inputs as they were last seen",
        // and copies updated only sometimes would report an edge that had
        // already been acted on, or sample a value from further back than the
        // one edge being answered.
        self.previous_clock.set(clock);
        self.previous_data.set(data.clone());
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

    /// `Q`, for a flip-flop with no asynchronous inputs.
    fn tick(part: &DFlipFlop, data: Level, clock: Level) -> Vec<Level> {
        let inputs = [Signal::bit(data), Signal::bit(clock)];
        part.eval(&inputs, &[])
            .first()
            .map(|signal| signal.levels().to_vec())
            .unwrap_or_default()
    }

    /// `Q`, for one with them.
    fn tick_async(part: &DFlipFlop, data: Level, clock: Level, s: Level, r: Level) -> Vec<Level> {
        let inputs = [
            Signal::bit(data),
            Signal::bit(clock),
            Signal::bit(s),
            Signal::bit(r),
        ];
        part.eval(&inputs, &[])
            .first()
            .map(|signal| signal.levels().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn it_captures_on_the_rising_edge_and_holds_between_them() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();

        // Settle the clock low first: an edge is a transition, and the very
        // first evaluation has nothing to have transitioned from. Then let
        // `D` settle before the edge, which is what a setup time is.
        tick(&part, Low, Low);
        tick(&part, High, Low);
        assert_eq!(tick(&part, High, High), vec![High], "captured on the edge");

        // D moves while the clock stays high: a flip-flop is not a latch.
        assert_eq!(tick(&part, Low, High), vec![High], "held, not transparent");
        assert_eq!(
            tick(&part, Low, Low),
            vec![High],
            "held on the falling edge"
        );
        assert_eq!(tick(&part, Low, High), vec![Low], "the next rising edge");
    }

    #[test]
    fn the_falling_variant_triggers_on_the_other_edge() {
        use Level::{High, Low, Unknown};
        let part = DFlipFlop::falling();
        // Settle the clock high. Nothing is captured on the way, so it is
        // still holding nothing — which is what a register that has never
        // been clocked holds.
        tick(&part, Low, High);
        assert_eq!(
            tick(&part, High, High),
            vec![Unknown],
            "no edge, so it holds"
        );
        assert_eq!(tick(&part, High, Low), vec![High], "captured going down");
        // And the other edge does nothing to it, which is what tells the two
        // variants apart at all.
        assert_eq!(tick(&part, Low, High), vec![High], "not its edge");
    }

    #[test]
    fn it_captures_the_value_d_had_before_the_edge() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();
        tick(&part, Low, Low);
        // `D` rises at the same instant as the clock, so what was on it
        // *before* is what gets captured — a setup time, and here what keeps
        // a chain of flip-flops on one clock from shifting a value all the
        // way down in a single edge. See `tests/shift_register.rs`.
        assert_eq!(tick(&part, High, High), vec![Low]);
        // And it is not lost: the next edge takes it.
        tick(&part, High, Low);
        assert_eq!(tick(&part, High, High), vec![High]);
    }

    #[test]
    fn the_first_evaluation_is_never_an_edge() {
        // Nothing that was never seen low can be said to have risen — and a
        // register that captured on power-up would come up holding whatever
        // happened to be on `D` at the time.
        let part = DFlipFlop::rising();
        assert_eq!(tick(&part, Level::High, Level::High), vec![Level::Unknown]);
    }

    #[test]
    fn q_bar_is_the_complement_only_once_q_is_definite() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();
        tick(&part, Low, Low);
        tick(&part, High, Low);
        let outputs = part.eval(&[Signal::bit(High), Signal::bit(High)], &[]);
        assert_eq!(outputs, vec![Signal::bit(High), Signal::bit(Low)]);

        // Unknown is not a state with a good complement, so both outputs
        // report it rather than one of them pretending to be sound.
        let fresh = DFlipFlop::rising();
        assert_eq!(
            fresh.eval(&[Signal::bit(High), Signal::bit(High)], &[]),
            vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)]
        );
    }

    #[test]
    fn set_and_reset_win_over_the_clock() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();
        tick_async(&part, Low, Low, Low, Low);

        // No edge anywhere in these: that is what "asynchronous" means.
        assert_eq!(tick_async(&part, Low, Low, High, Low), vec![High], "set");
        assert_eq!(tick_async(&part, High, Low, Low, High), vec![Low], "reset");
        assert_eq!(
            tick_async(&part, Low, Low, Low, Low),
            vec![Low],
            "released, it holds what it was forced to"
        );
    }

    #[test]
    fn asserting_both_is_reported_rather_than_resolved() {
        // The same rule as `SrLatch`: that combination has no defined answer,
        // and `Error` exists so a fault shows instead of being smoothed over.
        let part = DFlipFlop::rising();
        assert_eq!(
            tick_async(&part, Level::Low, Level::Low, Level::High, Level::High),
            vec![Level::Error]
        );
    }

    #[test]
    fn an_undriven_control_leaves_it_unknown_rather_than_unasserted() {
        use Level::{High, Low, Unknown};
        let part = DFlipFlop::rising();
        tick_async(&part, Low, Low, Low, Low);
        tick_async(&part, High, High, Low, Low);

        for control in [Unknown, Level::HighZ] {
            let part = DFlipFlop::rising();
            tick_async(&part, Low, Low, Low, Low);
            assert_eq!(
                tick_async(&part, Low, Low, control, Low),
                vec![Unknown],
                "an undriven set is not the same as one held low ({control:?})"
            );
        }
    }

    #[test]
    fn a_faulted_clock_faults_what_is_stored() {
        let part = DFlipFlop::rising();
        tick(&part, Level::Low, Level::Low);
        assert_eq!(tick(&part, Level::Low, Level::Error), vec![Level::Error]);
    }

    #[test]
    fn on_a_bus_every_bit_is_captured_at_once() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();
        let clock = |level| Signal::bit(level);
        let data = Signal::from_levels(vec![High, Low, High, Low]);

        part.eval(&[Signal::splat(Low, 4), clock(Low)], &[]);
        part.eval(&[data.clone(), clock(Low)], &[]);
        assert_eq!(
            part.eval(&[data.clone(), clock(High)], &[]).first(),
            Some(&data),
            "one clock, every bit"
        );

        // And it holds all of them: the register does not go transparent on
        // some bits and not others.
        part.eval(&[Signal::splat(Low, 4), clock(High)], &[]);
        assert_eq!(
            part.eval(&[Signal::splat(Low, 4), clock(High)], &[])
                .first(),
            Some(&data)
        );
    }

    #[test]
    fn one_reset_clears_the_whole_register() {
        use Level::{High, Low};
        let part = DFlipFlop::rising();
        let wide = |level| Signal::splat(level, 4);
        let bit = Signal::bit;

        part.eval(&[wide(Low), bit(Low), bit(Low), bit(Low)], &[]);
        part.eval(&[wide(High), bit(High), bit(Low), bit(Low)], &[]);
        assert_eq!(
            part.eval(&[wide(High), bit(High), bit(Low), bit(High)], &[])
                .first(),
            Some(&wide(Low)),
            "one wire clears every bit, which is what an asynchronous clear is"
        );
    }
}
