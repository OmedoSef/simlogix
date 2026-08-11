//! The pieces every storage element here shares.
//!
//! Three components hold a value — [`crate::SrLatch`], [`crate::DLatch`] and
//! [`crate::DFlipFlop`] — and they agree on what a complement is, on what an
//! asynchronous set and reset do, and on what a stored value means when the
//! bus it sits on changes width. Each of those is one rule, so each is in
//! one place: a second copy that agrees today is how the two come to
//! disagree tomorrow.

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
