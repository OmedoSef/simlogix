//! Per-component properties, and the panel that edits them.
//!
//! Every property is optional, and an absent one means "behave exactly as
//! this component always has". That's what lets a property be added without
//! inventing a default for the components already drawn — and it's why none
//! of them are written to a project file unless they've been set.
//!
//! Which properties a component *has* depends on its kind; the panel only
//! offers the ones that apply. The struct holds them all flat rather than in
//! a per-kind enum, because a property tends to start on one kind and turn
//! out to make sense on several.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use simlogix_core::PortLevel;

use crate::appearance::{Appearance, Facing, PinSlot, Shape, TextAlign};
use crate::canvas;
use crate::i18n::Strings;
use crate::palette::ComponentKind;
use crate::placed_component::InstancePort;

/// What the user has set on one component.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Properties {
    /// A label of the user's own, drawn under the symbol.
    ///
    /// This is a deliberate exception to the appearance convention (symbols
    /// carry no text, not even pin names): an annotation you wrote is not
    /// the same thing as a label the editor generated for you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// `Button` only — whether it rests pressed, so that clicking releases
    /// it instead of pressing it. The normally-closed switch.
    ///
    /// This is the *resting* state, not the current one: runtime state is
    /// still never saved, and a loaded project starts from this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressed: Option<bool>,

    /// `Led` only — what colour it glows when lit. Unset means the red a
    /// LED is by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,

    /// Driving ports only — whether clicking can put the port in its
    /// undriven position, and therefore whether the interface admits a
    /// third state at all. Unset means two-state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tri_state: Option<bool>,

    /// Driving ports only — where the port rests when the circuit is
    /// loaded. Like a button's `pressed`, this is the *resting* value, not
    /// the current one: runtime state is still never saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<PortLevel>,
}

impl Properties {
    /// Whether nothing has been set, so the whole thing can be left out of
    /// the file rather than written as a row of nulls.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether this port's click cycle includes the undriven position.
    pub fn is_tri_state(&self) -> bool {
        self.tri_state.unwrap_or(false)
    }

    /// Where a driving port rests. Undriven unless told otherwise — the
    /// honest starting point, since nothing has said what it carries yet.
    pub fn initial_level(&self) -> PortLevel {
        self.initial.unwrap_or_default()
    }

    /// The name to draw under the symbol, if there is one worth drawing.
    /// A name of nothing but spaces counts as none.
    pub fn label(&self) -> Option<&str> {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
    }
}

/// Component kinds that come in two flavours differing only in a setting —
/// the channel of a transistor, the polarity of a transceiver's enable.
///
/// Which flavour a component is lives in its `ComponentKind`, not in
/// [`Properties`]: an NMOS and a PMOS really are two components rather than
/// one with a switch, the symbol is already drawn from the kind, and a saved
/// `simlogix:PTransistor` says more than a `Transistor` with a flag beside
/// it. All the panel adds is the ability to change your mind after placing.
const VARIANTS: [[ComponentKind; 2]; 2] = [
    [ComponentKind::NTransistor, ComponentKind::PTransistor],
    [
        ComponentKind::BusTransceiver,
        ComponentKind::BusTransceiverOe,
    ],
];

/// Bounds on a label's size: small enough to annotate, large enough to
/// title a symbol, and never zero — which would be a shape you can't see and
/// can't get back.
const MIN_TEXT_SIZE: f32 = 5.0;
const MAX_TEXT_SIZE: f32 = 32.0;

/// A lead longer than this stops being part of the symbol and starts being a
/// wire the user didn't draw.
const MAX_PIN_LEAD: f32 = 40.0;

fn siblings(kind: &ComponentKind) -> Option<[ComponentKind; 2]> {
    VARIANTS.into_iter().find(|pair| pair.contains(kind))
}

/// What the panel wants done, beyond the properties it edited in place.
#[derive(Default)]
pub struct PanelResult {
    /// This frame begins an editing session — the moment to snapshot for
    /// undo, rather than every frame a value changes.
    pub edit_started: bool,
    /// Turn this component into its sibling kind.
    pub change_kind: Option<ComponentKind>,
}

/// Draws the panel for the symbol as a whole — what there is to set when
/// nothing in particular is picked.
pub fn show_symbol(ui: &mut Ui, strings: &Strings, appearance: &mut Appearance) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(strings.symbol_selected);
    ui.add_space(8.0);

    if ui
        .checkbox(&mut appearance.show_name, strings.symbol_show_name)
        .changed()
    {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    ui.label(RichText::new(strings.shape_none_selected).weak());

    result
}

