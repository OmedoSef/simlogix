use crate::component::Component;
use crate::signal::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailLevel {
    Ground,
    Power,
}

/// A fixed power rail: an input source with a single output pin (no inputs)
/// that always drives the same level, regardless of anything else in the
/// circuit — `Low` for [`Rail::ground`], `High` for [`Rail::power`]. Useful
/// for tying down a pin (e.g. a transistor's gate or source) without needing
/// a `Button` held to keep it steady.
pub struct Rail {
    level: RailLevel,
}

impl Rail {
    pub fn ground() -> Self {
        Self {
            level: RailLevel::Ground,
        }
    }

    pub fn power() -> Self {
        Self {
            level: RailLevel::Power,
        }
    }
}

impl Component for Rail {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        match self.level {
            RailLevel::Ground => vec![Signal::Low],
            RailLevel::Power => vec![Signal::High],
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_always_outputs_low() {
        assert_eq!(Rail::ground().eval(&[]), vec![Signal::Low]);
    }

    #[test]
    fn power_always_outputs_high() {
        assert_eq!(Rail::power().eval(&[]), vec![Signal::High]);
    }
}
