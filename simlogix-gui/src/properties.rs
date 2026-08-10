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

use simlogix_core::PortSetting;

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
    pub initial: Option<PortSetting>,
}

impl Properties {
    /// Whether nothing has been set, so the whole thing can be left out of
    /// the file rather than written as a row of nulls.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether this port's click cycle includes the undriven position.
    /// Whether clicking this component offers the undriven position.
    ///
    /// A tri-state source always does — being able to let go is the whole of
    /// what it is, and a two-state one would just be a `Switch`. A port
    /// *declares* it, because there the number of states is a promise made
    /// to whatever will drive the pin from outside.
    pub fn cycles_undriven(&self, kind: &ComponentKind) -> bool {
        *kind == ComponentKind::TriStateSource || self.is_tri_state()
    }

    pub fn is_tri_state(&self) -> bool {
        self.tri_state.unwrap_or(false)
    }

    /// Where a driving port rests. Undriven unless told otherwise — the
    /// honest starting point, since nothing has said what it carries yet.
    pub fn initial_level(&self) -> PortSetting {
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

/// The swatches offered wherever a colour is chosen.
///
/// A fixed set rather than a wheel, because the thing colour is *for* here
/// is telling two wires apart at a crossing — which needs a handful of hues
/// that are obviously different, not sixteen million that mostly aren't. It
/// also makes a colour reusable: the same swatch is the same bytes, where
/// two trips through a wheel never land on quite the same place.
///
/// Chosen to stay legible on either theme, so nothing is very dark or very
/// pale.
const SWATCHES: [[u8; 3]; 12] = [
    [0xE0, 0x3B, 0x3B], // red
    [0xE8, 0x7A, 0x22], // orange
    [0xE0, 0xB0, 0x21], // amber
    [0x8B, 0xC3, 0x2E], // lime
    [0x35, 0xA8, 0x53], // green
    [0x1F, 0xA8, 0x9E], // teal
    [0x2E, 0x9B, 0xD8], // sky
    [0x3B, 0x62, 0xD0], // blue
    [0x7A, 0x4F, 0xD0], // violet
    [0xC0, 0x48, 0xC0], // magenta
    [0xD0, 0x5A, 0x8A], // pink
    [0x8A, 0x6F, 0x5A], // brown
];

/// Side of one swatch.
const SWATCH_SIZE: f32 = 20.0;

fn to_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// Reads `#RRGGBB`, or `RRGGBB` — a code pasted from somewhere else arrives
/// spelled either way, and refusing one of them would be pedantry.
fn from_hex(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A colour control: the swatches, the hex code, and the full picker behind
/// a button for anything the swatches don't cover.
///
/// Returns `true` when the colour changed this frame.
fn color_control(ui: &mut Ui, strings: &Strings, color: &mut [u8; 3]) -> bool {
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for swatch in SWATCHES {
            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(SWATCH_SIZE), egui::Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if ui.is_rect_visible(rect) {
                let fill = egui::Color32::from_rgb(swatch[0], swatch[1], swatch[2]);
                ui.painter().rect_filled(rect, 3.0, fill);
                // The one in use is ringed rather than ticked: a tick would
                // have to be drawn in a colour that reads against every
                // swatch, and there isn't one.
                let stroke = if *color == swatch {
                    egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                } else {
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color())
                };
                ui.painter()
                    .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Outside);
            }
            if response.on_hover_text(to_hex(swatch)).clicked() {
                *color = swatch;
                changed = true;
            }
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // The field keeps its own text while it is being typed into, so a
        // half-finished code like "#1a" survives the frames before it parses.
        // Out of focus it is rewritten from the colour, which is what keeps
        // it honest when a swatch or the wheel changes it instead.
        let id = ui.id().with("hex");
        let mut text = ui
            .data_mut(|data| data.get_temp::<String>(id))
            .unwrap_or_else(|| to_hex(*color));

        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(74.0)
                .font(egui::TextStyle::Monospace),
        );
        if field.has_focus() {
            if let Some(parsed) = from_hex(&text) {
                if parsed != *color {
                    *color = parsed;
                    changed = true;
                }
            }
        } else {
            text = to_hex(*color);
        }
        ui.data_mut(|data| data.insert_temp(id, text));

        if ui.color_edit_button_srgb(color).changed() {
            changed = true;
        }
        ui.label(egui::RichText::new(strings.property_color_more).weak());
    });

    changed
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
    // Beside the lead, since that is what it changes: the bubble takes the
    // last of it, against the body.
    if ui
        .checkbox(&mut pin.inverted, strings.pin_inverted)
        .on_hover_text(strings.pin_inverted_hint)
        .changed()
    {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    if ui
        .checkbox(&mut pin.show_name, strings.pin_show_name)
        .changed()
    {
        result.edit_started = true;
    }

    // Only worth offering while there is a name on screen to move.
    if pin.show_name {
        ui.add_space(8.0);
        ui.label(strings.pin_name_offset);
        // On a shape's step rather than a pin's: this moves a label clear of
        // the line art, and a nudge of a whole grid space is not a nudge.
        result.edit_started |= point_row(ui, &mut pin.name_offset, crate::appearance::SHAPE_SNAP);
        ui.horizontal(|ui| {
            if pin.name_offset != (0.0, 0.0) && ui.button(strings.property_reset).clicked() {
                result.edit_started = true;
                pin.name_offset = (0.0, 0.0);
            }
        });
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
            let mut color = properties.color.unwrap_or(DEFAULT_LED_COLOR);
            // One undo step per change rather than per editing session: a
            // swatch click is one discrete change and lands as one step, but
            // the wheel behind the button still gives no "started" signal to
            // hang a snapshot on, so dragging through it leaves a few behind.
            if color_control(ui, strings, &mut color) {
                edit_started = true;
                properties.color = Some(color);
            }
            ui.horizontal(|ui| {
                if properties.color.is_some() && ui.button(strings.property_reset).clicked() {
                    edit_started = true;
                    properties.color = None;
                }
            });
        }
        // Everything that drives a level you set by hand. An output port
        // only reads, so it has neither a resting value nor a say in how
        // many states the interface has.
        ComponentKind::InputPort | ComponentKind::InOutPort | ComponentKind::TriStateSource => {
            // A source has nothing to declare: three positions is what it
            // is. A port's count is a promise to whatever drives its pin
            // from outside, so there it is a choice.
            if *kind != ComponentKind::TriStateSource {
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
            }

            ui.add_space(8.0);
            ui.label(strings.property_initial);
            let current = properties.initial_level();
            for (level, label) in [
                (PortSetting::Undriven, strings.port_level_undriven),
                (PortSetting::Low, strings.port_level_low),
                (PortSetting::High, strings.port_level_high),
            ] {
                // Undriven stays offered even on a two-state port: it is
                // still where a port sits before anything drives it, and
                // hiding it would make that state unreachable rather than
                // absent.
                if ui.radio(current == level, label).clicked() && current != level {
                    edit_started = true;
                    // Back to unset when it's the default again, so a
                    // project that says nothing keeps saying nothing.
                    properties.initial = (level != PortSetting::default()).then_some(level);
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
    let mut picked = color.unwrap_or(DEFAULT_WIRE_COLOR);
    if color_control(ui, strings, &mut picked) {
        edited = Some(Some(picked));
    }
    ui.horizontal(|ui| {
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
    fn a_hex_code_round_trips() {
        for color in SWATCHES {
            assert_eq!(from_hex(&to_hex(color)), Some(color));
        }
    }

    #[test]
    fn a_pasted_code_is_read_with_or_without_its_hash() {
        // Both spellings turn up when a code is copied from somewhere else,
        // and refusing one of them would be pedantry.
        assert_eq!(from_hex("#1A2B3C"), Some([0x1A, 0x2B, 0x3C]));
        assert_eq!(from_hex("1a2b3c"), Some([0x1A, 0x2B, 0x3C]));
        assert_eq!(from_hex("  #1a2b3c  "), Some([0x1A, 0x2B, 0x3C]));
    }

    #[test]
    fn a_half_typed_code_is_not_read_as_a_colour() {
        // The field is edited a character at a time, so most of what it
        // holds mid-typing is not yet a colour — and guessing at one would
        // make the value jump around while it is being entered.
        for text in ["", "#", "#1a2", "#1a2b3", "#1a2b3c4", "#gggggg", "hello"] {
            assert_eq!(from_hex(text), None, "{text:?} should not parse");
        }
    }

    #[test]
    fn no_two_swatches_are_the_same_colour() {
        // A duplicate would be a slot that says nothing, on a palette whose
        // whole job is telling things apart.
        let mut seen = SWATCHES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), SWATCHES.len());
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
            initial: Some(PortSetting::High),
        };

        let json = serde_json::to_string(&properties).expect("serializes");
        let parsed: Properties = serde_json::from_str(&json).expect("parses");

        assert_eq!(parsed, properties);
        // And the *text*, not only the round trip. What reaches the file is
        // the field and variant names, never the Rust type names — which is
        // what makes renaming a type free, and is worth checking rather than
        // asserting: `PortLevel` became `PortSetting` on the strength of it.
        assert_eq!(
            json,
            r#"{"name":"clk","pressed":true,"color":[1,2,3],"tri_state":true,"initial":"High"}"#
        );
    }

    #[test]
    fn a_ports_resting_level_defaults_to_undriven() {
        // Nothing has said what it carries yet, so claiming a level would be
        // inventing one — and undriven is the case worth being able to test.
        assert_eq!(Properties::default().initial_level(), PortSetting::Undriven);
        assert!(!Properties::default().is_tri_state());
    }

    #[test]
    fn a_component_saved_before_properties_existed_reads_as_having_none() {
        let parsed: Properties = serde_json::from_str("{}").expect("parses");
        assert!(parsed.is_empty());
    }
}
