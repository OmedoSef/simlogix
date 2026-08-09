//! The project explorer: the circuits a project contains, drawn as a tree
//! with the project itself at the root and folders in between.
//!
//! Purely a view — it reads the circuit list and reports what the user asked
//! for, leaving `app.rs` to actually do it. The rename buffer is passed in
//! rather than kept here because the edit spans frames and has to survive
//! alongside the rest of the app's state.
//!
//! The folder hierarchy is **derived** here, every frame, from the flat
//! `folders` list and each circuit's own path. No nested structure is kept
//! in memory that could drift from those paths — the same bargain the
//! engine's nets take with the drawing.

use egui::{TextEdit, Ui};

use crate::i18n::Strings;
use crate::project::SavedCircuit;

/// What a rename in progress is renaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    /// The project's library name, at the root of the tree.
    Project,
    Circuit(usize),
    /// A folder, by its full path. The buffer holds only its last segment —
    /// renaming a folder changes its name, it doesn't move it.
    Folder(String),
}

/// What the user asked the tree to do this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeAction {
    /// Edit a different circuit.
    Open(usize),
    /// Add a new, empty circuit in this folder.
    Create {
        folder: String,
    },
    /// Add a new, empty folder inside this one.
    CreateFolder {
        parent: String,
    },
    /// Start renaming — the row turns into a text field.
    BeginRename(RenameTarget),
    /// Take the name currently in the rename buffer.
    CommitRename,
    /// Drop the rename and keep the old name.
    CancelRename,
    Delete(usize),
    DeleteFolder(String),
    /// Drop an instance of this circuit into the one being edited.
    Place(usize),
    /// File a circuit somewhere else.
    MoveCircuit {
        circuit: usize,
        folder: String,
    },
}

/// What a dragged row carries: which circuit is in the user's hand.
#[derive(Debug, Clone, Copy)]
struct DraggedCircuit(usize);

/// One folder of the derived hierarchy, with what sits directly inside it.
struct Node {
    /// Full path — empty for the root.
    path: String,
    /// The last segment, which is what the row shows.
    label: String,
    children: Vec<Node>,
    /// Indices into the project's circuit list.
    circuits: Vec<usize>,
}

/// Everything a row needs that doesn't change as the tree is walked.
/// Bundled so the recursion doesn't take eight parameters.
struct View<'a> {
    strings: &'a Strings,
    circuits: &'a [SavedCircuit],
    folders: &'a [String],
    active: usize,
    reveal_active: bool,
}

/// Draws the tree. `project_name` labels the root (the project's library
/// name, or the file name before it has one), `active` is the circuit
/// currently being edited, and `renaming` holds what's being renamed and
/// the name as typed so far, if any.
///
/// `reveal_active` scrolls the open circuit into view — set the frame it
/// changes, so adding a circuit to a list longer than the panel doesn't
/// silently leave the new one below the fold.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    strings: &Strings,
    project_name: &str,
    folders: &[String],
    circuits: &[SavedCircuit],
    active: usize,
    reveal_active: bool,
    renaming: &mut Option<(RenameTarget, String)>,
) -> Option<TreeAction> {
    let mut action = None;

    // The heading and its buttons sit *outside* the scroll area: they stay
    // put while a long list scrolls under them, and egui's scrollbar floats
    // over its content, so anything clickable at the right edge would end up
    // underneath it.
    ui.horizontal(|ui| {
        ui.heading(strings.circuits_heading);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+").on_hover_text(strings.circuit_new).clicked() {
                action = Some(TreeAction::Create {
                    folder: String::new(),
                });
            }
            if ui.button("🗀").on_hover_text(strings.folder_new).clicked() {
                action = Some(TreeAction::CreateFolder {
                    parent: String::new(),
                });
            }
        });
    });
    ui.separator();

    let view = View {
        strings,
        circuits,
        folders,
        active,
        reveal_active,
    };
    let root = build_tree(folders, circuits);

    egui::ScrollArea::vertical()
        .id_salt("circuit_tree_scroll")
        .show(ui, |ui| {
            // A resizable panel only keeps the size it was given while its
            // content fills it — otherwise it snaps back onto the content and
            // a project with one circuit gets a panel two rows tall.
            ui.set_min_width(ui.available_width());
            ui.set_min_height(ui.available_height());

            if renaming.as_ref().map(|(target, _)| target) == Some(&RenameTarget::Project) {
                // The header is replaced by the field rather than the whole
                // tree collapsing: what's underneath stays listed, so nothing
                // jumps around while the name is being typed.
                if let Some((_, buffer)) = renaming.as_mut() {
                    if let Some(rename_action) = rename_row(ui, buffer) {
                        action = Some(rename_action);
                    }
                }
                ui.indent("project_children", |ui| {
                    if let Some(inner) = node_contents(ui, &view, &root, renaming) {
                        action = Some(inner);
                    }
                });
            } else {
                let header =
                    egui::CollapsingHeader::new(egui::RichText::new(project_name).strong())
                        .id_salt("project_root")
                        .default_open(true)
                        .show(ui, |ui| node_contents(ui, &view, &root, renaming));
                if let Some(Some(inner)) = header.body_returned {
                    action = Some(inner);
                }

                let response = header.header_response;
                highlight_drop_target(ui, &response);
                // Dropping on the root files a circuit at the top level.
                if let Some(dragged) = response.dnd_release_payload::<DraggedCircuit>() {
                    action = Some(TreeAction::MoveCircuit {
                        circuit: dragged.0,
                        folder: String::new(),
                    });
                } else if response.double_clicked() {
                    action = Some(TreeAction::BeginRename(RenameTarget::Project));
                }

                let response = response.on_hover_text(strings.project_library_hint);
                response.context_menu(|ui| {
                    if let Some(menu_action) = folder_menu(ui, strings, "", false) {
                        action = Some(menu_action);
                    }
                });
            }
        });

    action
}

