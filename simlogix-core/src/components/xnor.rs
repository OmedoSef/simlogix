use crate::component::Component;
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
        match inputs {
            [a, b] => vec![xnor(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn xnor(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::High, Signal::High) | (Signal::Low, Signal::Low) => Signal::High,
        (Signal::High, Signal::Low) | (Signal::Low, Signal::High) => Signal::Low,
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
    fn outputs_high_only_when_inputs_match() {
        assert_eq!(Xnor.eval(&[Signal::Low, Signal::Low]), vec![Signal::High]);
        assert_eq!(Xnor.eval(&[Signal::Low, Signal::High]), vec![Signal::Low]);
        assert_eq!(Xnor.eval(&[Signal::High, Signal::Low]), vec![Signal::Low]);
        assert_eq!(Xnor.eval(&[Signal::High, Signal::High]), vec![Signal::High]);
    }

    #[test]
    fn error_propagates_regardless_of_the_other_input() {
        assert_eq!(
            Xnor.eval(&[Signal::Low, Signal::Error]),
            vec![Signal::Error]
        );
        assert_eq!(
            Xnor.eval(&[Signal::High, Signal::Error]),
            vec![Signal::Error]
        );
    }

    #[test]
    fn no_single_input_dominates_uncertainty() {
        assert_eq!(
            Xnor.eval(&[Signal::Low, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Xnor.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Xnor.eval(&[Signal::Low, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
