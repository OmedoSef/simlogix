use crate::component::Component;
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
        match inputs {
            [a, b] => vec![and(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn and(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::Low, _) | (_, Signal::Low) => Signal::Low,
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::High, Signal::High) => Signal::High,
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
    fn outputs_high_only_when_both_inputs_are_high() {
        assert_eq!(And.eval(&[Signal::High, Signal::High]), vec![Signal::High]);
        assert_eq!(And.eval(&[Signal::High, Signal::Low]), vec![Signal::Low]);
        assert_eq!(And.eval(&[Signal::Low, Signal::High]), vec![Signal::Low]);
        assert_eq!(And.eval(&[Signal::Low, Signal::Low]), vec![Signal::Low]);
    }

    #[test]
    fn a_definite_low_dominates_an_uncertain_other_input() {
        assert_eq!(And.eval(&[Signal::Low, Signal::Unknown]), vec![Signal::Low]);
        assert_eq!(And.eval(&[Signal::Low, Signal::Error]), vec![Signal::Low]);
        assert_eq!(And.eval(&[Signal::Low, Signal::HighZ]), vec![Signal::Low]);
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_low() {
        assert_eq!(
            And.eval(&[Signal::High, Signal::Error]),
            vec![Signal::Error]
        );
    }

    #[test]
    fn anything_uncertain_without_a_low_or_error_is_unknown() {
        assert_eq!(
            And.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            And.eval(&[Signal::Unknown, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            And.eval(&[Signal::High, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