/// Assembles the folder hierarchy from the flat paths.
///
/// Every ancestor of a listed path counts as a folder even when it isn't
/// listed itself, and so does any folder a circuit claims to be in — if the
/// two lists ever disagree, the project still shows everything it holds
/// rather than hiding circuits inside folders that "don't exist".
fn build_tree(folders: &[String], circuits: &[SavedCircuit]) -> Node {
    let mut paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for path in folders
        .iter()
        .chain(circuits.iter().map(|circuit| &circuit.folder))
    {
        // Every prefix as well, so a folder is never orphaned by a missing
        // parent.
        let mut prefix = String::new();
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(segment);
            paths.insert(prefix.clone());
        }
    }

    build_node(String::new(), &paths, circuits)
}

fn build_node(
    path: String,
    paths: &std::collections::BTreeSet<String>,
    circuits: &[SavedCircuit],
) -> Node {
    let children = paths
        .iter()
        .filter(|candidate| is_direct_child(&path, candidate))
        .map(|child| build_node(child.clone(), paths, circuits))
        .collect();
    let own = circuits
        .iter()
        .enumerate()
        .filter(|(_, circuit)| circuit.folder == path)
        .map(|(index, _)| index)
        .collect();

    let label = path.rsplit('/').next().unwrap_or_default().to_string();
    Node {
        path,
        label,
        children,
        circuits: own,
    }
}

/// Whether `path` sits *directly* inside `parent` — one segment deeper, not
/// merely sharing a prefix (`alu2` is not inside `alu`).
fn is_direct_child(parent: &str, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(parent) else {
        return false;
    };
    let rest = if parent.is_empty() {
        rest
    } else {
        let Some(rest) = rest.strip_prefix('/') else {
            return false;
        };
        rest
    };
    !rest.is_empty() && !rest.contains('/')
}

/// What sits inside one folder: its sub-folders, then its circuits.
fn node_contents(
    ui: &mut Ui,
    view: &View<'_>,
    node: &Node,
    renaming: &mut Option<(RenameTarget, String)>,
) -> Option<TreeAction> {
    let mut action = None;

    for child in &node.children {
        if let Some(child_action) = folder_row(ui, view, child, renaming) {
            action = Some(child_action);
        }
    }
    for &index in &node.circuits {
        let Some(circuit) = view.circuits.get(index) else {
            continue;
        };
        if let Some((RenameTarget::Circuit(renaming_index), buffer)) = renaming.as_mut() {
            if *renaming_index == index {
                if let Some(rename_action) = rename_row(ui, buffer) {
                    action = Some(rename_action);
                }
                continue;
            }
        }
        if let Some(row_action) = circuit_row(ui, view, &circuit.name, index) {
            action = Some(row_action);
        }
    }

    action
}

