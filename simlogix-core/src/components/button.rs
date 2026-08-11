use std::cell::Cell;
use std::rc::Rc;

use crate::component::{scalar_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A source with a single output pin and no inputs, carrying whichever level
/// its handle is set to.
///
/// **Also what a latching switch is made of.** At this level the two are the
/// same component — a source whose level the GUI owns; the difference
/// between "held down" and "stays where you put it" is entirely in how the
/// handle is driven, and a second identical type here would only be a name.
/// The GUI keeps them apart as two `ComponentKind`s, which is where the
/// distinction actually lives.
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
    fn eval(&self, _inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        scalar_eval(_inputs, |_inputs| {
            vec![if self.pressed.get() {
                Level::High
            } else {
                Level::Low
            }]
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
    fn unpressed_button_outputs_low() {
        let (button, _pressed) = Button::new();
        assert_eq!(eval_levels(&button, &[]), vec![Level::Low]);
    }

    #[test]
    fn pressing_the_handle_makes_the_button_output_high() {
        let (button, pressed) = Button::new();
        pressed.set(true);
        assert_eq!(eval_levels(&button, &[]), vec![Level::High]);
    }
}
