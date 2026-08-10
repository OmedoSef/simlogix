//! The menu bar: File, Simulation, Edit, Settings and the help menu.
//!
//! Split out of `app.rs` as one subject. Nothing here draws the circuit; it
//! is the shelf of commands above it, and it grew large enough to be reading
//! material of its own.
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use crate::i18n::{Language, Strings};

use super::{PendingAction, SimLogixApp};

/// The chords, defined once.
///
/// They are needed in two places — consumed as key presses before any widget
/// sees them, and written beside the menu entries they belong to — and two
/// copies of a chord is one that eventually says `Ctrl+S` while doing
/// something else.
pub(super) struct Shortcuts {
    pub new: egui::KeyboardShortcut,
    pub open: egui::KeyboardShortcut,
    pub save: egui::KeyboardShortcut,
    pub save_as: egui::KeyboardShortcut,
    pub undo: egui::KeyboardShortcut,
    pub redo: egui::KeyboardShortcut,
    /// Both conventions, since which one means "redo" depends entirely on
    /// what the user came from.
    pub redo_alt: egui::KeyboardShortcut,
    /// Shown in the menu only: egui turns these two into `Event::Copy` and
    /// `Event::Paste` and never emits the key press, so they are labels
    /// rather than something to consume.
    pub copy: egui::KeyboardShortcut,
    pub paste: egui::KeyboardShortcut,
    /// A function key rather than a letter or punctuation: `F10` is printed
    /// the same on every layout, and it is what a debugger has meant by
    /// "step" for thirty years. `.` would have needed Shift on an AZERTY.
    pub step: egui::KeyboardShortcut,
    /// Shift alongside it, the way *step over* and *step into* sit together
    /// in a debugger: the same gesture, one going further.
    pub step_event: egui::KeyboardShortcut,
}

impl Shortcuts {
    pub(super) fn new() -> Self {
        let command = |key| egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
        let shifted = |key| {
            egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, key)
        };
        Self {
            new: command(egui::Key::N),
            open: command(egui::Key::O),
            save: command(egui::Key::S),
            save_as: shifted(egui::Key::S),
            undo: command(egui::Key::Z),
            redo: shifted(egui::Key::Z),
            redo_alt: command(egui::Key::Y),
            copy: command(egui::Key::C),
            paste: command(egui::Key::V),
            step: egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F10),
            step_event: egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::F10),
        }
    }
}

