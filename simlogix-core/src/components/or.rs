use crate::component::Component;
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
        match inputs {
            [a, b] => vec![or(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn or(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::High, _) | (_, Signal::High) => Signal::High,
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::Low, Signal::Low) => Signal::Low,
        _ => Signal::Unknown,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_low_only_when_both_inputs_are_low() {
        assert_eq!(Or.eval(&[Signal::Low, Signal::Low]), vec![Signal::Low]);
        assert_eq!(Or.eval(&[Signal::Low, Signal::High]), vec![Signal::High]);
        assert_eq!(Or.eval(&[Signal::High, Signal::Low]), vec![Signal::High]);
        assert_eq!(Or.eval(&[Signal::High, Signal::High]), vec![Signal::High]);
    }

    #[test]
    fn a_definite_high_dominates_an_uncertain_other_input() {
        assert_eq!(
            Or.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::High]
        );
        assert_eq!(Or.eval(&[Signal::High, Signal::Error]), vec![Signal::High]);
        assert_eq!(Or.eval(&[Signal::High, Signal::HighZ]), vec![Signal::High]);
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_high() {
        assert_eq!(Or.eval(&[Signal::Low, Signal::Error]), vec![Signal::Error]);
    }

    #[test]
    fn anything_uncertain_without_a_high_or_error_is_unknown() {
        assert_eq!(
            Or.eval(&[Signal::Low, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Or.eval(&[Signal::Unknown, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Or.eval(&[Signal::Low, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
