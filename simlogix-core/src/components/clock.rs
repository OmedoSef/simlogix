use std::cell::Cell;

use crate::component::Component;
use crate::signal::Signal;

/// A periodic source: alternates `Low`/`High` every time it's evaluated.
///
/// Unlike every other component, a `Clock` must be registered with
/// [`crate::Circuit::schedule_periodic`] rather than [`crate::Circuit::schedule_now`]
/// so it keeps ticking forever instead of firing once and going silent — it
/// has no input pins, so nothing would ever naturally re-trigger it otherwise.
pub struct Clock {
    state: Cell<Signal>,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            state: Cell::new(Signal::Low),
        }
    }
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for Clock {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        let next = match self.state.get() {
            Signal::High => Signal::Low,
            _ => Signal::High,
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
        assert_eq!(clock.eval(&[]), vec![Signal::High]);
        assert_eq!(clock.eval(&[]), vec![Signal::Low]);
        assert_eq!(clock.eval(&[]), vec![Signal::High]);
    }
}
