//! UI text in English, French, and Italian. Detected once from the OS locale
//! at startup (see [`Language::detect_from_os`]), overridable from the
//! Settings menu. Component/pin technical abbreviations (NMOS, PMOS, GND,
//! PWR, and pin names like G/S/D/IN/OUT) are treated as universal and left
//! untranslated — only whole-word labels and UI chrome are translated.
//!
//! Not tied to save/load: `ComponentKind`'s serialized form is its Rust enum
//! variant name (via `derive(Serialize)`), independent of whatever a
//! `Strings` catalog returns for display — changing the UI language never
//! changes what a project file means.

use crate::palette::ComponentKind;

/// One block of the shortcuts window: a heading and its rows.
///
/// Kept here with the rest of the UI text rather than next to the window
/// that draws it — one home for everything translated is worth more than a
/// tidier `help.rs`, since a second home is how translations drift.
pub struct HelpSection {
    pub title: &'static str,
    /// `(the keys or the gesture, what it does)`.
    ///
    /// The left column is translated too, which is easy to get wrong in the
    /// other direction: modifiers keep their universal names (`Ctrl`,
    /// `Shift`), but the keys that are physically labelled differently do
    /// not — a French keyboard says `Suppr`, `Entrée`, `Échap`. Writing
    /// `Delete` there would be a list of names that aren't on the keyboard
    /// in front of you. Gestures ("Wheel", "Middle drag") are plain prose
    /// and translate like any other.
    pub rows: &'static [(&'static str, &'static str)],
}

/// Which language the UI is currently displayed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    English,
    French,
    Italian,
}

impl Language {
    /// This language's own name, for the picker in Settings.
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
            Language::Italian => "Italiano",
        }
    }

    /// Picks a language from the OS locale (`LC_ALL`/`LC_MESSAGES`/`LANG`),
    /// defaulting to English if it's unset or doesn't match one of the three
    /// supported languages.
    pub fn detect_from_os() -> Self {
        let locale = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_lowercase();

        if locale.starts_with("fr") {
            Self::French
        } else if locale.starts_with("it") {
            Self::Italian
        } else {
            Self::English
        }
    }
}

/// Every user-facing string, for one language. Get one with
/// [`Strings::for_language`].
pub struct Strings {
    pub menu_file: &'static str,
    pub menu_file_new: &'static str,
    pub menu_file_open: &'static str,
    pub menu_file_save: &'static str,
    pub menu_file_save_as: &'static str,
    pub menu_file_quit: &'static str,
    pub menu_edit: &'static str,
    pub menu_edit_undo: &'static str,
    pub menu_edit_redo: &'static str,
    pub menu_edit_copy: &'static str,
    pub menu_edit_paste: &'static str,
    pub menu_simulation: &'static str,
    pub menu_simulation_run: &'static str,
    pub menu_simulation_pause: &'static str,
    pub menu_simulation_signals: &'static str,
    pub status_signals_hidden: &'static str,
    pub menu_settings: &'static str,
    pub menu_settings_theme: &'static str,
    pub menu_settings_language: &'static str,
    pub settings_reset: &'static str,
    pub settings_reset_hint: &'static str,
    pub menu_help: &'static str,
    pub menu_help_about: &'static str,
    pub menu_help_shortcuts: &'static str,
    pub shortcuts_title: &'static str,
    pub help_sections: &'static [HelpSection],

    pub circuits_heading: &'static str,
    /// Hover text on the tree's root: what the project's library name is
    /// for, and how to change it.
    pub project_library_hint: &'static str,
    pub circuit_new: &'static str,
    pub circuit_new_here: &'static str,
    pub circuit_move_to: &'static str,
    pub folder_new: &'static str,
    pub folder_new_here: &'static str,
    pub folder_delete: &'static str,
    /// What happens to a deleted folder's contents, on hover.
    pub folder_delete_hint: &'static str,
    pub folder_top_level: &'static str,
    /// Base name for a freshly created folder — see `circuit_default_name`.
    pub folder_default_name: &'static str,
    pub circuit_place: &'static str,
    pub circuit_place_self: &'static str,
    pub circuit_rename: &'static str,
    pub circuit_delete: &'static str,
    /// Why deleting is refused on the last remaining circuit.
    pub circuit_delete_last: &'static str,
    /// Contains a literal `{}` for the name that was already taken. Used
    /// wherever a name has to be distinct — a circuit within its folder, a
    /// folder within its parent.
    pub circuit_name_taken: &'static str,
    pub circuit_in_use: &'static str,
    /// Base name for a freshly created circuit — a number is appended when
    /// that name is already in use. Saved into the project file as typed,
    /// like any other name, so switching the UI language later doesn't
    /// rename anything.
    pub circuit_default_name: &'static str,

