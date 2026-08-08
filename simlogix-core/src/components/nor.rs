use crate::component::Component;
use crate::signal::Signal;

/// A 2-input NOR gate, combinational (no internal state): its output
/// follows `NOT (a OR b)` at every evaluation — `Or`'s dominance rule,
/// inverted: a definite `High` on either input forces the output `Low`;
/// short of that, an `Error` on either input forces `Error`; short of that,
/// two definite `Low`s force `High`; anything else is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Nor;

impl Component for Nor {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a, b] => vec![nor(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn nor(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::High, _) | (_, Signal::High) => Signal::Low,
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::Low, Signal::Low) => Signal::High,
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
    fn outputs_high_only_when_both_inputs_are_low() {
        assert_eq!(Nor.eval(&[Signal::Low, Signal::Low]), vec![Signal::High]);
        assert_eq!(Nor.eval(&[Signal::Low, Signal::High]), vec![Signal::Low]);
        assert_eq!(Nor.eval(&[Signal::High, Signal::Low]), vec![Signal::Low]);
        assert_eq!(Nor.eval(&[Signal::High, Signal::High]), vec![Signal::Low]);
    }

    #[test]
    fn a_definite_high_dominates_an_uncertain_other_input() {
        assert_eq!(
            Nor.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Low]
        );
        assert_eq!(Nor.eval(&[Signal::High, Signal::Error]), vec![Signal::Low]);
        assert_eq!(Nor.eval(&[Signal::High, Signal::HighZ]), vec![Signal::Low]);
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_high() {
        assert_eq!(Nor.eval(&[Signal::Low, Signal::Error]), vec![Signal::Error]);
    }

    #[test]
    fn anything_uncertain_without_a_high_or_error_is_unknown() {
        assert_eq!(
            Nor.eval(&[Signal::Low, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Nor.eval(&[Signal::Low, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
