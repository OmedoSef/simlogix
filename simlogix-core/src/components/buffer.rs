use crate::component::Component;
use crate::level::Level;

/// A buffer, combinational (no internal state): its single output repeats
/// its input unchanged, every signal state included — unlike a gate, a
/// buffer has nothing to resolve, so there's no dominance rule here.
#[derive(Debug, Default, Clone, Copy)]
pub struct Buffer;

impl Component for Buffer {
    fn eval(&self, inputs: &[Level]) -> Vec<Level> {
        match inputs {
            [a] => vec![*a],
            _ => vec![Level::Unknown],
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
        assert_eq!(Buffer.eval(&[Level::High]), vec![Level::High]);
        assert_eq!(Buffer.eval(&[Level::Low]), vec![Level::Low]);
        assert_eq!(Buffer.eval(&[Level::Unknown]), vec![Level::Unknown]);
        assert_eq!(Buffer.eval(&[Level::Error]), vec![Level::Error]);
        assert_eq!(Buffer.eval(&[Level::HighZ]), vec![Level::HighZ]);
    }
}
