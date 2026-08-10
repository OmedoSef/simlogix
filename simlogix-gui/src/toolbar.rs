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
/// Not `Copy`: `Place` carries a [`ComponentKind`], which names a circuit
/// when the thing being placed is one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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

/// Which side of a circuit is on the canvas.
///
/// A circuit has two: the schematic it is made of, and the symbol it shows
/// when it is used inside another one. Two views of one thing, the way
/// Logisim splits them — not two documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Schematic,
    Appearance,
    /// The circuit, running, with nothing that can move it.
    ///
    /// Not a separate copy of the drawing — the same one, with every gesture
    /// that edits taken away. Watching a circuit means clicking switches and
    /// reading probes for minutes at a time, and one dropped drag in the
    /// middle of that is a change you didn't see happen and won't think to
    /// undo.
    Simulation,
}

/// What a click does while a circuit is being watched rather than built.
///
/// Nothing here edits, and nothing here is a placeholder: this row is where
/// tools for *inspecting* a running circuit will go as they arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimTool {
    /// Click switches, buttons and ports; hover to light a net.
    #[default]
    Interact,
    /// Drag to move the view.
    Pan,
}

/// What the simulation row can produce: a mode, or a one-shot.
///
/// The two are kept apart rather than folded into `SimTool`, because a
/// *tool* stays chosen and an *action* happens once — a row where clicking
/// one button leaves it pressed and clicking the next does not would have to
/// be explained. They share the row, separated, since both answer "what can
/// I do while watching this run".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAction {
    Tool(SimTool),
    /// Advance by one tick — one propagation delay — and stay stopped.
    StepTick,
    /// Advance straight to the next tick where something is scheduled.
    StepEvent,
    /// Advance to the next edge of the chosen clock source.
    StepEdge,
    /// Act on a different clock source from now on, by its position in the
    /// circuit.
    PickClock(usize),
    /// Beat the chosen port on its own while the simulation runs, instead of
    /// only when stepped.
    ToggleFreeRun,
}

/// Draws the inspection tools and one-shots. Returns what was clicked.
///
/// `has_event` greys *skip to the next event* when nothing is pending: a
/// button that cannot do anything should say so rather than answer a click
/// with silence.
/// What the simulation row needs to know about the circuit it is drawn for.
///
/// A struct rather than a row of parameters: there were eight, three of them
/// bare booleans, which is a call site where transposing two of them still
/// compiles. The same trade the circuit tree already makes.
pub struct SimRow<'a> {
    pub tool: SimTool,
    /// Anything scheduled at all — greys *skip to the next event*.
    pub has_event: bool,
    /// Every clock source, by position in the circuit, with its label.
    pub clocks: &'a [(usize, String)],
    /// The one a clock step acts on.
    pub chosen: Option<usize>,
    /// Whether that one is a port, and so can be beaten on its own. A
    /// `Clock` already does.
    pub drivable: bool,
    pub free_running: bool,
}

pub fn show_sim_tools(ui: &mut Ui, strings: &Strings, row: SimRow<'_>) -> Option<SimAction> {
    let SimRow {
        tool: active,
        has_event,
        clocks,
        chosen,
        drivable,
        free_running,
    } = row;
    let mut clicked = None;
    for (tool, label) in [
        (SimTool::Interact, strings.tool_interact),
        (SimTool::Pan, strings.tool_pan),
    ] {
        let draw = match tool {
            SimTool::Interact => symbol::draw_select_tool,
            SimTool::Pan => symbol::draw_pan_tool,
        };
        if icon_button(ui, label, active == tool, draw) {
            clicked = Some(SimAction::Tool(tool));
        }
    }
    ui.separator();
    // Never drawn as held down: it is over as soon as it is pressed.
    if icon_button(ui, strings.tool_step_tick, false, symbol::draw_step_tool) {
        clicked = Some(SimAction::StepTick);
    }
    ui.add_enabled_ui(has_event, |ui| {
        if icon_button(ui, strings.tool_step_event, false, symbol::draw_skip_tool) {
            clicked = Some(SimAction::StepEvent);
        }
    });
    ui.add_enabled_ui(!clocks.is_empty(), |ui| {
        if icon_button(ui, strings.tool_step_edge, false, symbol::draw_edge_tool) {
            clicked = Some(SimAction::StepEdge);
        }
    });
    // Only offered when the source is a port. A `Clock` already beats on
    // its own, so the button would be a switch with nothing on the other
    // side of it.
    if drivable
        && icon_button(
            ui,
            strings.tool_free_run,
            free_running,
            symbol::draw_free_run_tool,
        )
    {
        clicked = Some(SimAction::ToggleFreeRun);
    }
    // Only worth asking when there is a choice: with one source it settles
    // itself, and a picker with a single entry is a control that can only
    // ever say what it already says.
    if clocks.len() > 1 {
        let current = chosen.unwrap_or(clocks[0].0);
        let label = clocks
            .iter()
            .find(|(at, _)| *at == current)
            .map(|(_, name)| name.as_str())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("clock_source")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (at, name) in clocks {
                    if ui.selectable_label(*at == current, name).clicked() {
                        clicked = Some(SimAction::PickClock(*at));
                    }
                }
            })
            .response
            .on_hover_text(strings.tool_clock_source);
    }
    clicked
}

