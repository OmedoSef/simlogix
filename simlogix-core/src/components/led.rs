use crate::component::{scalar_eval, Component};
use crate::signal::Signal;

/// An output sink: a single input pin and no outputs.
///
/// An `Led` doesn't hold or compute its own "lit" state — read
/// `Circuit::signal_at` on the net its input pin is connected to; that's what a
/// GUI renders.
#[derive(Debug, Default, Clone, Copy)]
pub struct Led;

impl Component for Led {
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
    fn led_has_no_outputs_regardless_of_its_input() {
        assert_eq!(eval_levels(&Led, &[Level::High]), Vec::new());
        assert_eq!(eval_levels(&Led, &[Level::Low]), Vec::new());
    }
}