/// Draws the panel for a selected pin of a symbol.
///
/// The direction is **set here, not worked out from where the pin sits.** It
/// used to be taken from the nearest edge of the line art on every drop,
/// which reads well on a rectangle and fights you on anything else — a pin
/// beside a curve, or one deliberately pointing across the body, kept being
/// turned back. Romain's call after using it, and the right one: guessing is
/// only worth it when it is right nearly always.
pub fn show_pin(
    ui: &mut Ui,
    strings: &Strings,
    pin: &mut PinSlot,
    port: Option<&InstancePort>,
) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    // Which pin this is, said by name rather than left to be worked out from
    // its position — that is the whole reason a port has a name, and on a
    // symbol with four identical-looking pins it is the only way to tell.
    match port
        .map(|port| port.name.as_str())
        .filter(|name| !name.is_empty())
    {
        Some(name) => ui.label(format!("{} — {name}", strings.pin_selected)),
        None => ui.label(format!(
            "{} — {}",
            strings.pin_selected, strings.pin_unnamed
        )),
    };
    if let Some(port) = port {
        ui.label(RichText::new(port_kind_label(strings, &port.kind)).weak());
    }
    ui.add_space(8.0);

    ui.label(strings.pin_facing);
    ui.horizontal_wrapped(|ui| {
        for (facing, label) in [
            (Facing::Left, strings.pin_facing_left),
            (Facing::Right, strings.pin_facing_right),
            (Facing::Up, strings.pin_facing_up),
            (Facing::Down, strings.pin_facing_down),
        ] {
            if ui.selectable_label(pin.facing == facing, label).clicked() {
                result.edit_started = true;
                pin.facing = facing;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(strings.shape_position);
    // A pin has to land on a grid dot, so its fields step by a whole one —
    // unlike a shape's, which are free. Typing an off-grid value is still
    // possible and is the user's business; the step is what a drag gives.
    result.edit_started |= point_row(ui, &mut pin.at, canvas::GRID_SPACING);

    ui.add_space(8.0);
    ui.label(strings.pin_lead);
    let response = ui.add(egui::Slider::new(&mut pin.lead, 0.0..=MAX_PIN_LEAD));
    if response.drag_started() || response.gained_focus() {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    if ui
        .checkbox(&mut pin.show_name, strings.pin_show_name)
        .changed()
    {
        result.edit_started = true;
    }

    result
}

fn port_kind_label(strings: &Strings, kind: &ComponentKind) -> &'static str {
    match kind {
        ComponentKind::OutputPort => strings.component_output_port,
        ComponentKind::InOutPort => strings.component_inout_port,
        _ => strings.component_input_port,
    }
}

/// One `X` / `Y` pair. Reports whether an editing session began this frame.
fn point_row(ui: &mut Ui, point: &mut (f32, f32), step: f32) -> bool {
    let mut started = false;
    ui.horizontal(|ui| {
        for (axis, value) in [("X", &mut point.0), ("Y", &mut point.1)] {
            ui.label(axis);
            let response = ui.add(
                egui::DragValue::new(value)
                    .speed(step / 4.0)
                    .fixed_decimals(1),
            );
            if response.drag_started() || response.gained_focus() {
                started = true;
            }
        }
    });
    started
}

/// Draws the panel for a selected shape of a symbol.
///
/// Every shape shows the points it is made of, editable by hand. Dragging is
/// how you sketch one; typing is how you make it exact — and the drawing step
/// deliberately doesn't apply here, so a curve can sit off the grid when
/// that is what it takes to look right.
pub fn show_shape(ui: &mut Ui, strings: &Strings, shape: &mut Shape) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(shape_kind_label(strings, shape));
    ui.add_space(8.0);

    let step = crate::appearance::SHAPE_SNAP;
    match shape {
        Shape::Polyline { points, closed } => {
            if ui.checkbox(closed, strings.shape_closed).changed() {
                result.edit_started = true;
            }
            ui.add_space(8.0);
            ui.label(strings.shape_points);
            for (index, point) in points.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", index + 1)).weak());
                    result.edit_started |= point_row(ui, point, step);
                });
            }
        }
        Shape::Circle { center, radius } => {
            ui.label(strings.shape_center);
            result.edit_started |= point_row(ui, center, step);

            ui.add_space(8.0);
            ui.label(strings.shape_radius);
            let response = ui.add(
                egui::DragValue::new(radius)
                    .speed(step / 4.0)
                    .fixed_decimals(1)
                    // Never zero: a circle you can't see is one you can't
                    // select either, and there'd be no way back to it.
                    .range(0.5..=f32::INFINITY),
            );
            if response.drag_started() || response.gained_focus() {
                result.edit_started = true;
            }
        }
        Shape::Arc { start, mid, end } => {
            for (label, point) in [
                (strings.shape_arc_start, start),
                (strings.shape_arc_mid, mid),
                (strings.shape_arc_end, end),
            ] {
                ui.label(label);
                result.edit_started |= point_row(ui, point, step);
            }
        }
        Shape::Text {
            at,
            align,
            size,
            text,
        } => {
            ui.label(strings.shape_text_content);
            // The snapshot is taken when the session *begins*, so a typed
            // label is one undo step rather than one per keystroke — the same
            // rule the component name field follows.
            if ui.text_edit_singleline(text).gained_focus() {
                result.edit_started = true;
            }

            ui.add_space(8.0);
            ui.label(strings.shape_position);
            result.edit_started |= point_row(ui, at, step);

            ui.add_space(8.0);
            ui.label(strings.shape_text_size);
            let response = ui.add(egui::Slider::new(size, MIN_TEXT_SIZE..=MAX_TEXT_SIZE));
            if response.drag_started() || response.gained_focus() {
                result.edit_started = true;
            }

            ui.add_space(8.0);
            ui.label(strings.shape_text_align);
            ui.horizontal_wrapped(|ui| {
                for (option, label) in [
                    (TextAlign::Left, strings.pin_facing_left),
                    (TextAlign::Center, strings.shape_align_center),
                    (TextAlign::Right, strings.pin_facing_right),
                ] {
                    if ui.selectable_label(*align == option, label).clicked() {
                        result.edit_started = true;
                        *align = option;
                    }
                }
            });
        }
    }

    result
}