/// One folder: a collapsing header that accepts a dropped circuit.
fn folder_row(
    ui: &mut Ui,
    view: &View<'_>,
    node: &Node,
    renaming: &mut Option<(RenameTarget, String)>,
) -> Option<TreeAction> {
    let mut action = None;

    let is_renaming = renaming.as_ref().map(|(target, _)| target)
        == Some(&RenameTarget::Folder(node.path.clone()));
    if is_renaming {
        if let Some((_, buffer)) = renaming.as_mut() {
            if let Some(rename_action) = rename_row(ui, buffer) {
                action = Some(rename_action);
            }
        }
        ui.indent(("folder_children", node.path.as_str()), |ui| {
            if let Some(inner) = node_contents(ui, view, node, renaming) {
                action = Some(inner);
            }
        });
        return action;
    }

    let header = egui::CollapsingHeader::new(format!("🗀 {}", node.label))
        .id_salt(("folder", node.path.as_str()))
        .default_open(true)
        .show(ui, |ui| node_contents(ui, view, node, renaming));
    if let Some(Some(inner)) = header.body_returned {
        action = Some(inner);
    }

    let response = header.header_response;
    highlight_drop_target(ui, &response);
    if let Some(dragged) = response.dnd_release_payload::<DraggedCircuit>() {
        action = Some(TreeAction::MoveCircuit {
            circuit: dragged.0,
            folder: node.path.clone(),
        });
    } else if response.double_clicked() {
        action = Some(TreeAction::BeginRename(RenameTarget::Folder(
            node.path.clone(),
        )));
    }
    response.context_menu(|ui| {
        if let Some(menu_action) = folder_menu(ui, view.strings, &node.path, true) {
            action = Some(menu_action);
        }
    });

    action
}

/// Rings a folder row while a circuit is held over it.
///
/// The row being dragged isn't painted at the cursor any more (that came
/// free with `dnd_drag_source`, which had to go — see `circuit_row`), so
/// without this there'd be no sign of where a drop would land.
fn highlight_drop_target(ui: &Ui, response: &egui::Response) {
    if response.dnd_hover_payload::<DraggedCircuit>().is_some() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            4.0,
            egui::Stroke::new(1.5, ui.visuals().selection.bg_fill),
            egui::StrokeKind::Outside,
        );
    }
}

/// The context menu shared by the root and any folder. `can_edit` is false
/// for the root, which has no name or existence of its own to act on — it
/// renames the project instead, and can't be deleted at all.
fn folder_menu(ui: &mut Ui, strings: &Strings, path: &str, can_edit: bool) -> Option<TreeAction> {
    let mut action = None;

    if ui.button(strings.circuit_new_here).clicked() {
        action = Some(TreeAction::Create {
            folder: path.to_string(),
        });
        ui.close();
    }
    if ui.button(strings.folder_new_here).clicked() {
        action = Some(TreeAction::CreateFolder {
            parent: path.to_string(),
        });
        ui.close();
    }
    ui.separator();
    if ui.button(strings.circuit_rename).clicked() {
        action = Some(TreeAction::BeginRename(if can_edit {
            RenameTarget::Folder(path.to_string())
        } else {
            RenameTarget::Project
        }));
        ui.close();
    }
    if can_edit {
        let delete = ui
            .button(strings.folder_delete)
            .on_hover_text(strings.folder_delete_hint);
        if delete.clicked() {
            action = Some(TreeAction::DeleteFolder(path.to_string()));
            ui.close();
        }
    }

    action
}

