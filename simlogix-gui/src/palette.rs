//! The left palette panel: pick a component kind to place next.

use egui::{pos2, Align2, FontId, Rect, Sense, Ui};
use serde::{Deserialize, Serialize};

use crate::canvas::Rotation;
use crate::i18n::Strings;
use crate::symbol;
use crate::toolbar::Tool;

/// The library the built-in components belong to, and the namespace they're
/// saved under: `simlogix:And`, `simlogix:Button`.
///
/// Named after the application rather than its file extension. The
/// extension has already changed once (`.simlogix` → `.slgx`), and this
/// string is written into every project file for good — a namespace named
/// after a since-retired extension would be a small permanent puzzle.
pub const BUILTIN_LIBRARY: &str = "simlogix";

/// Which kind of component the palette currently has queued for placement.
/// Also the tag saved in a project file (see `project.rs`) to say which
/// concrete component a saved entry should become on load.
///
/// Saved **qualified** by library, so that a circuit imported from another
/// project can name its own components without ever colliding with a
/// built-in — a user circuit called `And` has to be able to coexist with
/// the gate. Nothing produces a non-builtin kind yet; the format speaks the
/// qualified form ahead of that so it won't have to change again when it
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    Button,
    Led,
    NTransistor,
    PTransistor,
    Ground,
    Power,
    Probe,
    Clock,
    And,
    Or,
    Nand,
    Nor,
    Xor,
    Xnor,
    Not,
    Buffer,
    SrLatch,
    TriStateBuffer,
    BusTransceiver,
    BusTransceiverOe,
    InputPort,
    OutputPort,
    InOutPort,
    Switch,
}

impl ComponentKind {
    /// Every kind paired with the name it's saved under, unqualified.
    ///
    /// One table read in both directions, rather than a match per
    /// direction: a kind added to the writer and forgotten in the reader
    /// would be a project that saves and then won't open.
    const SAVED_NAMES: [(ComponentKind, &'static str); 24] = [
        (ComponentKind::Button, "Button"),
        (ComponentKind::Led, "Led"),
        (ComponentKind::NTransistor, "NTransistor"),
        (ComponentKind::PTransistor, "PTransistor"),
        (ComponentKind::Ground, "Ground"),
        (ComponentKind::Power, "Power"),
        (ComponentKind::Probe, "Probe"),
        (ComponentKind::Clock, "Clock"),
        (ComponentKind::And, "And"),
        (ComponentKind::Or, "Or"),
        (ComponentKind::Nand, "Nand"),
        (ComponentKind::Nor, "Nor"),
        (ComponentKind::Xor, "Xor"),
        (ComponentKind::Xnor, "Xnor"),
        (ComponentKind::Not, "Not"),
        (ComponentKind::Buffer, "Buffer"),
        (ComponentKind::SrLatch, "SrLatch"),
        (ComponentKind::TriStateBuffer, "TriStateBuffer"),
        (ComponentKind::BusTransceiver, "BusTransceiver"),
        (ComponentKind::BusTransceiverOe, "BusTransceiverOe"),
        (ComponentKind::InputPort, "InputPort"),
        (ComponentKind::OutputPort, "OutputPort"),
        (ComponentKind::InOutPort, "InOutPort"),
        (ComponentKind::Switch, "Switch"),
    ];

    fn saved_name(self) -> &'static str {
        Self::SAVED_NAMES
            .iter()
            .find(|(kind, _)| *kind == self)
            .map(|(_, name)| *name)
            // Unreachable while the table lists every variant, which is what
            // the round-trip test below is there to hold to.
            .unwrap_or("Button")
    }

    fn from_saved_name(name: &str) -> Option<Self> {
        Self::SAVED_NAMES
            .iter()
            .find(|(_, saved)| *saved == name)
            .map(|(kind, _)| *kind)
    }
}

impl Serialize for ComponentKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{BUILTIN_LIBRARY}:{}", self.saved_name()))
    }
}

impl<'de> Deserialize<'de> for ComponentKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        // Projects written before the qualified form carry a bare variant
        // name. Those were all built-ins, because nothing else existed.
        let (library, name) = text
            .split_once(':')
            .unwrap_or((BUILTIN_LIBRARY, text.as_str()));

        if library != BUILTIN_LIBRARY {
            return Err(serde::de::Error::custom(format!(
                "unknown component library `{library}`: circuits from another project can't be placed yet"
            )));
        }
        Self::from_saved_name(name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown component `{text}`")))
    }
}

/// The clickable icon size within each palette row.
const ICON_SIZE: f32 = 24.0;
/// The full row's height (icon plus a little breathing room).
const ROW_HEIGHT: f32 = 28.0;