/// What a click does while drawing a symbol.
///
/// Deliberately *not* folded into [`Tool`]: that one answers "what does a
/// click do on a schematic", and half its answers (wire, place a component)
/// have no meaning here. One enum per vocabulary keeps the combinations that
/// can't exist from being representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeTool {
    /// Click to select a shape or grab a pin, drag to move, Delete to remove.
    #[default]
    Select,
    /// Click by click, like a wire — Enter or a double-click finishes it,
    /// Escape drops it.
    Line,
    /// Drag one corner to the other.
    Rect,
    /// Drag from the centre out to the edge.
    Circle,
    /// Click each end, then move to bulge it and click again.
    Arc,
    /// Click to drop a label, then type it in the panel on the right.
    Text,
    /// Drag to move the view — the primary button belongs to the drawing
    /// here, so a symbol needs its own hand.
    Pan,
}

/// Draws the drawing tools. Returns the one clicked, if any.
pub fn show_shape_tools(ui: &mut Ui, strings: &Strings, active: ShapeTool) -> Option<ShapeTool> {
    let mut clicked = None;
    for (tool, label) in [
        (ShapeTool::Select, strings.tool_select),
        (ShapeTool::Line, strings.shape_line),
        (ShapeTool::Rect, strings.shape_rect),
        (ShapeTool::Circle, strings.shape_circle),
        (ShapeTool::Arc, strings.shape_arc),
        (ShapeTool::Text, strings.shape_text),
        (ShapeTool::Pan, strings.tool_pan),
    ] {
        let draw = match tool {
            ShapeTool::Select => symbol::draw_select_tool,
            ShapeTool::Line => symbol::draw_line_shape_tool,
            ShapeTool::Rect => symbol::draw_rect_shape_tool,
            ShapeTool::Circle => symbol::draw_circle_shape_tool,
            ShapeTool::Arc => symbol::draw_arc_shape_tool,
            ShapeTool::Text => symbol::draw_text_shape_tool,
            ShapeTool::Pan => symbol::draw_pan_tool,
        };
        if icon_button(ui, label, active == tool, draw) {
            clicked = Some(tool);
        }
    }
    clicked
}

/// Draws the schematic/appearance switch. Returns the view clicked, if any.
pub fn show_views(ui: &mut Ui, strings: &Strings, active: View) -> Option<View> {
    let mut clicked = None;
    for (view, label) in [
        (View::Schematic, strings.view_schematic),
        (View::Appearance, strings.view_appearance),
        (View::Simulation, strings.view_simulation),
    ] {
        if ui.selectable_label(active == view, label).clicked() {
            clicked = Some(view);
        }
    }
    clicked
}

/// Side of the square icon buttons.
const BUTTON_SIZE: f32 = 28.0;

/// Draws the toolbar. Returns the tool clicked this frame, if any.
pub fn show(ui: &mut Ui, strings: &Strings, active: &Tool) -> Option<Tool> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        for (tool, label) in [
            (Tool::Select, strings.tool_select),
            (Tool::Marquee, strings.tool_marquee),
            (Tool::Wire, strings.tool_wire),
            (Tool::Pan, strings.tool_pan),
        ] {
            let draw = match tool {
                Tool::Pan => symbol::draw_pan_tool,
                Tool::Marquee => symbol::draw_marquee_tool,
                Tool::Wire => symbol::draw_wire_tool,
                _ => symbol::draw_select_tool,
            };
            if icon_button(ui, label, *active == tool, draw) {
                clicked = Some(tool);
            }
        }
    });
    clicked
}

/// One square icon button, labelled by tooltip so the bar stays compact.
///
/// Shared by both toolbars: the schematic's tools and the symbol editor's are
/// the same kind of thing to the eye, and a bar of words beside a bar of
/// icons reads as two unrelated controls.
fn icon_button(
    ui: &mut Ui,
    label: &str,
    is_active: bool,
    draw: fn(&egui::Painter, egui::Rect, egui::Color32),
) -> bool {
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
        draw(ui.painter(), rect.shrink(5.0), visuals.fg_stroke.color);
    }

    response.on_hover_text(label).clicked()
}