fn shape_kind_label(strings: &Strings, shape: &Shape) -> &'static str {
    match shape {
        Shape::Polyline { closed: true, .. } => strings.shape_rect,
        Shape::Polyline { .. } => strings.shape_line,
        Shape::Circle { .. } => strings.shape_circle,
        Shape::Arc { .. } => strings.shape_arc,
        Shape::Text { .. } => strings.shape_text,
    }
}

/// Draws the panel for the selected component and applies what's edited.
pub fn show(
    ui: &mut Ui,
    strings: &Strings,
    kind: &ComponentKind,
    properties: &mut Properties,
) -> PanelResult {
    let mut result = PanelResult::default();
    let mut edit_started = false;

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);

    match siblings(kind) {
        // A component with a sibling shows the choice instead of a plain
        // name, so the name is still there but now says which one it is.
        Some(pair) => {
            ui.label(strings.property_variant);
            for option in pair {
                let label = strings.component_kind_label(&option);
                if ui.radio(option == *kind, label).clicked() && option != *kind {
                    result.change_kind = Some(option);
                }
            }
        }
        None => {
            ui.label(strings.component_kind_label(kind));
        }
    }
    ui.add_space(8.0);

    ui.label(strings.property_name);
    // Kept as an owned buffer so an empty field can mean "no name at all"
    // rather than "a name that happens to be empty".
    let mut name = properties.name.clone().unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::singleline(&mut name)
            .desired_width(f32::INFINITY)
            .hint_text(strings.property_name_hint),
    );
    // The snapshot is taken when the field is entered, so a whole typed name
    // is one undo step.
    if response.gained_focus() {
        edit_started = true;
    }
    if response.changed() {
        properties.name = if name.is_empty() { None } else { Some(name) };
    }

    match kind {
        ComponentKind::Button | ComponentKind::Switch => {
            ui.add_space(8.0);
            let mut pressed = properties.pressed.unwrap_or(false);
            // The same stored value, said two ways. For a button it is a
            // *resting* state, since a press springs back and is never
            // saved. For a switch it is the position itself: a latched
            // switch stays where it was put, so where it is *is* how the
            // circuit was left — something the user set, not something the
            // simulation produced, and the line the document draws.
            let (label, hint) = if *kind == ComponentKind::Switch {
                (strings.property_switch_on, strings.property_switch_on_hint)
            } else {
                (strings.property_pressed, strings.property_pressed_hint)
            };
            let response = ui.checkbox(&mut pressed, label).on_hover_text(hint);
            if response.changed() {
                edit_started = true;
                // Back to unset when it's the default again, so a project
                // that says nothing keeps saying nothing.
                properties.pressed = pressed.then_some(true);
            }
        }
        ComponentKind::Led => {
            ui.add_space(8.0);
            ui.label(strings.property_color);
            ui.horizontal(|ui| {
                let mut color = properties.color.unwrap_or(DEFAULT_LED_COLOR);
                // One undo step per change here rather than per editing
                // session: the picker gives no "started" signal to hang a
                // snapshot on, so dragging through the wheel leaves a few
                // steps behind. Accepted -- the alternative is losing the
                // ability to undo the colour at all.
                if ui.color_edit_button_srgb(&mut color).changed() {
                    edit_started = true;
                    properties.color = Some(color);
                }
                if properties.color.is_some() && ui.button(strings.property_reset).clicked() {
                    edit_started = true;
                    properties.color = None;
                }
            });
        }
        // The two ports that drive. An output only reads, so it has neither
        // a resting value nor a say in how many states the interface has.
        ComponentKind::InputPort | ComponentKind::InOutPort => {
            ui.add_space(8.0);
            let mut tri_state = properties.is_tri_state();
            if ui
                .checkbox(&mut tri_state, strings.property_tri_state)
                .on_hover_text(strings.property_tri_state_hint)
                .changed()
            {
                edit_started = true;
                properties.tri_state = tri_state.then_some(true);
            }

            ui.add_space(8.0);
            ui.label(strings.property_initial);
            let current = properties.initial_level();
            for (level, label) in [
                (PortLevel::Undriven, strings.port_level_undriven),
                (PortLevel::Low, strings.port_level_low),
                (PortLevel::High, strings.port_level_high),
            ] {
                // Undriven stays offered even on a two-state port: it is
                // still where a port sits before anything drives it, and
                // hiding it would make that state unreachable rather than
                // absent.
                if ui.radio(current == level, label).clicked() && current != level {
                    edit_started = true;
                    // Back to unset when it's the default again, so a
                    // project that says nothing keeps saying nothing.
                    properties.initial = (level != PortLevel::default()).then_some(level);
                }
            }
        }
        _ => {}
    }

    result.edit_started = edit_started;
    result
}

