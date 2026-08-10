use crate::component::Component;
use crate::level::Level;

/// An output sink: a single input pin and no outputs.
///
/// An `Led` doesn't hold or compute its own "lit" state — read
/// `Circuit::signal_at` on the net its input pin is connected to; that's what a
/// GUI renders.
#[derive(Debug, Default, Clone, Copy)]
pub struct Led;

impl Component for Led {
    fn eval(&self, _inputs: &[Level]) -> Vec<Level> {
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
    fn led_has_no_outputs_regardless_of_its_input() {
        assert_eq!(Led.eval(&[Level::High]), Vec::new());
        assert_eq!(Led.eval(&[Level::Low]), Vec::new());
    }
}
