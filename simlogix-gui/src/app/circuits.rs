//! Circuits and folders: creating, renaming, re-filing and deleting them.
//!
//! Split out of `app.rs` as one subject. These touch the document and never
//! the canvas — no widget, no pointer, no layer — which is what makes them a
//! seam rather than an arbitrary cut.
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use crate::i18n::Strings;
use crate::palette::ComponentKind;
use crate::project::SavedCircuit;

use super::SimLogixApp;

impl SimLogixApp {
    /// Opens a different circuit for editing. The one being left is folded
    /// back into `circuits` first (that's what `to_project` does), so its
    /// layout survives the switch. Not an edit: nothing about the document
    /// changes, only which part of it is on screen.
    pub(super) fn switch_to(&mut self, index: usize) {
        if index == self.active || index >= self.circuits.len() {
            return;
        }
        let project = self.to_project();
        self.reopen(&project, index);
        self.reveal_active = true;
        self.refit_view = true;
    }

    /// Adds an empty circuit to the project and opens it, filed in
    /// `in_folder` (empty for the top level).
    pub(super) fn create_circuit(&mut self, in_folder: String) {
        self.record_edit();
        let name = self.unique_name_in(
            &in_folder,
            Strings::for_language(self.language).circuit_default_name,
        );

        let mut project = self.to_project();
        project.circuits.push(SavedCircuit {
            name,
            folder: in_folder,
            components: Vec::new(),
            wires: Vec::new(),
            appearance: None,
        });
        let open = project.circuits.len() - 1;
        self.reopen(&project, open);
        self.reveal_active = true;
        self.refit_view = true;
    }

    /// The path of `path`'s parent folder, empty for a top-level one.
    fn parent_path(path: &str) -> &str {
        path.rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("")
    }

    /// Adds an empty folder inside `parent`.
    /// Returns the path it created, so the caller can put the name straight
    /// into edit — naming a thing is part of making it, and a folder called
    /// "Folder 3" that you have to go and rename is a step nobody wanted.
    pub(super) fn create_folder(&mut self, parent: &str) -> String {
        self.record_edit();
        let base = Strings::for_language(self.language).folder_default_name;
        let path = self.unique_folder_path(parent, base);
        self.folders.push(path.clone());
        path
    }

    /// Renames a folder's own segment, carrying everything filed under it
    /// along — sub-folders and circuits alike, since a path is a prefix.
    pub(super) fn rename_folder(&mut self, path: &str, leaf: &str) {
        let leaf = leaf.trim();
        // A `/` here would silently move the folder somewhere else rather
        // than rename it, which isn't what the gesture says it does.
        if leaf.is_empty() || leaf.contains('/') {
            return;
        }
        let parent = Self::parent_path(path);
        let new_path = if parent.is_empty() {
            leaf.to_string()
        } else {
            format!("{parent}/{leaf}")
        };
        if new_path == path {
            return;
        }
        if self.folders.iter().any(|folder| folder == &new_path) {
            let strings = Strings::for_language(self.language);
            self.error = Some(strings.circuit_name_taken.replace("{}", leaf));
            return;
        }

        self.record_edit();
        let prefix = format!("{path}/");
        let repath = |value: &mut String| {
            if value.as_str() == path {
                value.clone_from(&new_path);
            } else if let Some(rest) = value.clone().strip_prefix(&prefix) {
                *value = format!("{new_path}/{rest}");
            }
        };
        self.folders.iter_mut().for_each(repath);
        // Every circuit filed under this folder changes path, so every
        // instance of any of them has to follow — including ones sitting in
        // a part of the project this rename never went near.
        self.repointing_paths(|app| {
            app.circuits
                .iter_mut()
                .for_each(|circuit| repath(&mut circuit.folder));
        });
    }