/// One circuit as a selectable row: draggable into a folder, with
/// rename/move/delete on right-click.
fn circuit_row(ui: &mut Ui, view: &View<'_>, name: &str, index: usize) -> Option<TreeAction> {
    let mut action = None;

    let is_active = index == view.active;
    // Bold as well as highlighted: the selectable background alone is faint
    // enough to miss, and "which circuit am I editing?" has to be readable
    // at a glance.
    let label = if is_active {
        egui::RichText::new(name).strong()
    } else {
        egui::RichText::new(name)
    };

    // One response senses everything, rather than `dnd_drag_source`, which
    // registers a *second* widget over the row to catch the drag. That one
    // sits on top and takes the pointer, so a right-click never reached the
    // label underneath and the context menu was unreachable.
    let response = ui
        .selectable_label(is_active, label)
        .interact(egui::Sense::click_and_drag());
    response.dnd_set_drag_payload(DraggedCircuit(index));

    if view.reveal_active && is_active {
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
        action = Some(TreeAction::BeginRename(RenameTarget::Circuit(index)));
    }

    response.context_menu(|ui| {
        // Not offered on the circuit you're already in: a circuit cannot
        // contain itself, and greying it out here says so before the click
        // rather than after it.
        let elsewhere = !is_active;
        let place = ui.add_enabled(elsewhere, egui::Button::new(view.strings.circuit_place));
        if !elsewhere {
            place.on_hover_text(view.strings.circuit_place_self);
        } else if place.clicked() {
            action = Some(TreeAction::Place(index));
            ui.close();
        }
        ui.separator();
        if ui.button(view.strings.circuit_rename).clicked() {
            action = Some(TreeAction::BeginRename(RenameTarget::Circuit(index)));
            ui.close();
        }
        ui.menu_button(view.strings.circuit_move_to, |ui| {
            if ui.button(view.strings.folder_top_level).clicked() {
                action = Some(TreeAction::MoveCircuit {
                    circuit: index,
                    folder: String::new(),
                });
                ui.close();
            }
            for folder in view.folders {
                if ui.button(folder).clicked() {
                    action = Some(TreeAction::MoveCircuit {
                        circuit: index,
                        folder: folder.clone(),
                    });
                    ui.close();
                }
            }
        });
        // Deleting the only circuit would leave the project with nothing to
        // edit, so the entry stays visible but disabled — with the reason on
        // hover, rather than silently doing nothing.
        let can_delete = view.circuits.len() > 1;
        let delete = ui.add_enabled(can_delete, egui::Button::new(view.strings.circuit_delete));
        if !can_delete {
            delete.on_hover_text(view.strings.circuit_delete_last);
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

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit(name: &str, folder: &str) -> SavedCircuit {
        SavedCircuit {
            name: name.to_string(),
            folder: folder.to_string(),
            components: Vec::new(),
            wires: Vec::new(),
            appearance: None,
        }
    }

    #[test]
    fn a_shared_prefix_is_not_a_parent() {
        assert!(is_direct_child("alu", "alu/adder"));
        // The case this exists to get right: sharing leading characters is
        // not the same as sitting inside.
        assert!(!is_direct_child("alu", "alu2"));
        // A grandchild isn't a direct child.
        assert!(!is_direct_child("alu", "alu/decode/rom"));
        assert!(is_direct_child("", "alu"));
        assert!(!is_direct_child("", "alu/adder"));
    }

    #[test]
    fn the_hierarchy_is_derived_from_the_paths() {
        let folders = vec!["alu".to_string(), "alu/decode".to_string()];
        let circuits = vec![circuit("top", ""), circuit("adder", "alu")];

        let root = build_tree(&folders, &circuits);

        assert_eq!(root.circuits, vec![0]);
        assert_eq!(root.children.len(), 1);
        let alu = &root.children[0];
        assert_eq!(alu.label, "alu");
        assert_eq!(alu.circuits, vec![1]);
        assert_eq!(alu.children.len(), 1);
        assert_eq!(alu.children[0].label, "decode");
    }

    #[test]
    fn a_circuit_in_an_unlisted_folder_still_shows_up() {
        // The two lists disagreeing must not hide a circuit: the folder is
        // inferred from the circuit's own path, along with its ancestors.
        let circuits = vec![circuit("rom", "alu/decode")];

        let root = build_tree(&[], &circuits);

        assert_eq!(root.children.len(), 1);
        let alu = &root.children[0];
        assert_eq!(alu.label, "alu");
        assert_eq!(alu.children[0].circuits, vec![0]);
    }

    #[test]
    fn an_empty_folder_still_gets_a_row() {
        let root = build_tree(&["scratch".to_string()], &[]);

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].label, "scratch");
        assert!(root.children[0].circuits.is_empty());
    }
}
