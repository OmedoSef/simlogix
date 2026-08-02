use crate::signal::Signal;

/// A circuit element that computes its output signals from its input signals.
///
/// Both plain gates and sub-circuits implement this trait, so hierarchy
/// (a circuit reused as a component inside another circuit) is not a special case.
pub trait Component {
    /// Compute output signals from the given input signals.
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal>;

    /// Delay, in logical ticks, between an input change and the resulting output change.
    /// Defaults to 1 tick.
    fn propagation_delay(&self) -> u64 {
        1
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct NotGate;

    impl Component for NotGate {
        fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
            match inputs {
                [Signal::High] => vec![Signal::Low],
                [Signal::Low] => vec![Signal::High],
                _ => vec![Signal::Unknown],
            }
        }
    }

    #[test]
    fn component_eval_computes_outputs_from_inputs() {
        assert_eq!(NotGate.eval(&[Signal::High]), vec![Signal::Low]);
        assert_eq!(NotGate.eval(&[Signal::Low]), vec![Signal::High]);
    }

    #[test]
    fn component_propagation_delay_defaults_to_one_tick() {
        assert_eq!(NotGate.propagation_delay(), 1);
    }
}
