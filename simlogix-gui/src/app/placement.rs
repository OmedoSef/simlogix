//! Dropping a component on the canvas: the ghost, and the click.
//!
//! Split out of the frame loop because it shares none of what that loop
//! shares. It wants the hovered position and the kind queued in the
//! toolbar, and answers with a drawing or a component — no wire, no
//! selection, no click to consume, since the drop goes through the scene's
//! own background response rather than competing for one.
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use crate::canvas::{self, BOX_SIZE};
use crate::toolbar::Tool;

use super::SimLogixApp;

impl SimLogixApp {
    /// A translucent preview of what is about to be dropped, at the grid
    /// position it will actually land on. Without it, placing is a blind
    /// click.
    pub(super) fn draw_placement_ghost(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        hover: Option<egui::Pos2>,
    ) {
        let Tool::Place(kind) = &self.tool else {
            return;
        };
        let Some(pos) = hover else {
            return;
        };
        let at = canvas::snap_to_grid(pos);
        let faint = ui.visuals().strong_text_color().gamma_multiply(0.45);
        // Drawn where it will *land*, which for a symbol drawn away from its
        // own origin is not under the pointer.
        let at = self.drop_origin(kind, at, self.place_rotation);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);

        // An instance has no fixed symbol: its box is generated from the
        // ports of the circuit it refers to, so the ghost has to be
        // generated the same way — `symbol::draw` has nothing to show for
        // one, which is why there was no ghost at all before.
        if let Some(path) = kind.circuit_path() {
            // Through the same pair the real instance uses, so the ghost
            // shows the circuit's own symbol when it has one.
            let (ports, appearance) = self.instance_preview(path);
            crate::symbol::draw_instance(
                painter,
                at,
                crate::symbol::Orientation::new(self.place_rotation, self.place_mirrored),
                faint,
                path,
                &ports,
                &appearance,
                &crate::symbol::TextLayer::for_ui(ui),
            );
            return;
        }

        crate::symbol::draw(
            painter,
            kind,
            egui::Rect::from_center_size(at, BOX_SIZE),
            crate::symbol::Orientation::new(self.place_rotation, self.place_mirrored),
            faint,
            crate::symbol::SymbolState {
                label: crate::symbol::preview_label(kind),
                ..Default::default()
            },
            &crate::symbol::TextLayer::for_ui(ui),
        );
    }

    /// Drops the queued component where the canvas was clicked.
    ///
    /// Called with the scene's *background* click, so a click that landed on
    /// a component or a wire never also drops a new one underneath it.
    pub(super) fn drop_placed(&mut self, ui: &egui::Ui, pos: egui::Pos2) {
        let Tool::Place(kind) = self.tool.clone() else {
            return;
        };
        self.record_edit();
        let at = self.drop_origin(&kind, canvas::snap_to_grid(pos), self.place_rotation);
        let id = self.place(kind, at);
        if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
            placed.set_rotation(self.place_rotation);
        }
        self.pending_attach = Some(id);
        // Holding shift keeps the kind loaded, so a row of LEDs is one trip
        // to the palette rather than one per component. Releasing it drops
        // back to selecting, which is what you want after the last one.
        if !ui.ctx().input(|input| input.modifiers.shift) {
            self.tool = Tool::Select;
        }
    }
}
