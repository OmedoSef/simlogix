//! The shortcuts window: everything you can do that nothing on screen says.
//!
//! Mouse gestures sit alongside the key chords rather than in a list of
//! their own, because in this editor they're the *less* discoverable half —
//! cutting a wire with a right-click on a segment, joining two loose ends by
//! dropping one on the other, finishing a wire with Enter. A window called
//! "keyboard shortcuts" would have left out the part that needed saying.
//!
//! The text lives in `i18n.rs` with the rest of it. This file only lays it
//! out.

use egui::{Context, Ui};

use crate::i18n::Strings;

/// Draws the window while `open`, and lets its close button clear the flag.
pub fn show(ctx: &Context, strings: &Strings, open: &mut bool) {
    egui::Window::new(strings.shortcuts_title)
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_width(430.0)
        .show(ctx, |ui| {
            // Long enough to overflow a small window, so it scrolls rather
            // than growing past the screen.
            egui::ScrollArea::vertical()
                .max_height(460.0)
                .show(ui, |ui| sections(ui, strings));
        });
}

fn sections(ui: &mut Ui, strings: &Strings) {
    for (index, section) in strings.help_sections.iter().enumerate() {
        if index > 0 {
            ui.add_space(10.0);
        }
        ui.label(egui::RichText::new(section.title).strong());
        ui.add_space(2.0);

        // A grid keeps the two columns lined up across a section, which is
        // what makes the list scannable rather than merely present.
        egui::Grid::new(("shortcuts", index))
            .num_columns(2)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (keys, what) in section.rows {
                    ui.label(egui::RichText::new(*keys).monospace());
                    ui.label(*what);
                    ui.end_row();
                }
            });
    }
}