    pub properties_heading: &'static str,
    pub property_variant: &'static str,
    pub property_name: &'static str,
    pub property_name_hint: &'static str,
    pub property_switch_on: &'static str,
    pub property_switch_on_hint: &'static str,
    pub property_pressed: &'static str,
    pub property_pressed_hint: &'static str,
    pub property_color: &'static str,
    pub property_tri_state: &'static str,
    pub property_tri_state_hint: &'static str,
    pub property_initial: &'static str,
    pub port_level_undriven: &'static str,
    pub port_level_low: &'static str,
    pub port_level_high: &'static str,
    pub property_wire: &'static str,
    pub property_wire_color_hint: &'static str,
    pub property_reset: &'static str,
    pub properties_none_selected: &'static str,

    pub palette_heading: &'static str,
    /// Contains a literal `{}` for the component name — fill it in with
    /// `.replace("{}", name)`, not `format!` (the template isn't a
    /// compile-time literal).
    pub palette_click_to_place: &'static str,
    pub tool_select: &'static str,
    pub tool_wire: &'static str,
    pub tool_marquee: &'static str,
    pub tool_pan: &'static str,
    pub menu_settings_left_drag: &'static str,
    pub settings_left_drag_select: &'static str,
    pub settings_left_drag_pan: &'static str,
    pub category_interface: &'static str,
    pub category_sources: &'static str,
    pub category_outputs: &'static str,
    pub category_transistors: &'static str,
    pub category_gates: &'static str,
    pub category_memory: &'static str,
    pub category_buses: &'static str,

    pub component_button: &'static str,
    pub component_switch: &'static str,
    pub component_led: &'static str,
    pub component_nmos: &'static str,
    pub component_pmos: &'static str,
    pub component_ground: &'static str,
    pub component_power: &'static str,
    pub component_probe: &'static str,
    pub component_clock: &'static str,
    pub component_and: &'static str,
    pub component_or: &'static str,
    pub component_nand: &'static str,
    pub component_nor: &'static str,
    pub component_xor: &'static str,
    pub component_xnor: &'static str,
    pub component_not: &'static str,
    pub component_buffer: &'static str,
    pub component_sr_latch: &'static str,
    pub component_tri_state: &'static str,
    pub component_bus_transceiver: &'static str,
    pub component_bus_transceiver_oe: &'static str,
    pub component_input_port: &'static str,
    pub component_output_port: &'static str,
    pub component_inout_port: &'static str,

    pub hint_rotate_delete_component: &'static str,
    pub hint_delete_wire: &'static str,
    /// Contains a literal `{}` for how many things are selected.
    pub hint_selection: &'static str,
    pub hint_wiring: &'static str,

    pub about_title: &'static str,
    pub about_body: &'static str,
    /// Contains a literal `{}` for the version number — see
    /// `palette_click_to_place` on how to fill it in.
    pub about_version: &'static str,

    /// Shown in the window title bar when the circuit has never been saved.
    pub title_untitled: &'static str,

    pub confirm_discard_title: &'static str,
    pub confirm_discard_body: &'static str,
    pub confirm_discard_save: &'static str,
    pub confirm_discard_discard: &'static str,
    pub confirm_discard_cancel: &'static str,

    pub status_paused: &'static str,
    /// Contains a literal `{}` for the net number that wouldn't settle.
    pub status_unstable: &'static str,

    pub error_title: &'static str,
    /// Contains a literal `{}` for the underlying error message.
    pub error_save_failed: &'static str,
    /// Contains a literal `{}` for the underlying error message.
    pub error_open_failed: &'static str,
    /// Contains a literal `{}` for the circuit that contains itself.
    pub error_circuit_recursion: &'static str,
}