    /// Removes a folder, moving what was in it up into the folder that held
    /// it.
    ///
    /// Deleting the contents along with it is the other option, and it's the
    /// one that loses work: filing something away is a presentation choice,
    /// so undoing that choice must not be able to take circuits with it.
    pub(super) fn delete_folder(&mut self, path: &str) {
        if !self.folders.iter().any(|folder| folder == path) {
            return;
        }
        self.record_edit();

        let parent = Self::parent_path(path).to_string();
        let prefix = format!("{path}/");
        let lift = |value: &mut String| {
            if value.as_str() == path {
                value.clone_from(&parent);
            } else if let Some(rest) = value.clone().strip_prefix(&prefix) {
                *value = if parent.is_empty() {
                    rest.to_string()
                } else {
                    format!("{parent}/{rest}")
                };
            }
        };
        self.folders.retain(|folder| folder != path);
        self.folders.iter_mut().for_each(lift);
        // Lifting re-paths the contents, and a name that collides on arrival
        // is renamed — two path changes for one gesture, both of which
        // instances have to follow.
        self.repointing_paths(|app| {
            app.circuits
                .iter_mut()
                .for_each(|circuit| lift(&mut circuit.folder));
            app.resolve_name_clashes();
        });
    }

    /// Gives a free name to any circuit that has just landed in a folder
    /// where its own name was already taken.
    ///
    /// Only lifting can cause that — every other path refuses a clash up
    /// front. Refusing here instead would let one name collision block the
    /// deletion of a folder, which is the wrong thing to be stuck on.
    fn resolve_name_clashes(&mut self) {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for index in 0..self.circuits.len() {
            let key = (
                self.circuits[index].folder.clone(),
                self.circuits[index].name.clone(),
            );
            if seen.insert(key) {
                continue;
            }
            let (folder, base) = (
                self.circuits[index].folder.clone(),
                self.circuits[index].name.clone(),
            );
            let fresh = self.unique_name_in(&folder, &base);
            seen.insert((folder, fresh.clone()));
            self.circuits[index].name = fresh;
        }
    }

    /// Files a circuit in a different folder.
    ///
    /// Refused if a circuit of the same name is already there: two circuits
    /// in one folder would be indistinguishable, since a reference is
    /// `library:folder/name`.
    pub(super) fn move_circuit(&mut self, index: usize, folder: String) {
        let Some(circuit) = self.circuits.get(index) else {
            return;
        };
        if circuit.folder == folder {
            return;
        }
        let name = circuit.name.clone();
        if self
            .circuits
            .iter()
            .any(|other| other.folder == folder && other.name == name)
        {
            let strings = Strings::for_language(self.language);
            self.error = Some(strings.circuit_name_taken.replace("{}", &name));
            return;
        }

        self.record_edit();
        self.repointing_paths(|app| app.circuits[index].folder = folder);
    }

    /// A folder path inside `parent` that isn't taken yet.
    fn unique_folder_path(&self, parent: &str, base: &str) -> String {
        let join = |leaf: &str| {
            if parent.is_empty() {
                leaf.to_string()
            } else {
                format!("{parent}/{leaf}")
            }
        };
        let taken = |path: &String| self.folders.contains(path);

        let first = join(base);
        if !taken(&first) {
            return first;
        }
        (2..=u32::MAX)
            .map(|n| join(&format!("{base} {n}")))
            .find(|path| !taken(path))
            .unwrap_or(first)
    }

    /// Removes a circuit from the project. Refused on the last one: there
    /// has to be something left to edit.
    pub(super) fn delete_circuit(&mut self, index: usize) {
        if index >= self.circuits.len() || self.circuits.len() <= 1 {
            return;
        }
        // Unlike a rename there is no new path to point instances at, so
        // going ahead would leave them naming a circuit that isn't there.
        // Refusing keeps the choice with whoever knows what those instances
        // were for.
        let doomed = self.circuits[index].path();
        let users = self.users_of(&doomed, index);
        if !users.is_empty() {
            let strings = Strings::for_language(self.language);
            self.error = Some(
                strings
                    .circuit_in_use
                    .replacen("{}", &self.circuits[index].name, 1)
                    .replace("{}", &users.join(", ")),
            );
            return;
        }
        self.record_edit();

        let mut project = self.to_project();
        project.circuits.remove(index);
        // Stay on the same circuit when it wasn't the one deleted — its
        // index shifts down if it sat after the gap. Deleting the open one
        // falls onto whichever circuit takes its place.
        let open = if self.active > index {
            self.active - 1
        } else {
            self.active.min(project.circuits.len() - 1)
        };
        self.reopen(&project, open);
        self.reveal_active = true;
        self.refit_view = true;
    }

