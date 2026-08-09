//! The toolbar above the canvas: which interaction mode is active.
//!
//! Separate from the palette on purpose. The palette answers "what do I want
//! to put down"; a tool answers "what does clicking do right now". Keeping
//! them apart also means the mode stays visible instead of scrolling away
//! with a component list that will only keep growing.

use egui::{Sense, Ui};

use crate::i18n::Strings;
use crate::palette::ComponentKind;
use crate::symbol;

/// What the next click on the canvas will do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    /// Click to select, drag to move — the resting state.
    #[default]
    Select,
    /// Click the canvas to drop this kind of component. Chosen from the
    /// palette rather than the toolbar, and dropping back to `Select` once
    /// the component is placed.
    Place(ComponentKind),
    /// Click anywhere to start a wire, not just on a pin. Its ends can be
    /// left loose and connected later by dragging them onto something.
    Wire,
    /// Drag to sweep a selection rectangle, whatever the left-drag setting
    /// says. Its counterpart `Pan` does the same for the view: between them,
    /// both gestures stay reachable however the setting is set — the setting
    /// only picks which one `Select` gives you without a trip to the bar.
    Marquee,
    /// Drag the canvas to move the view.
    ///
    /// Exists so `Select` can have the primary drag for its rubber band —
    /// the arrow-and-hand pair every editor has. The middle button still
    /// pans whatever the tool is, so this is a convenience rather than the
    /// only way.
    Pan,
}

/// Side of the square icon buttons.
const BUTTON_SIZE: f32 = 28.0;

/// Draws the toolbar. Returns the tool clicked this frame, if any.
pub fn show(ui: &mut Ui, strings: &Strings, active: Tool) -> Option<Tool> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        for (tool, label) in [
            (Tool::Select, strings.tool_select),
            (Tool::Marquee, strings.tool_marquee),
            (Tool::Wire, strings.tool_wire),
            (Tool::Pan, strings.tool_pan),
        ] {
            if tool_button(ui, tool, label, active == tool) {
                clicked = Some(tool);
            }
        }
    });
    clicked
}

/// One square icon button, labelled by tooltip so the bar stays compact.
fn tool_button(ui: &mut Ui, tool: Tool, label: &str, is_active: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(BUTTON_SIZE, BUTTON_SIZE), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        // The active tool borrows the theme's own "held down" visuals rather
        // than hard-coded colours, so it reads correctly in light and dark.
        let visuals = if is_active {
            &ui.style().visuals.widgets.active
        } else {
            ui.style().interact(&response)
        };
        ui.painter().rect_filled(rect, 4.0, visuals.bg_fill);
        if is_active {
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.5, visuals.fg_stroke.color),
                egui::StrokeKind::Inside,
            );
        }

        let icon = rect.shrink(5.0);
        match tool {
            Tool::Pan => symbol::draw_pan_tool(ui.painter(), icon, visuals.fg_stroke.color),
            Tool::Marquee => symbol::draw_marquee_tool(ui.painter(), icon, visuals.fg_stroke.color),
            Tool::Wire => symbol::draw_wire_tool(ui.painter(), icon, visuals.fg_stroke.color),
            _ => symbol::draw_select_tool(ui.painter(), icon, visuals.fg_stroke.color),
        }
    }

    response.on_hover_text(label).clicked()
}