impl Strings {
    pub fn for_language(language: Language) -> &'static Strings {
        match language {
            Language::English => &ENGLISH,
            Language::French => &FRENCH,
            Language::Italian => &ITALIAN,
        }
    }

    /// The display name for a palette entry / a placed component's on-canvas
    /// box label.
    /// A circuit instance is named by the circuit it refers to, which isn't
    /// a translatable string — hence the borrowed return.
    pub fn component_kind_label<'a>(&'a self, kind: &'a ComponentKind) -> &'a str {
        if let ComponentKind::Circuit(path) = kind {
            return path;
        }
        match kind {
            ComponentKind::Button => self.component_button,
            ComponentKind::Switch => self.component_switch,
            ComponentKind::Led => self.component_led,
            ComponentKind::NTransistor => self.component_nmos,
            ComponentKind::PTransistor => self.component_pmos,
            ComponentKind::Ground => self.component_ground,
            ComponentKind::Power => self.component_power,
            ComponentKind::Probe => self.component_probe,
            ComponentKind::Clock => self.component_clock,
            ComponentKind::And => self.component_and,
            ComponentKind::Or => self.component_or,
            ComponentKind::Nand => self.component_nand,
            ComponentKind::Nor => self.component_nor,
            ComponentKind::Xor => self.component_xor,
            ComponentKind::Xnor => self.component_xnor,
            ComponentKind::Not => self.component_not,
            ComponentKind::Buffer => self.component_buffer,
            ComponentKind::SrLatch => self.component_sr_latch,
            // Handled above by returning the path itself.
            ComponentKind::Circuit(path) => path,
            ComponentKind::TriStateBuffer => self.component_tri_state,
            ComponentKind::BusTransceiver => self.component_bus_transceiver,
            ComponentKind::BusTransceiverOe => self.component_bus_transceiver_oe,
            ComponentKind::InputPort => self.component_input_port,
            ComponentKind::OutputPort => self.component_output_port,
            ComponentKind::InOutPort => self.component_inout_port,
        }
    }
}