    /// Runs an edit that may change what circuits are called, then points
    /// every instance at wherever its circuit ended up.
    ///
    /// A reference is a path, so *renaming or re-filing a circuit breaks
    /// every instance of it* unless they are carried along. Doing it here,
    /// around the edit, rather than in each of the four operations that can
    /// change a path, is what stops the fifth one from forgetting.
    ///
    /// Paths are paired by index, which holds because none of these edits
    /// reorder the list — they rename, re-file, or lift out of a folder.
    fn repointing_paths(&mut self, edit: impl FnOnce(&mut Self)) {
        let before: Vec<String> = self.circuits.iter().map(|c| c.path()).collect();
        edit(self);
        let after: Vec<String> = self.circuits.iter().map(|c| c.path()).collect();
        if before.len() != after.len() {
            return;
        }
        for (old, new) in before.iter().zip(&after) {
            if old != new {
                self.repoint_instances(old, new);
            }
        }
    }

    /// Rewrites every reference to `from` so it names `to` instead — in the
    /// circuits held in their saved form, and in the one currently open,
    /// whose live state isn't in that list.
    pub(super) fn repoint_instances(&mut self, from: &str, to: &str) {
        let (was, now) = (
            ComponentKind::Circuit(from.to_string()),
            ComponentKind::Circuit(to.to_string()),
        );
        for circuit in &mut self.circuits {
            for component in &mut circuit.components {
                if component.kind == was {
                    component.kind = now.clone();
                }
            }
        }
        for placed in &mut self.placed {
            placed.repoint_instance(from, to);
        }
    }

    /// The circuits holding an instance of `path`, by name.
    ///
    /// Checked before deleting one: unlike a rename there is no new path to
    /// point at, so the only options are to break the instances or to refuse
    /// — and refusing is the one that doesn't lose work.
    /// `ignoring` is the circuit being deleted itself: its own contents go
    /// with it, so an instance in there isn't a reason to keep it. Skipped
    /// by index rather than by name, because two folders can each hold a
    /// circuit called the same thing and only one of them is doomed.
    fn users_of(&self, path: &str, ignoring: usize) -> Vec<String> {
        let wanted = ComponentKind::Circuit(path.to_string());
        let mut users: Vec<String> = self
            .circuits
            .iter()
            .enumerate()
            .filter(|(index, circuit)| {
                // The open circuit's saved entry is stale — its live state
                // is in `placed`, checked below.
                *index != self.active
                    && *index != ignoring
                    && circuit.components.iter().any(|c| c.kind == wanted)
            })
            .map(|(_, circuit)| circuit.name.clone())
            .collect();
        if self.active != ignoring && self.placed.iter().any(|p| p.kind() == wanted) {
            if let Some(open) = self.circuits.get(self.active) {
                users.push(open.name.clone());
            }
        }
        users
    }

    /// Renames a circuit. An empty name, or one another circuit in the same
    /// folder already has, is refused rather than quietly altered — the name
    /// is half of how a circuit will be referred to once one can be placed
    /// inside another.
    pub(super) fn rename_circuit(&mut self, index: usize, name: &str) {
        let name = name.trim();
        let Some(current) = self.circuits.get(index) else {
            return;
        };
        if name.is_empty() || name == current.name {
            return;
        }
        let folder = current.folder.clone();
        if self
            .circuits
            .iter()
            .any(|circuit| circuit.folder == folder && circuit.name == name)
        {
            let strings = Strings::for_language(self.language);
            self.error = Some(strings.circuit_name_taken.replace("{}", name));
            return;
        }

        self.record_edit();
        self.repointing_paths(|app| app.circuits[index].name = name.to_string());
    }

    /// `base` if no circuit *in `folder`* is using it, else `base 2`,
    /// `base 3`, and so on.
    ///
    /// Names only have to be distinct within their own folder, because a
    /// circuit is referred to as `library:folder/name` — the folder is part
    /// of what identifies it.
    fn unique_name_in(&self, folder: &str, base: &str) -> String {
        let taken = |name: &str| {
            self.circuits
                .iter()
                .any(|circuit| circuit.folder == folder && circuit.name == name)
        };
        if !taken(base) {
            return base.to_string();
        }
        (2..=u32::MAX)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_string())
    }
}
