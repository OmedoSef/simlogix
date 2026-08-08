//! The project explorer: the circuits a project contains, drawn as a tree
//! with the project itself at the root.
//!
//! Purely a view — it reads the circuit list and reports what the user asked
//! for, leaving `app.rs` to actually do it. The rename buffer is passed in
//! rather than kept here because the edit spans frames and has to survive
//! alongside the rest of the app's state.

use egui::{TextEdit, Ui};

use crate::i18n::Strings;
use crate::project::SavedCircuit;

/// What the user asked the tree to do this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    /// Edit a different circuit.
    Open(usize),
    /// Add a new, empty circuit to the project.
    Create,
    /// Start renaming a circuit — the row turns into a text field.
    BeginRename(usize),
    /// Take the name currently in the rename buffer.
    CommitRename,
    /// Drop the rename and keep the old name.
    CancelRename,
    Delete(usize),
}

/// Draws the tree. `project_name` labels the root (the file name, or
/// "Untitled" before the project has ever been saved), `active` is the
/// circuit currently being edited, and `renaming` holds the row being
/// renamed and the name as typed so far, if any.
///
/// `reveal_active` scrolls the open circuit into view — set the frame it
/// changes, so adding a circuit to a list longer than the panel doesn't
/// silently leave the new one below the fold.
pub fn show(
    ui: &mut Ui,
    strings: &Strings,
    project_name: &str,
    circuits: &[SavedCircuit],
    active: usize,
    reveal_active: bool,
    renaming: &mut Option<(usize, String)>,
) -> Option<TreeAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.heading(strings.circuits_heading);
        // Pushed to the trailing edge so the heading doesn't jump around as
        // the panel is resized.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+").on_hover_text(strings.circuit_new).clicked() {
                action = Some(TreeAction::Create);
            }
        });
    });

    egui::CollapsingHeader::new(egui::RichText::new(project_name).strong())
        .id_salt("project_root")
        .default_open(true)
        .show(ui, |ui| {
            for (index, circuit) in circuits.iter().enumerate() {
                if let Some((renaming_index, buffer)) = renaming.as_mut() {
                    if *renaming_index == index {
                        if let Some(rename_action) = rename_row(ui, buffer) {
                            action = Some(rename_action);
                        }
                        continue;
                    }
                }
                let is_active = index == active;
                if let Some(row_action) = circuit_row(
                    ui,
                    strings,
                    &circuit.name,
                    index,
                    is_active,
                    reveal_active && is_active,
                    circuits.len() > 1,
                ) {
                    action = Some(row_action);
                }
            }
        });

    action
}

/// One circuit as a selectable row, with rename/delete on right-click.
fn circuit_row(
    ui: &mut Ui,
    strings: &Strings,
    name: &str,
    index: usize,
    is_active: bool,
    reveal: bool,
    can_delete: bool,
) -> Option<TreeAction> {
    let mut action = None;

    // Bold as well as highlighted: the selectable background alone is faint
    // enough to miss, and "which circuit am I editing?" has to be readable
    // at a glance.
    let label = if is_active {
        egui::RichText::new(name).strong()
    } else {
        egui::RichText::new(name)
    };
    let response = ui.selectable_label(is_active, label);
    if reveal {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        action = Some(TreeAction::Open(index));
    }
    // Double-clicking a name to edit it is the usual gesture in a tree, and
    // saves going through the context menu for the common case.
    if response.double_clicked() {
        action = Some(TreeAction::BeginRename(index));
    }

    response.context_menu(|ui| {
        if ui.button(strings.circuit_rename).clicked() {
            action = Some(TreeAction::BeginRename(index));
            ui.close();
        }
        // Deleting the only circuit would leave the project with nothing to
        // edit, so the entry stays visible but disabled — with the reason on
        // hover, rather than silently doing nothing.
        let delete = ui.add_enabled(can_delete, egui::Button::new(strings.circuit_delete));
        if !can_delete {
            delete.on_hover_text(strings.circuit_delete_last);
        } else if delete.clicked() {
            action = Some(TreeAction::Delete(index));
            ui.close();
        }
    });

    action
}

/// The row being renamed: a text field in place of the label.
fn rename_row(ui: &mut Ui, buffer: &mut String) -> Option<TreeAction> {
    let response = ui.add(TextEdit::singleline(buffer).desired_width(f32::INFINITY));

    // Checked before `lost_focus`, because Escape drops the field's focus
    // too — without this, backing out would commit the half-typed name.
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        return Some(TreeAction::CancelRename);
    }
    if response.lost_focus() {
        // Enter, or a click somewhere else: either way, take what's typed.
        return Some(TreeAction::CommitRename);
    }
    if !response.has_focus() {
        // The field only appeared this frame — put the caret in it.
        response.request_focus();
    }
    None
}