static ENGLISH: Strings = Strings {
    menu_file: "File",
    menu_file_new: "New",
    menu_file_open: "Open Project…",
    menu_file_save: "Save",
    menu_file_save_as: "Save As…",
    menu_file_quit: "Quit",
    menu_edit: "Edit",
    menu_edit_undo: "Undo",
    menu_edit_redo: "Redo",
    menu_edit_copy: "Copy",
    menu_edit_paste: "Paste",
    menu_simulation: "Simulation",
    menu_simulation_run: "Run",
    menu_simulation_pause: "Pause",
    menu_simulation_signals: "Show signal state",
    status_signals_hidden: "Signal state hidden — C shows it again",
    menu_settings: "Settings",
    menu_settings_theme: "Theme",
    menu_settings_language: "Language",
    settings_reset: "Reset to defaults",
    settings_reset_hint: "Theme, language and left drag go back to their starting values.",
    menu_help: "?",
    menu_help_about: "About",
    menu_help_shortcuts: "Shortcuts and gestures",
    shortcuts_title: "Shortcuts and gestures",
    help_sections: &[
        HelpSection {
            title: "Files",
            rows: &[
            ("Ctrl+N", "New project"),
            ("Ctrl+O", "Open a project"),
            ("Ctrl+S", "Save"),
            ("Ctrl+Shift+S", "Save as"),
            ],
        },
        HelpSection {
            title: "Editing",
            rows: &[
            ("Ctrl+Z", "Undo"),
            ("Ctrl+Shift+Z / Ctrl+Y", "Redo"),
            ("Ctrl+C", "Copy the selection"),
            ("Ctrl+V", "Paste"),
            ("Delete", "Remove what is selected"),
            ("R", "Rotate the selected components"),
            ],
        },
        HelpSection {
            title: "Canvas",
            rows: &[
            ("Wheel", "Zoom, while over the canvas"),
            ("Middle drag", "Move the view"),
            ("Left drag", "Selection rectangle, or the view — see Settings"),
            ("Shift+click", "Add to or remove from the selection"),
            ("Shift while placing", "Keep the component loaded for the next click"),
            ("Escape / right-click", "Back out one step"),
            ],
        },
        HelpSection {
            title: "Wires",
            rows: &[
            ("Click a pin", "Start a wire"),
            ("Click", "Add a corner to the wire being drawn"),
            ("Enter", "Finish the wire, leaving the end loose"),
            ("Double-click a wire", "Add a point to it"),
            ("Right-click a point", "Remove that point"),
            ("Right-click a segment", "Cut the wire there"),
            ("Drag a loose end onto another", "Join the two wires"),
            ],
        },
        HelpSection {
            title: "Simulation",
            rows: &[
            ("Space", "Run or pause the simulation"),
            ("C", "Show or hide the signal state on wires"),
            ],
        },
    ],

    circuits_heading: "Circuits",
    project_library_hint: "The name other projects use to refer to this one's circuits. Double-click to change it.",
    circuit_new: "New circuit",
    circuit_new_here: "New circuit here",
    circuit_move_to: "Move to",
    folder_new: "New folder",
    folder_new_here: "New folder here",
    folder_delete: "Delete folder",
    folder_delete_hint: "What is inside moves up to the folder above.",
    folder_top_level: "Top level",
    folder_default_name: "folder",
    circuit_place: "Place in this circuit",
    circuit_place_self: "A circuit can't contain itself.",
    circuit_rename: "Rename",
    circuit_delete: "Delete",
    circuit_delete_last: "A project has to keep at least one circuit.",
    circuit_name_taken: "\"{}\" is already taken here.",
    circuit_in_use: "\"{}\" can't be deleted: it is used in {}.",
    circuit_default_name: "circuit",

    properties_heading: "Properties",
    property_variant: "Type",
    property_name: "Name",
    property_name_hint: "shown under the symbol",
    property_switch_on: "Closed",
    property_switch_on_hint: "Where the switch is now, and how the project is saved — flipping it counts as an edit.",
    property_pressed: "Pressed at rest",
    property_pressed_hint: "The button rests pressed, so clicking it releases it instead.",
    property_color: "Colour",
    property_tri_state: "Three-state",
    property_tri_state_hint: "Clicking the port can also leave it undriven, and the interface admits that state.",
    property_initial: "Resting value",
    port_level_undriven: "Undriven",
    port_level_low: "Low",
    port_level_high: "High",
    property_wire: "Wire",
    property_wire_color_hint: "Applies to the whole net, as a casing around the signal colour.",
    property_reset: "Reset",
    properties_none_selected: "Select a component to see its properties.",

    palette_heading: "Palette",
    palette_click_to_place:
        "Click the canvas to place a {} — R turns it, Shift places several",
    tool_select: "Select",
    tool_wire: "Draw wire",
    tool_marquee: "Selection rectangle",
    tool_pan: "Pan the view",
    menu_settings_left_drag: "Left drag",
    settings_left_drag_select: "Sweeps a selection",
    settings_left_drag_pan: "Moves the view",
    category_interface: "Interface",
    category_sources: "Sources",
    category_outputs: "Outputs",
    category_transistors: "Transistors",
    category_gates: "Gates",
    category_memory: "Memory",
    category_buses: "Buses",

    component_button: "Button",
    component_switch: "Switch",
    component_led: "LED",
    component_nmos: "NMOS",
    component_pmos: "PMOS",
    component_ground: "GND",
    component_power: "PWR",
    component_probe: "Probe",
    component_clock: "Clock",
    component_and: "AND",
    component_or: "OR",
    component_nand: "NAND",
    component_nor: "NOR",
    component_xor: "XOR",
    component_xnor: "XNOR",
    component_not: "NOT",
    component_buffer: "Buffer",
    component_sr_latch: "SR latch",
    component_tri_state: "Tri-state buffer",
    component_bus_transceiver: "Bus transceiver (EN)",
    component_bus_transceiver_oe: "Bus transceiver (OE)",
    component_input_port: "Input",
    component_output_port: "Output",
    component_inout_port: "Bidirectional",

    hint_rotate_delete_component:
        "R to rotate, Delete to remove the selected component, Esc to deselect",
    hint_delete_wire: "Delete removes the wire, double-click adds a point, right-click removes one",
    hint_selection: "{} selected — drag to move, Delete to remove, Ctrl+C to copy",
    hint_wiring:
        "Click to add a point, click a pin or wire to finish, Enter to leave the end loose, Esc to cancel",

    about_title: "About SimLogix",
    about_body: "SimLogix — a cross-platform logic simulator.",
    about_version: "Version {}",

    title_untitled: "Untitled",

    confirm_discard_title: "Unsaved changes",
    confirm_discard_body: "This circuit has changes that haven't been saved.",
    confirm_discard_save: "Save",
    confirm_discard_discard: "Discard",
    confirm_discard_cancel: "Cancel",

    status_paused: "Simulation paused",
    status_unstable: "Simulation paused: net {} keeps oscillating instead of settling",

    error_title: "Error",
    error_save_failed: "Couldn't save project: {}",
    error_open_failed: "Couldn't open project: {}",
    error_circuit_recursion: "\"{}\" can't contain itself, directly or through another circuit.",
};

