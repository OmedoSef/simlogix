use crate::component::{scalar_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A 2-input XOR gate, combinational (no internal state): its output follows
/// `a XOR b` at every evaluation.
///
/// Unlike `And`/`Or`, neither input alone can dominate XOR's output — even a
/// definite `Low` still leaves the result entirely dependent on the other
/// input's value. So both inputs must be definite (`High`/`Low`) to resolve
/// the output; an `Error` on either input forces `Error`; anything else
/// (`Unknown`/`HighZ` mixed in without an `Error`) is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Xor;

impl Component for Xor {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        scalar_eval(inputs, |inputs| match inputs {
            [a, b] => vec![xor(*a, *b)],
            _ => vec![Level::Unknown],
        })
    }
}

fn xor(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::High, Level::High) | (Level::Low, Level::Low) => Level::Low,
        (Level::High, Level::Low) | (Level::Low, Level::High) => Level::High,
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
    fn outputs_high_only_when_inputs_differ() {
        assert_eq!(
            eval_levels(&Xor, &[Level::Low, Level::Low]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::Low, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::High, Level::Low]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::High, Level::High]),
            vec![Level::Low]
        );
    }

    #[test]
    fn error_propagates_regardless_of_the_other_input() {
        assert_eq!(
            eval_levels(&Xor, &[Level::Low, Level::Error]),
            vec![Level::Error]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn no_single_input_dominates_uncertainty() {
        assert_eq!(
            eval_levels(&Xor, &[Level::Low, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&Xor, &[Level::Low, Level::HighZ]),
            vec![Level::Unknown]
        );
    }
}
