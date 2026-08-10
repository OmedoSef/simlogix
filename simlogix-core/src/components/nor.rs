use crate::component::Component;
use crate::level::Level;

/// A 2-input NOR gate, combinational (no internal state): its output
/// follows `NOT (a OR b)` at every evaluation — `Or`'s dominance rule,
/// inverted: a definite `High` on either input forces the output `Low`;
/// short of that, an `Error` on either input forces `Error`; short of that,
/// two definite `Low`s force `High`; anything else is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Nor;

impl Component for Nor {
    fn eval(&self, inputs: &[Level]) -> Vec<Level> {
        match inputs {
            [a, b] => vec![nor(*a, *b)],
            _ => vec![Level::Unknown],
        }
    }
}

fn nor(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::High, _) | (_, Level::High) => Level::Low,
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::Low, Level::Low) => Level::High,
        _ => Level::Unknown,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_high_only_when_both_inputs_are_low() {
        assert_eq!(Nor.eval(&[Level::Low, Level::Low]), vec![Level::High]);
        assert_eq!(Nor.eval(&[Level::Low, Level::High]), vec![Level::Low]);
        assert_eq!(Nor.eval(&[Level::High, Level::Low]), vec![Level::Low]);
        assert_eq!(Nor.eval(&[Level::High, Level::High]), vec![Level::Low]);
    }

    #[test]
    fn a_definite_high_dominates_an_uncertain_other_input() {
        assert_eq!(Nor.eval(&[Level::High, Level::Unknown]), vec![Level::Low]);
        assert_eq!(Nor.eval(&[Level::High, Level::Error]), vec![Level::Low]);
        assert_eq!(Nor.eval(&[Level::High, Level::HighZ]), vec![Level::Low]);
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_high() {
        assert_eq!(Nor.eval(&[Level::Low, Level::Error]), vec![Level::Error]);
    }

    #[test]
    fn anything_uncertain_without_a_high_or_error_is_unknown() {
        assert_eq!(
            Nor.eval(&[Level::Low, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(Nor.eval(&[Level::Low, Level::HighZ]), vec![Level::Unknown]);
    }
}
