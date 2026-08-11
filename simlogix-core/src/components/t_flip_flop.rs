use crate::component::Component;
use crate::components::storage::EdgeTriggered;
use crate::components::xor::xor;
use crate::signal::Signal;

/// An edge-triggered T flip-flop: on the chosen clock edge it **toggles** if
/// `T` is high and holds if `T` is low.
///
/// Pins, in order: `T`, the clock, `S`, `R`, then `Q` and `Q̄` — the same
/// shape as [`crate::DFlipFlop`], which it is one line away from.
///
/// # It cannot start without a way in, so the editor always gives it one
///
/// This has no data path: it only ever transforms what it already holds, and
/// it starts holding nothing — `Unknown` toggled is still `Unknown`, for
/// ever. A D flip-flop can be started because `D` puts a value in; this one
/// can only be *set* or *cleared*. So where a D's asynchronous inputs are
/// optional, a T's are not, and the schematic editor never places one
/// without them. The two-input form is still accepted here, since a unit
/// test may reasonably drive one that way and then never assert it holds
/// anything definite.
///
/// # It is a D flip-flop with an exclusive-or fed back
///
/// `Q` becomes `Q ⊕ T` on each edge, which is exactly what wiring `Q` and
/// `T` into an [`crate::Xor`] and that into a D flip-flop's `D` would do.
/// Drawing it that way works and still does; what a primitive adds over the
/// drawn form is one symbol instead of three objects and a wire running
/// backwards — legibility, not speed.
///
/// It shares the rule as well as the shape: the toggle *is* `xor`, taken
/// from the gate rather than written out again, and everything about edges
/// and sampling belongs to
/// [`crate::components::storage::EdgeTriggered`]. So an uncertain `T` gives
/// an uncertain `Q` — nobody knows whether it toggled — and a faulted one
/// dominates, without this file deciding either.
///
/// # On a bus it is one flip-flop per bit
///
/// `T`, `Q` and `Q̄` widen together and each bit toggles on its own bit of
/// `T`, all on the one clock. That is what makes a synchronous counter a
/// single component wide rather than a row of them.
pub struct TFlipFlop {
    inner: EdgeTriggered,
}

impl TFlipFlop {
    /// Toggles on the low-to-high edge.
    pub fn rising() -> Self {
        Self::new(true)
    }

    /// Toggles on the high-to-low edge. Its symbol draws the inversion
    /// bubble on the clock input, which is the entire visible difference.
    pub fn falling() -> Self {
        Self::new(false)
    }

    fn new(rising: bool) -> Self {
        Self {
            inner: EdgeTriggered::new(rising),
        }
    }
}

impl Component for TFlipFlop {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        // What an edge stores: what it held, flipped when `T` says so. The
        // one line that is not a D flip-flop.
        self.inner.eval(inputs, xor)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;

    /// `Q`, for a flip-flop with no asynchronous inputs.
    fn tick(part: &TFlipFlop, t: Level, clock: Level) -> Vec<Level> {
        part.eval(&[Signal::bit(t), Signal::bit(clock)], &[])
            .first()
            .map(|signal| signal.levels().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn it_toggles_on_every_edge_while_t_is_high() {
        use Level::{High, Low};
        let part = TFlipFlop::rising();
        // Clear it first: a flip-flop that has never been told anything
        // holds nothing, and `Unknown ⊕ High` is still unknown — there is no
        // value to flip.
        part.eval(
            &[
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(High),
            ],
            &[],
        );

        tick(&part, High, Low);
        assert_eq!(tick(&part, High, High), vec![High], "flipped");
        tick(&part, High, Low);
        assert_eq!(tick(&part, High, High), vec![Low], "and back");
        tick(&part, High, Low);
        assert_eq!(tick(&part, High, High), vec![High]);
    }

    #[test]
    fn it_holds_while_t_is_low() {
        use Level::{High, Low};
        let part = TFlipFlop::rising();
        part.eval(
            &[
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(High),
            ],
            &[],
        );
        tick(&part, High, Low);
        tick(&part, High, High);

        tick(&part, Low, Low);
        assert_eq!(tick(&part, Low, High), vec![High], "an edge, and no flip");
        tick(&part, Low, Low);
        assert_eq!(tick(&part, Low, High), vec![High]);
    }

    #[test]
    fn nothing_happens_between_edges() {
        use Level::{High, Low};
        let part = TFlipFlop::rising();
        part.eval(
            &[
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(High),
            ],
            &[],
        );
        // `T` moving about with the clock still is not a toggle: this is a
        // flip-flop, not a gate.
        for t in [High, Low, High] {
            assert_eq!(tick(&part, t, Low), vec![Low]);
        }
    }

    #[test]
    fn an_uncertain_t_leaves_it_uncertain() {
        use Level::{High, Low, Unknown};
        let part = TFlipFlop::rising();
        part.eval(
            &[
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(Low),
                Signal::bit(High),
            ],
            &[],
        );
        // Nobody knows whether it toggled, so nobody knows what it holds.
        tick(&part, Unknown, Low);
        assert_eq!(tick(&part, Unknown, High), vec![Unknown]);
    }

    #[test]
    fn on_a_bus_each_bit_toggles_on_its_own_bit_of_t() {
        use Level::{High, Low};
        let part = TFlipFlop::rising();
        let wide = |level| Signal::splat(level, 4);
        let bit = Signal::bit;
        // Clear all four.
        part.eval(&[wide(Low), bit(Low), bit(Low), bit(High)], &[]);

        let t = Signal::from_levels(vec![High, Low, High, Low]);
        part.eval(&[t.clone(), bit(Low)], &[]);
        assert_eq!(
            part.eval(&[t.clone(), bit(High)], &[]).first(),
            Some(&Signal::from_levels(vec![High, Low, High, Low])),
            "only the bits whose T is high"
        );
    }
}
