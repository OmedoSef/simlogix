use crate::component::Component;
use crate::signal::Signal;

/// A read-only measurement point: a single input pin and no outputs, used to
/// observe a net's signal without affecting the circuit.
///
/// Functionally identical to [`crate::Led`] at the model level — the
/// difference is purely how a GUI renders it: an `Led` shows a simple on/off
/// indicator, a `Probe` is meant to show the full signal state (`High`,
/// `Low`, `Unknown`, `Error`, `HighZ`) as text, for debugging a circuit.
#[derive(Debug, Default, Clone, Copy)]
pub struct Probe;

impl Component for Probe {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        Vec::new()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_has_no_outputs_regardless_of_its_input() {
        assert_eq!(Probe.eval(&[Signal::High]), Vec::new());
        assert_eq!(Probe.eval(&[Signal::Unknown]), Vec::new());
    }
}
