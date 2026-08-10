//! Where the canvas is looking: the framed region, the wheel, and panning.
//!
//! Split out of the frame loop because it touches none of what that loop
//! shares — no component, no wire, no click to consume. It answers one
//! question, "what part of the drawing is on screen", and everything else
//! there answers "what did the pointer just do".
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use crate::toolbar;

use super::{SimLogixApp, FIT_MARGIN, WHEEL_ZOOM_SENSITIVITY};

impl SimLogixApp {
    /// Takes the wheel before `Scene` sees it, so it zooms rather than pans
    /// — the schematic-editor convention — and only while the pointer is
    /// over the canvas.
    ///
    /// Gated on the pointer being here rather than on the side panels having
    /// consumed it first, which was the original rule and was wrong: a
    /// scroll area only takes the wheel over its *list*, so an event over
    /// the circuit tree's heading, or over a list already scrolled to its
    /// end, fell through and zoomed the schematic while the user was plainly
    /// working somewhere else. A positive test also holds for any panel
    /// added later.
    pub(super) fn take_wheel(&self, ui: &egui::Ui) -> f32 {
        if !ui.rect_contains_pointer(ui.max_rect()) {
            return 0.0;
        }
        ui.ctx().input_mut(|input| {
            let dy = input.smooth_scroll_delta.y;
            if dy != 0.0 {
                input.smooth_scroll_delta = egui::Vec2::ZERO;
            }
            dy
        })
    }

    /// The region `Scene` should frame this pass, reframed on the drawing if
    /// something asked for it or nothing has ever framed it.
    ///
    /// Returned rather than written to `self.scene_rect`, because `Scene`
    /// mutates a local copy as the user pans and zooms and that copy is what
    /// gets written back afterwards. Assigning the field here would be
    /// overwritten a few lines later, and the framing would be computed
    /// every frame and thrown away every frame.
    ///
    /// The region starts equal to the canvas, so a project opens at 1:1. It
    /// used to be a fixed 1200×800 which `Scene` then fitted into whatever
    /// space there was — so a circuit opened at roughly 60% and *stayed*
    /// there. Invisible on line art and obvious the moment there is text:
    /// `Scene` applies a layer transform, which scales already-rasterised
    /// glyphs as a texture, so any factor but 1 blurs them.
    pub(super) fn framed_region(&mut self, ui: &egui::Ui) -> egui::Rect {
        let scene_rect = self.scene_rect;
        let unframed = scene_rect.width() <= 0.0 || scene_rect.height() <= 0.0;
        if !std::mem::take(&mut self.refit_view) && !unframed {
            return scene_rect;
        }

        let canvas = ui.available_size();
        match self.content_rect() {
            Some(content) => {
                let content = content.expand(FIT_MARGIN);
                // Never magnify: a circuit smaller than the canvas is centred
                // at 1:1 rather than blown up to fill it, which would open a
                // two-gate circuit at 4× and blur every label. Only a drawing
                // too big to fit zooms out.
                if content.width() <= canvas.x && content.height() <= canvas.y {
                    egui::Rect::from_center_size(content.center(), canvas)
                } else {
                    content
                }
            }
            None => egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
        }
    }

    /// Which buttons drag the view.
    ///
    /// The middle one always, so there is a way to pan whatever the tool and
    /// no preference to get wrong; the primary one only when the hand is out
    /// or the setting says so, since it otherwise belongs to the rubber band.
    pub(super) fn pan_buttons(&self) -> egui::containers::DragPanButtons {
        let mut buttons = egui::containers::DragPanButtons::MIDDLE;
        if self.pans_on_left_drag() {
            buttons |= egui::containers::DragPanButtons::PRIMARY;
        }
        buttons
    }

    /// Applies a wheel turn to the framed region for the next frame:
    /// shrinking it zooms in.
    ///
    /// Anchored on the pointer, which is what makes zooming feel like it is
    /// following you rather than the window centre. `pivot` is `None` when
    /// the pointer isn't over the canvas, and then the centre is the only
    /// honest answer.
    pub(super) fn zoom_by_wheel(&mut self, wheel: f32, pivot: Option<egui::Pos2>) {
        if wheel == 0.0 {
            return;
        }
        let pivot = pivot.unwrap_or_else(|| self.scene_rect.center());
        let factor = (-wheel * WHEEL_ZOOM_SENSITIVITY).exp();
        self.scene_rect = egui::Rect::from_min_max(
            pivot + (self.scene_rect.min - pivot) * factor,
            pivot + (self.scene_rect.max - pivot) * factor,
        );
    }

    /// Puts the drawing's camera away and takes out the symbol's, or the
    /// other way round — but only when crossing between the two.
    ///
    /// There are two things to look at, not three: the drawing, which the
    /// schematic and the simulation both show, and the symbol, which always
    /// sits on the origin.
    pub(super) fn swap_camera_for(&mut self, was: toolbar::View, now: toolbar::View) {
        let crossing = (was == toolbar::View::Appearance) != (now == toolbar::View::Appearance);
        if crossing {
            std::mem::swap(&mut self.scene_rect, &mut self.idle_scene_rect);
        }
    }
}
