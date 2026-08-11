use crate::component::{scalar_eval, Component};
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
    fn eval(&self, _inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        scalar_eval(_inputs, |_inputs| Vec::new())
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::eval_levels;
    use crate::level::Level;

    #[test]
    fn probe_has_no_outputs_regardless_of_its_input() {
        assert_eq!(eval_levels(&Probe, &[Level::High]), Vec::new());
        assert_eq!(eval_levels(&Probe, &[Level::Unknown]), Vec::new());
    }
}
