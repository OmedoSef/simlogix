use crate::component::Component;
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
        match inputs {
            [a] => vec![not(*a)],
            _ => vec![Signal::Unknown],
        }
    }
}

fn not(a: Signal) -> Signal {
    // A weak level can't reach an input: `Circuit` resolves a net before any
    // component reads it, and it never hands out a weak one. The arms exist
    // because the compiler is right to insist, and treating them as their
    // full-strength selves is the only answer that could ever be correct.
    match a.strengthened() {
        Signal::High => Signal::Low,
        Signal::Low => Signal::High,
        Signal::Error => Signal::Error,
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
    fn inverts_a_definite_input() {
        assert_eq!(Not.eval(&[Signal::High]), vec![Signal::Low]);
        assert_eq!(Not.eval(&[Signal::Low]), vec![Signal::High]);
    }

    #[test]
    fn error_stays_error() {
        assert_eq!(Not.eval(&[Signal::Error]), vec![Signal::Error]);
    }

    #[test]
    fn unknown_and_high_z_both_resolve_to_unknown() {
        assert_eq!(Not.eval(&[Signal::Unknown]), vec![Signal::Unknown]);
        assert_eq!(Not.eval(&[Signal::HighZ]), vec![Signal::Unknown]);
    }
}
