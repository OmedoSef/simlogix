use crate::component::Component;
use crate::signal::Signal;

/// A 2-input NAND gate, combinational (no internal state): its output
/// follows `NOT (a AND b)` at every evaluation — `And`'s dominance rule,
/// inverted: a definite `Low` on either input forces the output `High`;
/// short of that, an `Error` on either input forces `Error`; short of that,
/// two definite `High`s force `Low`; anything else is `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Nand;

impl Component for Nand {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a, b] => vec![nand(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn nand(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::Low, _) | (_, Signal::Low) => Signal::High,
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::High, Signal::High) => Signal::Low,
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
    fn outputs_low_only_when_both_inputs_are_high() {
        assert_eq!(Nand.eval(&[Signal::High, Signal::High]), vec![Signal::Low]);
        assert_eq!(Nand.eval(&[Signal::High, Signal::Low]), vec![Signal::High]);
        assert_eq!(Nand.eval(&[Signal::Low, Signal::High]), vec![Signal::High]);
        assert_eq!(Nand.eval(&[Signal::Low, Signal::Low]), vec![Signal::High]);
    }

    #[test]
    fn a_definite_low_dominates_an_uncertain_other_input() {
        assert_eq!(
            Nand.eval(&[Signal::Low, Signal::Unknown]),
            vec![Signal::High]
        );
        assert_eq!(Nand.eval(&[Signal::Low, Signal::Error]), vec![Signal::High]);
        assert_eq!(Nand.eval(&[Signal::Low, Signal::HighZ]), vec![Signal::High]);
    }

    #[test]
    fn error_propagates_when_no_input_is_definitely_low() {
        assert_eq!(
            Nand.eval(&[Signal::High, Signal::Error]),
            vec![Signal::Error]
        );
    }

    #[test]
    fn anything_uncertain_without_a_low_or_error_is_unknown() {
        assert_eq!(
            Nand.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Nand.eval(&[Signal::High, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
