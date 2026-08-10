use crate::component::{scalar_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A 2-input XNOR gate, combinational (no internal state): its output
/// follows `NOT (a XOR b)` at every evaluation — same "neither input alone
/// dominates" shape as `Xor`, inverted: both inputs must be definite
/// (`High`/`Low`) to resolve the output; an `Error` on either input forces
/// `Error`; anything else is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Xnor;

impl Component for Xnor {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        scalar_eval(inputs, |inputs| match inputs {
            [a, b] => vec![xnor(*a, *b)],
            _ => vec![Level::Unknown],
        })
    }
}

fn xnor(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::High, Level::High) | (Level::Low, Level::Low) => Level::High,
        (Level::High, Level::Low) | (Level::Low, Level::High) => Level::Low,
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
    fn outputs_high_only_when_inputs_match() {
        assert_eq!(
            eval_levels(&Xnor, &[Level::Low, Level::Low]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::Low, Level::High]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::High, Level::Low]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::High, Level::High]),
            vec![Level::High]
        );
    }

    #[test]
    fn error_propagates_regardless_of_the_other_input() {
        assert_eq!(
            eval_levels(&Xnor, &[Level::Low, Level::Error]),
            vec![Level::Error]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn no_single_input_dominates_uncertainty() {
        assert_eq!(
            eval_levels(&Xnor, &[Level::Low, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Xnor, &[Level::Low, Level::HighZ]),
            vec![Level::Unknown]
        );
    }
}
