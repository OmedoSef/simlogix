use crate::component::Component;
use crate::level::Level;
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
///
/// # It drives them all alike
///
/// A rail is as wide as the wire it is tied to, and it is the one component
/// that has to be *told*: a gate takes its width from its inputs and a rail
/// has none, while a port and a constant take theirs from a property and a
/// rail has no value to hang one on. Tying a bus down would otherwise be
/// two gestures — draw it, then say how many bits — for a component whose
/// whole point is that there is nothing to say about it.
///
/// So it declares no width of its own. It can never widen a net and can
/// never disagree with one, exactly as a `Probe` cannot.
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
    fn eval(&self, _inputs: &[Signal], widths: &[usize]) -> Vec<Signal> {
        let level = match self.level {
            RailLevel::Ground => Level::Low,
            RailLevel::Power => Level::High,
        };
        // A plain wire when nothing says otherwise, which is every drawing
        // made before a wire could be wider than one bit.
        vec![Signal::splat(level, widths.first().copied().unwrap_or(1))]
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::eval_levels;

    #[test]
    fn ground_always_outputs_low() {
        assert_eq!(eval_levels(&Rail::ground(), &[]), vec![Level::Low]);
    }

    #[test]
    fn power_always_outputs_high() {
        assert_eq!(eval_levels(&Rail::power(), &[]), vec![Level::High]);
    }

    #[test]
    fn it_drives_every_bit_of_the_wire_it_is_told_about() {
        assert_eq!(
            Rail::power().eval(&[], &[8]),
            vec![Signal::splat(Level::High, 8)]
        );
        assert_eq!(
            Rail::ground().eval(&[], &[4]),
            vec![Signal::splat(Level::Low, 4)]
        );
    }

    #[test]
    fn told_nothing_it_is_a_plain_wire() {
        // Which is every drawing made before a wire could be wider than one
        // bit, and every rail sitting on one now.
        assert_eq!(Rail::ground().eval(&[], &[]), vec![Signal::bit(Level::Low)]);
    }
}