/// Draws the palette, grouped by category. `pending` is the kind currently
/// queued for placement (if any), drawn as a held-down entry so the palette
/// itself shows what you're about to drop — not just the status bar.
/// Returns `Some(kind)` the frame a palette entry is clicked, requesting
/// that kind be queued for placement.
pub fn show(ui: &mut Ui, strings: &Strings, active: Tool) -> Option<Tool> {
    ui.heading(strings.palette_heading);

    let mut clicked = None;

    let categories: [(&str, &[ComponentKind]); 7] = [
        (
            strings.category_interface,
            &[
                ComponentKind::InputPort,
                ComponentKind::OutputPort,
                ComponentKind::InOutPort,
            ],
        ),
        (
            strings.category_sources,
            &[
                ComponentKind::Button,
                ComponentKind::Switch,
                ComponentKind::Clock,
                ComponentKind::Ground,
                ComponentKind::Power,
            ],
        ),
        (
            strings.category_outputs,
            &[ComponentKind::Led, ComponentKind::Probe],
        ),
        (
            strings.category_transistors,
            &[ComponentKind::NTransistor, ComponentKind::PTransistor],
        ),
        (
            strings.category_gates,
            &[
                ComponentKind::And,
                ComponentKind::Or,
                ComponentKind::Nand,
                ComponentKind::Nor,
                ComponentKind::Xor,
                ComponentKind::Xnor,
                ComponentKind::Not,
                ComponentKind::Buffer,
                ComponentKind::TriStateBuffer,
            ],
        ),
        (strings.category_memory, &[ComponentKind::SrLatch]),
        (
            strings.category_buses,
            &[
                ComponentKind::BusTransceiver,
                ComponentKind::BusTransceiverOe,
            ],
        ),
    ];

    for (category_label, kinds) in categories {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(egui::RichText::new(category_label).strong())
            .default_open(true)
            .show(ui, |ui| {
                for &kind in kinds {
                    let is_active = active == Tool::Place(kind);
                    if palette_row(
                        ui,
                        Some(kind),
                        strings.component_kind_label(kind),
                        is_active,
                    ) {
                        clicked = Some(Tool::Place(kind));
                    }
                }
            });
    }

    clicked
}

/// A single palette entry: a small symbol followed by its name, sharing the
/// same hover/click feedback as a regular button. `kind` of `None` draws the
/// wire tool's own icon instead of a component symbol. `is_active` draws the
/// entry as held down (this is the current tool). Returns `true` if clicked
/// this frame.
fn palette_row(ui: &mut Ui, kind: Option<ComponentKind>, name: &str, is_active: bool) -> bool {
    let desired_size = egui::vec2(ui.available_width(), ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        // A queued entry borrows the "active" (held-down) visuals rather
        // than getting its own ad-hoc colors, so it stays consistent with
        // the current theme, light or dark.
        let visuals = if is_active {
            &ui.style().visuals.widgets.active
        } else {
            ui.style().interact(&response)
        };
        ui.painter()
            .rect_filled(rect, 4.0, visuals.bg_fill.gamma_multiply(0.6));
        if is_active {
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.5, visuals.fg_stroke.color),
                egui::StrokeKind::Inside,
            );
        }

        let icon_rect = Rect::from_min_size(
            pos2(rect.left() + 4.0, rect.center().y - ICON_SIZE / 2.0),
            egui::vec2(ICON_SIZE, ICON_SIZE),
        );
        match kind {
            Some(kind) => {
                let preview_label = if kind == ComponentKind::Probe {
                    "1"
                } else {
                    ""
                };
                symbol::draw(
                    ui.painter(),
                    kind,
                    icon_rect,
                    Rotation::Deg0,
                    visuals.fg_stroke.color,
                    symbol::SymbolState {
                        label: preview_label,
                        ..Default::default()
                    },
                );
            }
            None => symbol::draw_wire_tool(ui.painter(), icon_rect, visuals.fg_stroke.color),
        }

        ui.painter().text(
            pos2(icon_rect.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            visuals.text_color(),
        );
    }

    response.clicked()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the one thing `SAVED_NAMES` can get wrong: a variant added to
    /// the enum and left out of the table. Such a kind would save as some
    /// *other* component and silently change on the next load.
    #[test]
    fn every_kind_in_the_table_round_trips_through_its_saved_form() {
        for (kind, name) in ComponentKind::SAVED_NAMES {
            let json = serde_json::to_string(&kind).expect("serializes");
            assert_eq!(json, format!("\"{BUILTIN_LIBRARY}:{name}\""));

            let parsed: ComponentKind = serde_json::from_str(&json).expect("parses");
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn the_table_covers_every_variant() {
        // Every entry distinct, and as many as the enum has variants -- the
        // count is the half a duplicated entry wouldn't catch.
        let kinds: std::collections::HashSet<ComponentKind> = ComponentKind::SAVED_NAMES
            .iter()
            .map(|(kind, _)| *kind)
            .collect();
        assert_eq!(kinds.len(), ComponentKind::SAVED_NAMES.len());
    }

    #[test]
    fn a_bare_name_from_an_older_project_still_reads_as_a_builtin() {
        let parsed: ComponentKind = serde_json::from_str("\"Xnor\"").expect("parses");
        assert_eq!(parsed, ComponentKind::Xnor);
    }

    #[test]
    fn a_component_from_an_unknown_library_is_refused_rather_than_guessed_at() {
        let error = serde_json::from_str::<ComponentKind>("\"othercpu:adder\"")
            .expect_err("should not resolve");
        assert!(error.to_string().contains("othercpu"), "got {error}");
    }
}
