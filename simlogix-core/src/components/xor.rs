use crate::component::Component;
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
        match inputs {
            [a, b] => vec![xor(*a, *b)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn xor(a: Signal, b: Signal) -> Signal {
    match (a, b) {
        (Signal::Error, _) | (_, Signal::Error) => Signal::Error,
        (Signal::High, Signal::High) | (Signal::Low, Signal::Low) => Signal::Low,
        (Signal::High, Signal::Low) | (Signal::Low, Signal::High) => Signal::High,
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
    fn outputs_high_only_when_inputs_differ() {
        assert_eq!(Xor.eval(&[Signal::Low, Signal::Low]), vec![Signal::Low]);
        assert_eq!(Xor.eval(&[Signal::Low, Signal::High]), vec![Signal::High]);
        assert_eq!(Xor.eval(&[Signal::High, Signal::Low]), vec![Signal::High]);
        assert_eq!(Xor.eval(&[Signal::High, Signal::High]), vec![Signal::Low]);
    }

    #[test]
    fn error_propagates_regardless_of_the_other_input() {
        assert_eq!(Xor.eval(&[Signal::Low, Signal::Error]), vec![Signal::Error]);
        assert_eq!(
            Xor.eval(&[Signal::High, Signal::Error]),
            vec![Signal::Error]
        );
    }

    #[test]
    fn no_single_input_dominates_uncertainty() {
        assert_eq!(
            Xor.eval(&[Signal::Low, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Xor.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            Xor.eval(&[Signal::Low, Signal::HighZ]),
            vec![Signal::Unknown]
        );
    }
}
