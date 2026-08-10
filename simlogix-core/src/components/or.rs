use crate::component::{bitwise_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A 2-input OR gate, combinational (no internal state): its output follows
/// `a OR b` at every evaluation.
///
/// Uncertain inputs resolve by dominance, the mirror image of `And`'s rule: a
/// definite `High` on either input forces the output `High` regardless of the
/// other input's value; short of that, an `Error` on either input forces
/// `Error`; short of that, anything but two definite `Low`s is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Or;

impl Component for Or {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        bitwise_eval(inputs, |inputs| match inputs {
            [a, b] => or(*a, *b),
            _ => Level::Unknown,
        })
    }
}

fn or(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::High, _) | (_, Level::High) => Level::High,
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::Low, Level::Low) => Level::Low,
        _ => Level::Unknown,
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
    fn outputs_low_only_when_both_inputs_are_low() {
        assert_eq!(
            eval_levels(&Or, &[Level::Low, Level::Low]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::Low, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::High, Level::Low]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::High, Level::High]),
            vec![Level::High]
        );
    }

    #[test]
    fn a_definite_high_dominates_an_uncertain_other_input() {
        assert_eq!(
            eval_levels(&Or, &[Level::High, Level::Unknown]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::High, Level::Error]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::High, Level::HighZ]),
            vec![Level::High]
        );
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_high() {
        assert_eq!(
            eval_levels(&Or, &[Level::Low, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn anything_uncertain_without_a_high_or_error_is_unknown() {
        assert_eq!(
            eval_levels(&Or, &[Level::Low, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::Unknown, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Or, &[Level::Low, Level::HighZ]),
            vec![Level::Unknown]
        );
    }
}
