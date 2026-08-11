//! The pieces every storage element here shares.
//!
//! Four components hold a value — [`crate::SrLatch`], [`crate::DLatch`],
//! [`crate::DFlipFlop`] and [`crate::TFlipFlop`] — and they agree on what a
//! complement is, on what an asynchronous set and reset do, and on what a
//! stored value means when the bus it sits on changes width. The two
//! edge-triggered ones agree on a good deal more, which [`EdgeTriggered`]
//! holds. Each of those is one rule, so each is in one place: a second copy
//! that agrees today is how the two come to disagree tomorrow.

use std::cell::Cell;

use crate::component::across_bits;
use crate::level::Level;
use crate::signal::Signal;

/// `Q̄`, which is only a real complement once `Q` is a definite level — an
/// unknown or faulted element drives the same thing on both outputs rather
/// than pretending one of them is good.
pub(crate) fn complement(state: Level) -> Level {
    match state {
        Level::High => Level::Low,
        Level::Low => Level::High,
        other => other,
    }
}

/// The level every bit is forced to when something other than the data
/// decides — or `None` when the element's normal behaviour applies.
///
/// `control` is the clock of a flip-flop or the enable of a latch: the rule
/// is the same either way, because what it says is "can this pin be read at
/// all". `set`/`reset` are `None` when the element has no asynchronous
/// inputs, which is not the same as their being low — absent means the
/// question does not arise.
pub(crate) fn forced(control: Level, set: Option<Level>, reset: Option<Level>) -> Option<Level> {
    if let (Some(set), Some(reset)) = (set, reset) {
        match (set, reset) {
            // Asynchronous, so they win over the control pin — including
            // over a faulted one: a part being held clear is held clear
            // whatever its clock is doing.
            (Level::High, Level::High) => return Some(Level::Error),
            (Level::High, Level::Low) => return Some(Level::High),
            (Level::Low, Level::High) => return Some(Level::Low),
            (Level::Error, _) | (_, Level::Error) => return Some(Level::Error),
            (Level::Low, Level::Low) => {}
            // One of them is undriven or not yet known, so whether the part
            // is being held is unknown, and so is what it holds.
            _ => return Some(Level::Unknown),
        }
    }
    match control {
        // A faulted control is a faulted element: there is no reading of
        // "this wire is in conflict" under which the stored value is good.
        Level::Error => Some(Level::Error),
        Level::High | Level::Low => None,
        // Undriven, or not known yet. Holding would be claiming nothing
        // happened on that pin, which is more than is known.
        _ => Some(Level::Unknown),
    }
}

/// A remembered signal brought to the width being asked about.
///
/// An element just widened does not know its new bits: nothing says they
/// line up with the old ones, so carrying bit 0 across would be a guess.
/// `Unknown` is what "no value here yet" means.
pub(crate) fn resize(remembered: Signal, width: usize) -> Signal {
    if remembered.width() == width {
        remembered
    } else {
        Signal::splat(Level::Unknown, width)
    }
}

/// The machinery every edge-triggered element here shares: the stored value,
/// the clock as it was last seen, and the data as it was last seen.
///
/// What is *left* to a component built on this is one function — what to
/// store when an edge arrives. A D flip-flop stores what it sampled; a T
/// flip-flop stores the exclusive-or of what it sampled with what it held.
/// That really is the whole difference between them, and having it be one
/// argument here is what keeps the claim honest: a second copy of the
/// sampling rule is the copy that would come to disagree, and the sampling
/// rule is the subtle part.
pub(crate) struct EdgeTriggered {
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
    /// What the data pin carried at the previous evaluation — the value an
    /// edge captures, rather than the one it has at the instant of it.
    ///
    /// That is a setup time on a real part, and here it is load-bearing: a
    /// chain of flip-flops on one clock is evaluated within the same tick,
    /// and a component's output is visible to the next one straight away, so
    /// reading the data as it stands would shift a value down the whole
    /// chain in a single edge.
    ///
    /// **The engine was deliberately not changed instead.** Committing every
    /// output at the end of a tick — the delta cycle of a Verilog simulator —
    /// would fix that and break something already relied on: an SR latch
    /// released from its forbidden state settles here *because* the tie is
    /// broken by the order the two gates happen to be evaluated in. Commit
    /// them together and it oscillates for ever.
    previous_data: Cell<Signal>,
}

