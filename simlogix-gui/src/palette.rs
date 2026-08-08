//! The left palette panel: pick a component kind to place next.

use egui::{pos2, Align2, FontId, Rect, Sense, Ui};
use serde::{Deserialize, Serialize};

use crate::canvas::Rotation;
use crate::i18n::Strings;
use crate::symbol;
use crate::toolbar::Tool;

/// Which kind of component the palette currently has queued for placement.
/// Also the tag saved in a project file (see `project.rs`) to say which
/// concrete component a saved entry should become on load — that's the Rust
/// enum variant name, via `derive(Serialize)`, independent of whatever
/// display label `Strings::component_kind_label` returns for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    let categories: [(&str, &[ComponentKind]); 4] = [
        (
            strings.category_sources,
            &[
                ComponentKind::Button,
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
                    preview_label,
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
