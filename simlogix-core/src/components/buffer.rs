use crate::component::Component;
use crate::signal::Signal;

/// A buffer, combinational (no internal state): its single output repeats
/// its input unchanged, every signal state included — unlike a gate, a
/// buffer has nothing to resolve, so there's no dominance rule here.
#[derive(Debug, Default, Clone, Copy)]
pub struct Buffer;

impl Component for Buffer {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a] => vec![*a],
            _ => vec![Signal::Unknown],
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_every_signal_state_unchanged() {
        assert_eq!(Buffer.eval(&[Signal::High]), vec![Signal::High]);
        assert_eq!(Buffer.eval(&[Signal::Low]), vec![Signal::Low]);
        assert_eq!(Buffer.eval(&[Signal::Unknown]), vec![Signal::Unknown]);
        assert_eq!(Buffer.eval(&[Signal::Error]), vec![Signal::Error]);
        assert_eq!(Buffer.eval(&[Signal::HighZ]), vec![Signal::HighZ]);
    }
}