impl EdgeTriggered {
    pub(crate) fn new(rising: bool) -> Self {
        Self {
            rising,
            state: Cell::default(),
            previous_clock: Cell::new(Level::Unknown),
            previous_data: Cell::default(),
        }
    }

    /// Whether the clock just made the transition this element triggers on.
    ///
    /// Both levels have to be definite. A clock that was `Unknown` and is now
    /// `High` may or may not have risen — the honest answer is that nothing
    /// is known about it, which [`forced`] turns into an unknown stored value
    /// rather than a silent hold.
    fn edge(&self, clock: Level) -> bool {
        let (before, after) = if self.rising {
            (Level::Low, Level::High)
        } else {
            (Level::High, Level::Low)
        };
        self.previous_clock.get() == before && clock == after
    }

    /// Pins in order: data, clock, then `S` and `R` if this element was given
    /// asynchronous inputs, and the two outputs. The shape is read from how
    /// many inputs arrive, so no flag here can come to disagree with the pins
    /// the component actually has.
    ///
    /// `store(sampled, held)` says what an edge puts in, per bit.
    pub(crate) fn eval(
        &self,
        inputs: &[Signal],
        store: impl Fn(Level, Level) -> Level,
    ) -> Vec<Signal> {
        let (data, clock, set, reset) = match inputs {
            [data, clock] => (data, clock.only_level(), None, None),
            [data, clock, set, reset] => (
                data,
                clock.only_level(),
                Some(set.only_level()),
                Some(reset.only_level()),
            ),
            _ => return vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)],
        };

        let held = resize(self.state.take(), data.width());
        let sampled = resize(self.previous_data.take(), data.width());

        let forced = forced(clock, set, reset);
        let capture = self.edge(clock);
        let outputs = across_bits(&[&sampled, &held], |bits| match bits {
            [sampled, held] => {
                let next = match (forced, capture) {
                    (Some(level), _) => level,
                    (None, true) => store(*sampled, *held),
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

    #[test]
    fn only_a_definite_level_has_a_complement() {
        assert_eq!(complement(Level::High), Level::Low);
        assert_eq!(complement(Level::Low), Level::High);
        for uncertain in [Level::Unknown, Level::Error, Level::HighZ] {
            assert_eq!(complement(uncertain), uncertain);
        }
    }

    #[test]
    fn without_asynchronous_inputs_only_the_control_can_force() {
        assert_eq!(forced(Level::High, None, None), None);
        assert_eq!(forced(Level::Low, None, None), None);
        assert_eq!(forced(Level::Error, None, None), Some(Level::Error));
        assert_eq!(forced(Level::Unknown, None, None), Some(Level::Unknown));
    }

    #[test]
    fn asynchronous_inputs_win_over_the_control() {
        let held = |set, reset| forced(Level::Error, Some(set), Some(reset));
        // A faulted clock, and it still answers what the pins say: that is
        // what asynchronous means.
        assert_eq!(held(Level::High, Level::Low), Some(Level::High));
        assert_eq!(held(Level::Low, Level::High), Some(Level::Low));
        assert_eq!(held(Level::High, Level::High), Some(Level::Error));
    }

    #[test]
    fn a_remembered_value_of_the_wrong_width_is_not_carried_over() {
        let two = Signal::splat(Level::High, 2);
        assert_eq!(resize(two.clone(), 2), two);
        assert_eq!(resize(two, 4), Signal::splat(Level::Unknown, 4));
    }
}