impl SimLogixApp {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui, strings: &Strings, keys: &Shortcuts) {
        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(strings.menu_file, |ui| {
                    let shortcut =
                        |ui: &egui::Ui, s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_new)
                                .shortcut_text(shortcut(ui, &keys.new)),
                        )
                        .clicked()
                    {
                        self.request_action(PendingAction::New, ui.ctx());
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_open)
                                .shortcut_text(shortcut(ui, &keys.open)),
                        )
                        .clicked()
                    {
                        self.request_action(PendingAction::Open, ui.ctx());
                        ui.close();
                    }

                    // Disabled rather than hidden when empty: a menu whose
                    // entries come and go is one you have to hunt through.
                    ui.add_enabled_ui(!self.recent.is_empty(), |ui| {
                        ui.menu_button(strings.menu_file_recent, |ui| {
                            let mut chosen = None;
                            for path in &self.recent {
                                // The file name is what identifies a project
                                // at a glance; the full path goes in the
                                // tooltip, for the days two of them share a
                                // name.
                                let label = path
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                                if ui
                                    .button(label)
                                    .on_hover_text(path.to_string_lossy())
                                    .clicked()
                                {
                                    chosen = Some(path.clone());
                                }
                            }
                            if let Some(path) = chosen {
                                self.request_action(PendingAction::OpenRecent(path), ui.ctx());
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(strings.menu_file_recent_clear).clicked() {
                                self.recent.clear();
                                ui.close();
                            }
                        });
                    });

                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_save)
                                .shortcut_text(shortcut(ui, &keys.save)),
                        )
                        .clicked()
                    {
                        self.save_project();
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_save_as)
                                .shortcut_text(shortcut(ui, &keys.save_as)),
                        )
                        .clicked()
                    {
                        self.save_project_as();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(strings.menu_file_quit).clicked() {
                        self.request_action(PendingAction::Quit, ui.ctx());
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_simulation, |ui| {
                    let label = if self.running {
                        strings.menu_simulation_pause
                    } else {
                        strings.menu_simulation_run
                    };
                    if ui
                        .add(
                            egui::Button::new(label).shortcut_text(ui.ctx().format_shortcut(
                                &egui::KeyboardShortcut::new(
                                    egui::Modifiers::NONE,
                                    egui::Key::Space,
                                ),
                            )),
                        )
                        .clicked()
                    {
                        self.toggle_running();
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_simulation_step)
                                .shortcut_text(ui.ctx().format_shortcut(&keys.step)),
                        )
                        .clicked()
                    {
                        self.step(1);
                        ui.close();
                    }
                    let has_event = self.circuit.next_event_tick().is_some();
                    if ui
                        .add_enabled(
                            has_event,
                            egui::Button::new(strings.menu_simulation_step_event)
                                .shortcut_text(ui.ctx().format_shortcut(&keys.step_event)),
                        )
                        .clicked()
                    {
                        self.step_to_next_event();
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button(strings.menu_simulation_speed, |ui| {
                        for speed in super::SPEEDS {
                            if ui
                                .radio(self.speed == speed, super::speed_label(speed))
                                .clicked()
                            {
                                self.speed = speed;
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    // In this menu rather than in Settings: what the wires
                    // show *is* the simulation's output, and this is
                    // something you flip while working — which is why it has
                    // a key of its own — not something you set once like a
                    // theme. It isn't remembered between runs, for the same
                    // reason pause isn't.
                    let mut show_state = self.show_signal_state;
                    // A `Checkbox` carries no shortcut column, so the key
                    // goes in the label — the tick is worth more here than
                    // the alignment would be.
                    let signals_label = format!(
                        "{}  ({})",
                        strings.menu_simulation_signals,
                        ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
                            egui::Modifiers::NONE,
                            egui::Key::C,
                        ))
                    );
                    if ui
                        .add(egui::Checkbox::new(&mut show_state, signals_label))
                        .changed()
                    {
                        self.show_signal_state = show_state;
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_edit, |ui| {
                    let shortcut =
                        |ui: &egui::Ui, s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
                    // Greyed out rather than hidden when there's nothing to
                    // step to, so the menu also answers "is there anything
                    // to undo?".
                    if ui
                        .add_enabled(
                            !self.undo_stack.is_empty(),
                            egui::Button::new(strings.menu_edit_undo)
                                .shortcut_text(shortcut(ui, &keys.undo)),
                        )
                        .clicked()
                    {
                        self.undo();
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.redo_stack.is_empty(),
                            egui::Button::new(strings.menu_edit_redo)
                                .shortcut_text(shortcut(ui, &keys.redo)),
                        )
                        .clicked()
                    {
                        self.redo();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            !self.selection.is_empty(),
                            egui::Button::new(strings.menu_edit_copy)
                                .shortcut_text(shortcut(ui, &keys.copy)),
                        )
                        .clicked()
                    {
                        self.copy_to_clipboard(ui.ctx());
                        ui.close();
                    }
                    // Pastes what *this* window last copied, because that's
                    // all a menu item can reach: egui only ever hands over
                    // the system clipboard through the `Ctrl+V` event, so
                    // there is no way to read it on demand from here.
                    if ui
                        .add_enabled(
                            self.clipboard.is_some(),
                            egui::Button::new(strings.menu_edit_paste)
                                .shortcut_text(shortcut(ui, &keys.paste)),
                        )
                        .clicked()
                    {
                        if let Some(fragment) = self.clipboard.clone() {
                            self.paste_fragment(&fragment);
                        }
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_settings, |ui| {
                    ui.label(strings.menu_settings_left_drag);
                    for (pans, label) in [
                        (false, strings.settings_left_drag_select),
                        (true, strings.settings_left_drag_pan),
                    ] {
                        if ui.radio(self.left_drag_pans == pans, label).clicked() {
                            self.left_drag_pans = pans;
                        }
                    }
                    ui.separator();
                    ui.label(strings.menu_settings_theme);
                    // egui already defaults to ThemePreference::System (follows
                    // the OS) and tracks the current choice itself -- read it,
                    // let the built-in widget mutate the local copy, write it
                    // back. No SimLogixApp-level state needed for this.
                    let mut theme_preference = ui.ctx().options(|opt| opt.theme_preference);
                    theme_preference.radio_buttons(ui);
                    ui.ctx().set_theme(theme_preference);

                    ui.separator();

                    ui.label(strings.menu_settings_language);
                    ui.horizontal(|ui| {
                        for language in [Language::English, Language::French, Language::Italian] {
                            if ui
                                .selectable_value(&mut self.language, language, language.label())
                                .clicked()
                            {
                                // From here on it's a choice, not a guess at
                                // the OS locale, so it's worth remembering.
                                self.language_chosen = true;
                            }
                        }
                    });

                    ui.separator();
                    if ui
                        .button(strings.settings_reset)
                        .on_hover_text(strings.settings_reset_hint)
                        .clicked()
                    {
                        self.reset_settings(ui.ctx());
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_help, |ui| {
                    if ui.button(strings.menu_help_shortcuts).clicked() {
                        self.show_shortcuts = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(strings.menu_help_licenses).clicked() {
                        self.licenses.open = true;
                        ui.close();
                    }
                    if ui.button(strings.menu_help_about).clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                });
            });
        });
    }
}
