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

use egui::Ui;
use serde::{Deserialize, Serialize};

use simlogix_core::PortLevel;

use crate::i18n::Strings;
use crate::palette::ComponentKind;

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

fn siblings(kind: ComponentKind) -> Option<[ComponentKind; 2]> {
    VARIANTS.into_iter().find(|pair| pair.contains(&kind))
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

/// Draws the panel for the selected component and applies what's edited.
pub fn show(
    ui: &mut Ui,
    strings: &Strings,
    kind: ComponentKind,
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
                let label = strings.component_kind_label(option);
                if ui.radio(option == kind, label).clicked() && option != kind {
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
            let (label, hint) = if kind == ComponentKind::Switch {
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
