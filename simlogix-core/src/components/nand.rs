use crate::component::{bitwise_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A 2-input NAND gate, combinational (no internal state): its output
/// follows `NOT (a AND b)` at every evaluation — `And`'s dominance rule,
/// inverted: a definite `Low` on either input forces the output `High`;
/// short of that, an `Error` on either input forces `Error`; short of that,
/// two definite `High`s force `Low`; anything else is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Nand;

impl Component for Nand {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        bitwise_eval(inputs, |inputs| match inputs {
            [a, b] => nand(*a, *b),
            _ => Level::Unknown,
        })
    }
}

fn nand(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::Low, _) | (_, Level::Low) => Level::High,
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::High, Level::High) => Level::Low,
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
    fn outputs_low_only_when_both_inputs_are_high() {
        assert_eq!(
            eval_levels(&Nand, &[Level::High, Level::High]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::High, Level::Low]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::Low, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::Low, Level::Low]),
            vec![Level::High]
        );
    }

    #[test]
    fn a_definite_low_dominates_an_uncertain_other_input() {
        assert_eq!(
            eval_levels(&Nand, &[Level::Low, Level::Unknown]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::Low, Level::Error]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::Low, Level::HighZ]),
            vec![Level::High]
        );
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_low() {
        assert_eq!(
            eval_levels(&Nand, &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn anything_uncertain_without_a_low_or_error_is_unknown() {
        assert_eq!(
            eval_levels(&Nand, &[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Nand, &[Level::High, Level::HighZ]),
            vec![Level::Unknown]
        );
    }
}
