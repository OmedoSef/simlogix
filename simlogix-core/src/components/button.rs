use std::cell::Cell;
use std::rc::Rc;

use crate::component::Component;
use crate::signal::Signal;

/// A push button: an input source with a single output pin (no inputs of its
/// own) that follows whether it's currently pressed.
///
/// The pressed state lives in a shared `Rc<Cell<bool>>` rather than a plain
/// `bool` field, so the GUI can toggle it (e.g. on a mouse click) without
/// needing a concrete `&mut Button` back from the `Circuit` (which only hands
/// out `Box<dyn Component>`). [`Button::new`] returns the component to register
/// alongside the handle used to press/release it.
pub struct Button {
    pressed: Rc<Cell<bool>>,
}

impl Button {
    /// Creates a new, unpressed button and a handle to press/release it.
    pub fn new() -> (Self, Rc<Cell<bool>>) {
        let pressed = Rc::new(Cell::new(false));
        (
            Self {
                pressed: Rc::clone(&pressed),
            },
            pressed,
        )
    }
}

impl Component for Button {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        vec![if self.pressed.get() {
            Signal::High
        } else {
            Signal::Low
        }]
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpressed_button_outputs_low() {
        let (button, _pressed) = Button::new();
        assert_eq!(button.eval(&[]), vec![Signal::Low]);
    }

    #[test]
    fn pressing_the_handle_makes_the_button_output_high() {
        let (button, pressed) = Button::new();
        pressed.set(true);
        assert_eq!(button.eval(&[]), vec![Signal::High]);
    }
}
