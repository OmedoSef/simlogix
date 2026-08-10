use crate::component::{bitwise_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A buffer, combinational (no internal state): its single output repeats
/// its input unchanged, every signal state included — unlike a gate, a
/// buffer has nothing to resolve, so there's no dominance rule here.
#[derive(Debug, Default, Clone, Copy)]
pub struct Buffer;

impl Component for Buffer {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        bitwise_eval(inputs, |inputs| match inputs {
            [a] => *a,
            _ => Level::Unknown,
        })
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
    fn repeats_every_signal_state_unchanged() {
        assert_eq!(eval_levels(&Buffer, &[Level::High]), vec![Level::High]);
        assert_eq!(eval_levels(&Buffer, &[Level::Low]), vec![Level::Low]);
        assert_eq!(
            eval_levels(&Buffer, &[Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(eval_levels(&Buffer, &[Level::Error]), vec![Level::Error]);
        assert_eq!(eval_levels(&Buffer, &[Level::HighZ]), vec![Level::HighZ]);
    }
}
