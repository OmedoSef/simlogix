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
    pub menu_file_quit: &'static str,
    pub menu_settings: &'static str,
    pub menu_settings_theme: &'static str,
    pub menu_settings_language: &'static str,
    pub menu_help: &'static str,
    pub menu_help_about: &'static str,

    pub palette_heading: &'static str,
    /// Contains a literal `{}` for the component name — fill it in with
    /// `.replace("{}", name)`, not `format!` (the template isn't a
    /// compile-time literal).
    pub palette_click_to_place: &'static str,
    pub category_sources: &'static str,
    pub category_outputs: &'static str,
    pub category_transistors: &'static str,
    pub category_gates: &'static str,

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

    pub hint_rotate_delete_component: &'static str,
    pub hint_delete_wire: &'static str,

    pub about_title: &'static str,
    pub about_body: &'static str,
    /// Contains a literal `{}` for the version number — see
    /// `palette_click_to_place` on how to fill it in.
    pub about_version: &'static str,

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
        }
    }
}

static ENGLISH: Strings = Strings {
    menu_file: "File",
    menu_file_new: "New",
    menu_file_open: "Open Project…",
    menu_file_save: "Save Project…",
    menu_file_quit: "Quit",
    menu_settings: "Settings",
    menu_settings_theme: "Theme",
    menu_settings_language: "Language",
    menu_help: "?",
    menu_help_about: "About",

    palette_heading: "Palette",
    palette_click_to_place: "Click the canvas to place a {}",
    category_sources: "Sources",
    category_outputs: "Outputs",
    category_transistors: "Transistors",
    category_gates: "Gates",

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

    hint_rotate_delete_component: "Press R to rotate, Delete to remove the selected component",
    hint_delete_wire: "Press Delete to remove the selected wire",

    about_title: "About SimLogix",
    about_body: "SimLogix — a cross-platform logic simulator.",
    about_version: "Version {}",

    error_title: "Error",
    error_save_failed: "Couldn't save project: {}",
    error_open_failed: "Couldn't open project: {}",
};

static FRENCH: Strings = Strings {
    menu_file: "Fichier",
    menu_file_new: "Nouveau",
    menu_file_open: "Ouvrir un projet…",
    menu_file_save: "Enregistrer le projet…",
    menu_file_quit: "Quitter",
    menu_settings: "Paramètres",
    menu_settings_theme: "Thème",
    menu_settings_language: "Langue",
    menu_help: "?",
    menu_help_about: "À propos",

    palette_heading: "Palette",
    palette_click_to_place: "Cliquez sur le canevas pour placer : {}",
    category_sources: "Sources",
    category_outputs: "Sorties",
    category_transistors: "Transistors",
    category_gates: "Portes",

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

    hint_rotate_delete_component: "R pour tourner, Suppr pour supprimer le composant sélectionné",
    hint_delete_wire: "Suppr pour supprimer le fil sélectionné",

    about_title: "À propos de SimLogix",
    about_body: "SimLogix — un simulateur logique multiplateforme.",
    about_version: "Version {}",

    error_title: "Erreur",
    error_save_failed: "Échec de l'enregistrement du projet : {}",
    error_open_failed: "Échec de l'ouverture du projet : {}",
};

static ITALIAN: Strings = Strings {
    menu_file: "File",
    menu_file_new: "Nuovo",
    menu_file_open: "Apri progetto…",
    menu_file_save: "Salva progetto…",
    menu_file_quit: "Esci",
    menu_settings: "Impostazioni",
    menu_settings_theme: "Tema",
    menu_settings_language: "Lingua",
    menu_help: "?",
    menu_help_about: "Informazioni",

    palette_heading: "Tavolozza",
    palette_click_to_place: "Clicca sulla tela per posizionare: {}",
    category_sources: "Sorgenti",
    category_outputs: "Uscite",
    category_transistors: "Transistor",
    category_gates: "Porte",

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

    hint_rotate_delete_component: "R per ruotare, Canc per eliminare il componente selezionato",
    hint_delete_wire: "Canc per eliminare il filo selezionato",

    about_title: "Informazioni su SimLogix",
    about_body: "SimLogix — un simulatore logico multipiattaforma.",
    about_version: "Versione {}",

    error_title: "Errore",
    error_save_failed: "Impossibile salvare il progetto: {}",
    error_open_failed: "Impossibile aprire il progetto: {}",
};