static FRENCH: Strings = Strings {
    menu_file: "Fichier",
    menu_file_new: "Nouveau",
    menu_file_open: "Ouvrir un projet…",
    menu_file_save: "Enregistrer",
    menu_file_save_as: "Enregistrer sous…",
    menu_file_quit: "Quitter",
    menu_edit: "Édition",
    menu_edit_undo: "Annuler",
    menu_edit_redo: "Rétablir",
    menu_edit_copy: "Copier",
    menu_edit_paste: "Coller",
    menu_simulation: "Simulation",
    menu_simulation_run: "Démarrer",
    menu_simulation_pause: "Pause",
    menu_simulation_signals: "Afficher l'état des signaux",
    status_signals_hidden: "État des signaux masqué — C le réaffiche",
    menu_settings: "Paramètres",
    menu_settings_theme: "Thème",
    menu_settings_language: "Langue",
    settings_reset: "Réinitialiser",
    settings_reset_hint: "Le thème, la langue et le glisser gauche reviennent à leurs valeurs de départ.",
    menu_help: "?",
    menu_help_about: "À propos",
    menu_help_shortcuts: "Raccourcis et gestes",
    shortcuts_title: "Raccourcis et gestes",
    help_sections: &[
        HelpSection {
            title: "Fichiers",
            rows: &[
            ("Ctrl+N", "Nouveau projet"),
            ("Ctrl+O", "Ouvrir un projet"),
            ("Ctrl+S", "Enregistrer"),
            ("Ctrl+Shift+S", "Enregistrer sous"),
            ],
        },
        HelpSection {
            title: "Édition",
            rows: &[
            ("Ctrl+Z", "Annuler"),
            ("Ctrl+Shift+Z / Ctrl+Y", "Rétablir"),
            ("Ctrl+C", "Copier la sélection"),
            ("Ctrl+V", "Coller"),
            ("Suppr", "Supprimer la sélection"),
            ("R", "Faire pivoter les composants sélectionnés"),
            ],
        },
        HelpSection {
            title: "Canevas",
            rows: &[
            ("Molette", "Zoomer, au-dessus du canevas"),
            ("Glisser du milieu", "Déplacer la vue"),
            ("Glisser gauche", "Rectangle de sélection, ou la vue — voir Réglages"),
            ("Shift+clic", "Ajouter à la sélection ou en retirer"),
            ("Shift en posant", "Garder le composant en main pour le clic suivant"),
            ("Échap / clic droit", "Revenir d'un cran en arrière"),
            ],
        },
        HelpSection {
            title: "Fils",
            rows: &[
            ("Clic sur une broche", "Commencer un fil"),
            ("Clic", "Ajouter un coin au fil en cours"),
            ("Entrée", "Terminer le fil en laissant l'extrémité libre"),
            ("Double-clic sur un fil", "Y ajouter un point"),
            ("Clic droit sur un point", "Retirer ce point"),
            ("Clic droit sur un segment", "Couper le fil à cet endroit"),
            ("Glisser un bout libre sur un autre", "Raccorder les deux fils"),
            ],
        },
        HelpSection {
            title: "Simulation",
            rows: &[
            ("Espace", "Lancer ou mettre en pause la simulation"),
            ("C", "Afficher ou masquer l'état des signaux sur les fils"),
            ],
        },
    ],

    circuits_heading: "Circuits",
    project_library_hint: "Le nom que les autres projets utilisent pour désigner les circuits de celui-ci. Double-cliquez pour le changer.",
    circuit_new: "Nouveau circuit",
    circuit_new_here: "Nouveau circuit ici",
    circuit_move_to: "Déplacer vers",
    folder_new: "Nouveau dossier",
    folder_new_here: "Nouveau dossier ici",
    folder_delete: "Supprimer le dossier",
    folder_delete_hint: "Son contenu remonte dans le dossier parent.",
    folder_top_level: "Racine",
    folder_default_name: "dossier",
    circuit_place: "Poser dans le circuit ouvert",
    circuit_place_self: "Un circuit ne peut pas se contenir lui-même.",
    circuit_rename: "Renommer",
    circuit_delete: "Supprimer",
    circuit_delete_last: "Un projet doit conserver au moins un circuit.",
    circuit_name_taken: "« {} » est déjà utilisé ici.",
    circuit_in_use: "« {} » ne peut pas être supprimé : il est utilisé dans {}.",
    circuit_default_name: "circuit",

    properties_heading: "Propriétés",
    property_variant: "Type",
    property_name: "Nom",
    property_name_hint: "affiché sous le symbole",
    property_switch_on: "Fermé",
    property_switch_on_hint: "La position actuelle, et celle qui sera enregistrée — la basculer compte comme une modification.",
    property_pressed: "Enfoncé au repos",
    property_pressed_hint: "Le bouton est enfoncé au repos : cliquer le relâche au lieu de l'enfoncer.",
    property_color: "Couleur",
    property_tri_state: "Trois états",
    property_tri_state_hint: "Cliquer la broche peut aussi la laisser non pilotée, et l'interface admet cet état.",
    property_initial: "Valeur au repos",
    port_level_undriven: "Non pilotée",
    port_level_low: "Bas",
    port_level_high: "Haut",
    property_wire: "Fil",
    property_wire_color_hint: "S'applique à tout le net, en gaine autour de la couleur de signal.",
    property_reset: "Réinitialiser",
    properties_none_selected: "Sélectionnez un composant pour voir ses propriétés.",

    palette_heading: "Palette",
    palette_click_to_place:
        "Cliquez sur le canevas pour placer : {} — R le fait tourner, Maj en pose plusieurs",
    tool_select: "Sélection",
    tool_wire: "Tracer un fil",
    tool_marquee: "Rectangle de sélection",
    tool_pan: "Déplacer la vue",
    menu_settings_left_drag: "Glisser gauche",
    settings_left_drag_select: "Trace une sélection",
    settings_left_drag_pan: "Déplace la vue",
    category_interface: "Interface",
    category_sources: "Sources",
    category_outputs: "Sorties",
    category_transistors: "Transistors",
    category_gates: "Portes",
    category_memory: "Mémoire",
    category_buses: "Bus",

    component_button: "Bouton",
    component_switch: "Interrupteur",
    component_led: "LED",
    component_nmos: "NMOS",
    component_pmos: "PMOS",
    component_ground: "GND",
    component_power: "PWR",
    component_probe: "Sonde",
    component_clock: "Horloge",
    component_and: "ET",
    component_or: "OU",
    component_nand: "NON-ET",
    component_nor: "NON-OU",
    component_xor: "OU-EXCL",
    component_xnor: "NON-OU-EXCL",
    component_not: "NON",
    component_buffer: "Tampon",
    component_sr_latch: "Bascule SR",
    component_tri_state: "Tampon 3 états",
    component_bus_transceiver: "Transceiver (EN)",
    component_bus_transceiver_oe: "Transceiver (OE)",
    component_input_port: "Entrée",
    component_output_port: "Sortie",
    component_inout_port: "Bidirectionnelle",

    hint_rotate_delete_component:
        "R pour tourner, Suppr pour supprimer le composant sélectionné, Échap pour désélectionner",
    hint_delete_wire: "Suppr supprime le fil, double-clic ajoute un point, clic droit en retire un",
    hint_selection: "{} sélectionnés — glisser pour déplacer, Suppr pour retirer, Ctrl+C pour copier",
    hint_wiring:
        "Cliquez pour ajouter un point, une pin ou un fil pour finir, Entrée pour laisser le bout libre, Échap pour annuler",

    about_title: "À propos de SimLogix",
    about_body: "SimLogix — un simulateur logique multiplateforme.",
    about_version: "Version {}",

    title_untitled: "Sans titre",

    confirm_discard_title: "Modifications non enregistrées",
    confirm_discard_body: "Ce circuit contient des modifications qui n'ont pas été enregistrées.",
    confirm_discard_save: "Enregistrer",
    confirm_discard_discard: "Abandonner",
    confirm_discard_cancel: "Annuler",

    status_paused: "Simulation en pause",
    status_unstable: "Simulation en pause : le net {} oscille sans se stabiliser",

    error_title: "Erreur",
    error_save_failed: "Échec de l'enregistrement du projet : {}",
    error_open_failed: "Échec de l'ouverture du projet : {}",
    error_circuit_recursion: "« {} » ne peut pas se contenir lui-même, ni directement ni via un autre circuit.",
};

