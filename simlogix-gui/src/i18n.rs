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

/// Which language the UI is currently displayed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub menu_simulation: &'static str,
    pub menu_simulation_run: &'static str,
    pub menu_simulation_pause: &'static str,
    pub menu_settings: &'static str,
    pub menu_settings_theme: &'static str,
    pub menu_settings_language: &'static str,
    pub menu_help: &'static str,
    pub menu_help_about: &'static str,

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
    pub circuit_rename: &'static str,
    pub circuit_delete: &'static str,
    /// Why deleting is refused on the last remaining circuit.
    pub circuit_delete_last: &'static str,
    /// Contains a literal `{}` for the name that was already taken. Used
    /// wherever a name has to be distinct — a circuit within its folder, a
    /// folder within its parent.
    pub circuit_name_taken: &'static str,
    /// Base name for a freshly created circuit — a number is appended when
    /// that name is already in use. Saved into the project file as typed,
    /// like any other name, so switching the UI language later doesn't
    /// rename anything.
    pub circuit_default_name: &'static str,

    pub properties_heading: &'static str,
    pub property_variant: &'static str,
    pub property_name: &'static str,
    pub property_name_hint: &'static str,
    pub property_pressed: &'static str,
    pub property_pressed_hint: &'static str,
    pub property_color: &'static str,
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
    pub category_sources: &'static str,
    pub category_outputs: &'static str,
    pub category_transistors: &'static str,
    pub category_gates: &'static str,
    pub category_memory: &'static str,
    pub category_buses: &'static str,

    pub component_button: &'static str,
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

    pub hint_rotate_delete_component: &'static str,
    pub hint_delete_wire: &'static str,
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
    pub fn component_kind_label(&self, kind: ComponentKind) -> &'static str {
        match kind {
            ComponentKind::Button => self.component_button,
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
            ComponentKind::TriStateBuffer => self.component_tri_state,
            ComponentKind::BusTransceiver => self.component_bus_transceiver,
            ComponentKind::BusTransceiverOe => self.component_bus_transceiver_oe,
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
    menu_simulation: "Simulation",
    menu_simulation_run: "Run",
    menu_simulation_pause: "Pause",
    menu_settings: "Settings",
    menu_settings_theme: "Theme",
    menu_settings_language: "Language",
    menu_help: "?",
    menu_help_about: "About",

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
    circuit_rename: "Rename",
    circuit_delete: "Delete",
    circuit_delete_last: "A project has to keep at least one circuit.",
    circuit_name_taken: "\"{}\" is already taken here.",
    circuit_default_name: "circuit",

    properties_heading: "Properties",
    property_variant: "Type",
    property_name: "Name",
    property_name_hint: "shown under the symbol",
    property_pressed: "Pressed at rest",
    property_pressed_hint: "The button rests pressed, so clicking it releases it instead.",
    property_color: "Colour",
    property_wire: "Wire",
    property_wire_color_hint: "Applies to the whole net, as a casing around the signal colour.",
    property_reset: "Reset",
    properties_none_selected: "Select a component to see its properties.",

    palette_heading: "Palette",
    palette_click_to_place:
        "Click the canvas to place a {} — hold Shift to place several",
    tool_select: "Select",
    tool_wire: "Draw wire",
    category_sources: "Sources",
    category_outputs: "Outputs",
    category_transistors: "Transistors",
    category_gates: "Gates",
    category_memory: "Memory",
    category_buses: "Buses",

    component_button: "Button",
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

    hint_rotate_delete_component:
        "R to rotate, Delete to remove the selected component, Esc to deselect",
    hint_delete_wire: "Delete removes the wire, double-click adds a point, right-click removes one",
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
    menu_simulation: "Simulation",
    menu_simulation_run: "Démarrer",
    menu_simulation_pause: "Pause",
    menu_settings: "Paramètres",
    menu_settings_theme: "Thème",
    menu_settings_language: "Langue",
    menu_help: "?",
    menu_help_about: "À propos",

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
    circuit_rename: "Renommer",
    circuit_delete: "Supprimer",
    circuit_delete_last: "Un projet doit conserver au moins un circuit.",
    circuit_name_taken: "« {} » est déjà utilisé ici.",
    circuit_default_name: "circuit",

    properties_heading: "Propriétés",
    property_variant: "Type",
    property_name: "Nom",
    property_name_hint: "affiché sous le symbole",
    property_pressed: "Enfoncé au repos",
    property_pressed_hint: "Le bouton est enfoncé au repos : cliquer le relâche au lieu de l'enfoncer.",
    property_color: "Couleur",
    property_wire: "Fil",
    property_wire_color_hint: "S'applique à tout le net, en gaine autour de la couleur de signal.",
    property_reset: "Réinitialiser",
    properties_none_selected: "Sélectionnez un composant pour voir ses propriétés.",

    palette_heading: "Palette",
    palette_click_to_place:
        "Cliquez sur le canevas pour placer : {} — maintenez Maj pour en poser plusieurs",
    tool_select: "Sélection",
    tool_wire: "Tracer un fil",
    category_sources: "Sources",
    category_outputs: "Sorties",
    category_transistors: "Transistors",
    category_gates: "Portes",
    category_memory: "Mémoire",
    category_buses: "Bus",

    component_button: "Bouton",
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

    hint_rotate_delete_component:
        "R pour tourner, Suppr pour supprimer le composant sélectionné, Échap pour désélectionner",
    hint_delete_wire: "Suppr supprime le fil, double-clic ajoute un point, clic droit en retire un",
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
    menu_simulation: "Simulazione",
    menu_simulation_run: "Avvia",
    menu_simulation_pause: "Pausa",
    menu_settings: "Impostazioni",
    menu_settings_theme: "Tema",
    menu_settings_language: "Lingua",
    menu_help: "?",
    menu_help_about: "Informazioni",

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
    circuit_rename: "Rinomina",
    circuit_delete: "Elimina",
    circuit_delete_last: "Un progetto deve conservare almeno un circuito.",
    circuit_name_taken: "\"{}\" è già in uso qui.",
    circuit_default_name: "circuito",

    properties_heading: "Proprietà",
    property_variant: "Tipo",
    property_name: "Nome",
    property_name_hint: "mostrato sotto il simbolo",
    property_pressed: "Premuto a riposo",
    property_pressed_hint: "Il pulsante è premuto a riposo: farci clic lo rilascia invece di premerlo.",
    property_color: "Colore",
    property_wire: "Filo",
    property_wire_color_hint: "Si applica all'intera rete, come guaina attorno al colore del segnale.",
    property_reset: "Reimposta",
    properties_none_selected: "Seleziona un componente per vederne le proprietà.",

    palette_heading: "Tavolozza",
    palette_click_to_place:
        "Clicca sulla tela per posizionare: {} — tieni premuto Maiusc per posarne più",
    tool_select: "Selezione",
    tool_wire: "Traccia un filo",
    category_sources: "Sorgenti",
    category_outputs: "Uscite",
    category_transistors: "Transistor",
    category_gates: "Porte",
    category_memory: "Memoria",
    category_buses: "Bus",

    component_button: "Pulsante",
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

    hint_rotate_delete_component:
        "R per ruotare, Canc per eliminare il componente selezionato, Esc per deselezionare",
    hint_delete_wire:
        "Canc elimina il filo, doppio clic aggiunge un punto, clic destro ne rimuove uno",
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
};