/// Draws the panel for the selected wire. Returns the colour it should now
/// have, when that changed this frame — `Some(None)` is a reset.
///
/// A wire's colour is really the *net's*: every wire of a net gets it, so
/// a conductor is one colour end to end. The caller does that spreading,
/// since only it knows what's connected to what.
pub fn show_wire(
    ui: &mut Ui,
    strings: &Strings,
    color: Option<[u8; 3]>,
) -> Option<Option<[u8; 3]>> {
    let mut edited = None;

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(strings.property_wire);
    ui.add_space(8.0);

    ui.label(strings.property_color);
    ui.horizontal(|ui| {
        let mut picked = color.unwrap_or(DEFAULT_WIRE_COLOR);
        if ui.color_edit_button_srgb(&mut picked).changed() {
            edited = Some(Some(picked));
        }
        if color.is_some() && ui.button(strings.property_reset).clicked() {
            edited = Some(None);
        }
    });
    ui.add_space(4.0);
    ui.label(strings.property_wire_color_hint);

    edited
}

/// What the colour picker starts from for a wire that has none. Only a
/// starting point for the picker — an unset wire draws no casing at all.
pub const DEFAULT_WIRE_COLOR: [u8; 3] = [90, 127, 214];

/// What a LED glows when nothing says otherwise — a real one is red, which
/// is why this one colour doesn't follow the editor's theme.
pub const DEFAULT_LED_COLOR: [u8; 3] = [220, 30, 30];

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_properties_are_left_out_of_the_file_entirely() {
        let json = serde_json::to_string(&Properties::default()).expect("serializes");
        assert_eq!(json, "{}");
        assert!(Properties::default().is_empty());
    }

    #[test]
    fn a_name_of_nothing_but_spaces_is_not_a_name() {
        let properties = Properties {
            name: Some("   ".to_string()),
            ..Default::default()
        };
        assert_eq!(properties.label(), None);
    }

    #[test]
    fn properties_survive_a_round_trip() {
        let properties = Properties {
            name: Some("clk".to_string()),
            pressed: Some(true),
            color: Some([1, 2, 3]),
            tri_state: Some(true),
            initial: Some(PortLevel::High),
        };

        let json = serde_json::to_string(&properties).expect("serializes");
        let parsed: Properties = serde_json::from_str(&json).expect("parses");

        assert_eq!(parsed, properties);
    }

    #[test]
    fn a_ports_resting_level_defaults_to_undriven() {
        // Nothing has said what it carries yet, so claiming a level would be
        // inventing one — and undriven is the case worth being able to test.
        assert_eq!(Properties::default().initial_level(), PortLevel::Undriven);
        assert!(!Properties::default().is_tri_state());
    }

    #[test]
    fn a_component_saved_before_properties_existed_reads_as_having_none() {
        let parsed: Properties = serde_json::from_str("{}").expect("parses");
        assert!(parsed.is_empty());
    }
}