static ITALIAN: Strings = Strings {
    menu_file: "File",
    menu_file_new: "Nuovo",
    menu_file_open: "Apri progetto…",
    menu_file_save: "Salva",
    menu_file_save_as: "Salva con nome…",
    menu_file_quit: "Esci",
    menu_edit: "Modifica",
    menu_edit_undo: "Annulla",
    menu_edit_redo: "Ripeti",
    menu_edit_copy: "Copia",
    menu_edit_paste: "Incolla",
    menu_simulation: "Simulazione",
    menu_simulation_run: "Avvia",
    menu_simulation_pause: "Pausa",
    menu_simulation_signals: "Mostra lo stato dei segnali",
    status_signals_hidden: "Stato dei segnali nascosto — C lo rimostra",
    menu_settings: "Impostazioni",
    menu_settings_theme: "Tema",
    menu_settings_language: "Lingua",
    settings_reset: "Ripristina i valori predefiniti",
    settings_reset_hint: "Tema, lingua e trascinamento sinistro tornano ai valori iniziali.",
    menu_help: "?",
    menu_help_about: "Informazioni",
    menu_help_shortcuts: "Scorciatoie e gesti",
    shortcuts_title: "Scorciatoie e gesti",
    help_sections: &[
        HelpSection {
            title: "File",
            rows: &[
            ("Ctrl+N", "Nuovo progetto"),
            ("Ctrl+O", "Apri un progetto"),
            ("Ctrl+S", "Salva"),
            ("Ctrl+Shift+S", "Salva con nome"),
            ],
        },
        HelpSection {
            title: "Modifica",
            rows: &[
            ("Ctrl+Z", "Annulla"),
            ("Ctrl+Shift+Z / Ctrl+Y", "Ripeti"),
            ("Ctrl+C", "Copia la selezione"),
            ("Ctrl+V", "Incolla"),
            ("Canc", "Elimina la selezione"),
            ("R", "Ruota i componenti selezionati"),
            ],
        },
        HelpSection {
            title: "Area di lavoro",
            rows: &[
            ("Rotellina", "Zoom, sopra l'area di lavoro"),
            ("Trascinamento centrale", "Sposta la vista"),
            ("Trascinamento sinistro", "Rettangolo di selezione, o la vista — vedi Impostazioni"),
            ("Shift+clic", "Aggiungi alla selezione o togli"),
            ("Shift mentre posizioni", "Tieni il componente per il clic successivo"),
            ("Esc / clic destro", "Torna indietro di un passo"),
            ],
        },
        HelpSection {
            title: "Fili",
            rows: &[
            ("Clic su un pin", "Inizia un filo"),
            ("Clic", "Aggiungi un angolo al filo in corso"),
            ("Invio", "Termina il filo lasciando l'estremità libera"),
            ("Doppio clic su un filo", "Aggiungi un punto"),
            ("Clic destro su un punto", "Rimuovi quel punto"),
            ("Clic destro su un segmento", "Taglia il filo lì"),
            ("Trascina un'estremità libera su un'altra", "Unisci i due fili"),
            ],
        },
        HelpSection {
            title: "Simulazione",
            rows: &[
            ("Spazio", "Avvia o metti in pausa la simulazione"),
            ("C", "Mostra o nascondi lo stato dei segnali sui fili"),
            ],
        },
    ],

    circuits_heading: "Circuiti",
    project_library_hint: "Il nome che gli altri progetti usano per riferirsi ai circuiti di questo. Fai doppio clic per cambiarlo.",
    circuit_new: "Nuovo circuito",
    circuit_new_here: "Nuovo circuito qui",
    circuit_move_to: "Sposta in",
    folder_new: "Nuova cartella",
    folder_new_here: "Nuova cartella qui",
    folder_delete: "Elimina la cartella",
    folder_delete_hint: "Il contenuto risale alla cartella superiore.",
    folder_top_level: "Livello principale",
    folder_default_name: "cartella",
    circuit_place: "Inserisci nel circuito aperto",
    circuit_place_self: "Un circuito non può contenere se stesso.",
    circuit_rename: "Rinomina",
    circuit_delete: "Elimina",
    circuit_delete_last: "Un progetto deve conservare almeno un circuito.",
    circuit_name_taken: "\"{}\" è già in uso qui.",
    circuit_in_use: "\"{}\" non può essere eliminato: è usato in {}.",
    circuit_default_name: "circuito",

    properties_heading: "Proprietà",
    property_variant: "Tipo",
    property_name: "Nome",
    property_name_hint: "mostrato sotto il simbolo",
    property_switch_on: "Chiuso",
    property_switch_on_hint: "La posizione attuale, e quella che verrà salvata — cambiarla conta come una modifica.",
    property_pressed: "Premuto a riposo",
    property_pressed_hint: "Il pulsante è premuto a riposo: farci clic lo rilascia invece di premerlo.",
    property_color: "Colore",
    property_tri_state: "Tre stati",
    property_tri_state_hint: "Fare clic sulla porta può anche lasciarla non pilotata, e l'interfaccia ammette quello stato.",
    property_initial: "Valore a riposo",
    port_level_undriven: "Non pilotata",
    port_level_low: "Basso",
    port_level_high: "Alto",
    property_wire: "Filo",
    property_wire_color_hint: "Si applica all'intera rete, come guaina attorno al colore del segnale.",
    property_reset: "Reimposta",
    properties_none_selected: "Seleziona un componente per vederne le proprietà.",

    palette_heading: "Tavolozza",
    palette_click_to_place:
        "Clicca sulla tela per posizionare: {} — R lo ruota, Maiusc ne posa più",
    tool_select: "Selezione",
    tool_wire: "Traccia un filo",
    tool_marquee: "Rettangolo di selezione",
    tool_pan: "Sposta la vista",
    menu_settings_left_drag: "Trascinamento sinistro",
    settings_left_drag_select: "Traccia una selezione",
    settings_left_drag_pan: "Sposta la vista",
    category_interface: "Interfaccia",
    category_sources: "Sorgenti",
    category_outputs: "Uscite",
    category_transistors: "Transistor",
    category_gates: "Porte",
    category_memory: "Memoria",
    category_buses: "Bus",

    component_button: "Pulsante",
    component_switch: "Interruttore",
    component_led: "LED",
    component_nmos: "NMOS",
    component_pmos: "PMOS",
    component_ground: "GND",
    component_power: "PWR",
    component_probe: "Sonda",
    component_clock: "Orologio",
    component_and: "AND",
    component_or: "OR",
    component_nand: "NAND",
    component_nor: "NOR",
    component_xor: "XOR",
    component_xnor: "XNOR",
    component_not: "NOT",
    component_buffer: "Buffer",
    component_sr_latch: "Latch SR",
    component_tri_state: "Buffer tri-state",
    component_bus_transceiver: "Ricetrasmettitore (EN)",
    component_bus_transceiver_oe: "Ricetrasmettitore (OE)",
    component_input_port: "Ingresso",
    component_output_port: "Uscita",
    component_inout_port: "Bidirezionale",

    hint_rotate_delete_component:
        "R per ruotare, Canc per eliminare il componente selezionato, Esc per deselezionare",
    hint_delete_wire:
        "Canc elimina il filo, doppio clic aggiunge un punto, clic destro ne rimuove uno",
    hint_selection: "{} selezionati — trascina per spostare, Canc per rimuovere, Ctrl+C per copiare",
    hint_wiring:
        "Clicca per aggiungere un punto, un pin o un filo per finire, Invio per lasciare l'estremità libera, Esc per annullare",

    about_title: "Informazioni su SimLogix",
    about_body: "SimLogix — un simulatore logico multipiattaforma.",
    about_version: "Versione {}",

    title_untitled: "Senza titolo",

    confirm_discard_title: "Modifiche non salvate",
    confirm_discard_body: "Questo circuito contiene modifiche non salvate.",
    confirm_discard_save: "Salva",
    confirm_discard_discard: "Ignora",
    confirm_discard_cancel: "Annulla",

    status_paused: "Simulazione in pausa",
    status_unstable: "Simulazione in pausa: il net {} oscilla senza stabilizzarsi",

    error_title: "Errore",
    error_save_failed: "Impossibile salvare il progetto: {}",
    error_open_failed: "Impossibile aprire il progetto: {}",
    error_circuit_recursion: "\"{}\" non può contenere se stesso, né direttamente né tramite un altro circuito.",
};

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A row added to one language and forgotten in another is invisible
    /// until someone switches language, so the tables are held to the same
    /// *shape*.
    ///
    /// Deliberately not to the same text, not even in the key column: this
    /// test first asserted that and failed on `Suppr` vs `Delete` — which
    /// was the test being wrong, since that is exactly what a French
    /// keyboard has printed on it.
    #[test]
    fn every_language_lists_the_same_shortcuts() {
        for other in [&FRENCH, &ITALIAN] {
            assert_eq!(
                other.help_sections.len(),
                ENGLISH.help_sections.len(),
                "different number of sections"
            );
            for (reference, translated) in ENGLISH.help_sections.iter().zip(other.help_sections) {
                assert_eq!(
                    translated.rows.len(),
                    reference.rows.len(),
                    "section \"{}\" has a different number of rows",
                    reference.title
                );
            }
        }
    }

    #[test]
    fn no_shortcut_row_is_left_empty() {
        for strings in [&ENGLISH, &FRENCH, &ITALIAN] {
            for section in strings.help_sections {
                assert!(!section.title.is_empty());
                for (keys, what) in section.rows {
                    assert!(!keys.is_empty() && !what.is_empty(), "in {}", section.title);
                }
            }
        }
    }
}
