use std::cell::Cell;

use crate::component::Component;
use crate::level::Level;

/// A periodic source: alternates `Low`/`High` every time it's evaluated.
///
/// Unlike every other component, a `Clock` must be registered with
/// [`crate::Circuit::schedule_periodic`] rather than [`crate::Circuit::schedule_now`]
/// so it keeps ticking forever instead of firing once and going silent — it
/// has no input pins, so nothing would ever naturally re-trigger it otherwise.
pub struct Clock {
    state: Cell<Level>,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            state: Cell::new(Level::Low),
        }
    }
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for Clock {
    fn eval(&self, _inputs: &[Level]) -> Vec<Level> {
        let next = match self.state.get() {
            Level::High => Level::Low,
            _ => Level::High,
        };
        self.state.set(next);
        vec![next]
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_alternates_high_and_low_each_evaluation() {
        let clock = Clock::new();
        assert_eq!(clock.eval(&[]), vec![Level::High]);
        assert_eq!(clock.eval(&[]), vec![Level::Low]);
        assert_eq!(clock.eval(&[]), vec![Level::High]);
    }
}
