use crate::component::{bitwise_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// An inverter, combinational (no internal state): its single output
/// follows `NOT input` at every evaluation. `Error` stays `Error`
/// (inverting an already-contended value is still contended); a `HighZ`
/// input isn't meaningfully invertible either, so it resolves to `Unknown`,
/// same as an already-`Unknown` input.
#[derive(Debug, Default, Clone, Copy)]
pub struct Not;

impl Component for Not {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        bitwise_eval(inputs, |inputs| match inputs {
            [a] => not(*a),
            _ => Level::Unknown,
        })
    }
}

fn not(a: Level) -> Level {
    // A weak level can't reach an input: `Circuit` resolves a net before any
    // component reads it, and it never hands out a weak one. The arms exist
    // because the compiler is right to insist, and treating them as their
    // full-strength selves is the only answer that could ever be correct.
    match a.strengthened() {
        Level::High => Level::Low,
        Level::Low => Level::High,
        Level::Error => Level::Error,
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
    fn inverts_a_definite_input() {
        assert_eq!(eval_levels(&Not, &[Level::High]), vec![Level::Low]);
        assert_eq!(eval_levels(&Not, &[Level::Low]), vec![Level::High]);
    }

    #[test]
    fn error_stays_error() {
        assert_eq!(eval_levels(&Not, &[Level::Error]), vec![Level::Error]);
    }

    #[test]
    fn unknown_and_high_z_both_resolve_to_unknown() {
        assert_eq!(eval_levels(&Not, &[Level::Unknown]), vec![Level::Unknown]);
        assert_eq!(eval_levels(&Not, &[Level::HighZ]), vec![Level::Unknown]);
    }
}
