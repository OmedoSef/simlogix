use crate::component::{bitwise_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A 2-input AND gate, combinational (no internal state): its output follows
/// `a AND b` at every evaluation.
///
/// Uncertain inputs resolve by dominance, matching how a real AND gate
/// behaves electrically: a definite `Low` on either input forces the output
/// `Low` regardless of the other input's value; short of that, an `Error` on
/// either input forces `Error`; short of that, anything but two definite
/// `High`s is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct And;

impl Component for And {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        bitwise_eval(inputs, |inputs| match inputs {
            [a, b] => and(*a, *b),
            _ => Level::Unknown,
        })
    }
}

fn and(a: Level, b: Level) -> Level {
    match (a, b) {
        (Level::Low, _) | (_, Level::Low) => Level::Low,
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::High, Level::High) => Level::High,
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
    fn outputs_high_only_when_both_inputs_are_high() {
        assert_eq!(
            eval_levels(&And, &[Level::High, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&And, &[Level::High, Level::Low]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&And, &[Level::Low, Level::High]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&And, &[Level::Low, Level::Low]),
            vec![Level::Low]
        );
    }

    #[test]
    fn a_bus_is_the_same_gate_on_every_bit() {
        // An 8-bit AND is eight AND gates side by side. Bit 0 is the least
        // significant, so this is 0b10 AND 0b11 = 0b10.
        let a = Signal::from_levels(vec![Level::Low, Level::High]);
        let b = Signal::from_levels(vec![Level::High, Level::High]);
        assert_eq!(
            And.eval(&[a, b]),
            vec![Signal::from_levels(vec![Level::Low, Level::High])]
        );
    }

    #[test]
    fn each_bit_resolves_on_its_own() {
        // The dominance rules are per bit: a definite Low forces its own
        // bit Low without saying anything about the one beside it.
        let a = Signal::from_levels(vec![Level::Low, Level::High]);
        let b = Signal::from_levels(vec![Level::Unknown, Level::Unknown]);
        assert_eq!(
            And.eval(&[a, b]),
            vec![Signal::from_levels(vec![Level::Low, Level::Unknown])]
        );
    }

    #[test]
    fn a_definite_low_dominates_an_uncertain_other_input() {
        assert_eq!(
            eval_levels(&And, &[Level::Low, Level::Unknown]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&And, &[Level::Low, Level::Error]),
            vec![Level::Low]
        );
        assert_eq!(
            eval_levels(&And, &[Level::Low, Level::HighZ]),
            vec![Level::Low]
        );
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_low() {
        assert_eq!(
            eval_levels(&And, &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn anything_uncertain_without_a_low_or_error_is_unknown() {
        assert_eq!(
            eval_levels(&And, &[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&And, &[Level::Unknown, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&And, &[Level::High, Level::HighZ]),
            vec![Level::Unknown]
        );
    }
}
