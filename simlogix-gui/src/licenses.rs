//! The licence window: SimLogix's own terms, and those of everything it is
//! built on.
//!
//! # Why the text is embedded rather than read from disk
//!
//! The obligation these licences carry is attribution, and attribution has to
//! reach whoever ends up with a copy. A file sitting next to the binary is
//! one that can be separated from it; text compiled *into* the binary cannot
//! be. `THIRD-PARTY.md` is checked in as well, so the same list is readable
//! without running anything.
//!
//! # Why the data is JSON and not that Markdown
//!
//! Both come from the same run of the `write-licenses` tool. The Markdown is
//! for the repository — readable on a forge, greppable, diffable. Showing it
//! *here* would mean putting `| crate | 1.2 | MIT |` and `[name](url)` on
//! screen as literal text, which is a marked-up file being displayed rather
//! than a list being presented. The window reads the structured form and
//! draws a real table.

use std::sync::OnceLock;

use egui::{Context, Ui};
use serde::Deserialize;

use crate::i18n::Strings;

/// SimLogix's own terms — the same file the repository ships, so the two
/// cannot come to disagree.
const OWN: &str = include_str!("../../LICENSE");

/// Generated alongside `THIRD-PARTY.md` by `write-licenses`.
const THIRD_PARTY: &str = include_str!("../../assets/third-party.json");

#[derive(Debug, Default, Deserialize)]
struct Notice {
    crates: Vec<Listed>,
    /// Distinct licence texts, each with the crates that ship it.
    groups: Vec<Group>,
    /// Crates declaring terms in their manifest but shipping no copy.
    undocumented: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Listed {
    name: String,
    version: String,
    license: String,
    repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Group {
    users: Vec<String>,
    text: String,
}

/// Parsed once, on the first frame the window is opened — not at start-up,
/// and not on every frame. Most sessions never open it at all.
fn notice() -> &'static Notice {
    static PARSED: OnceLock<Notice> = OnceLock::new();
    PARSED.get_or_init(|| serde_json::from_str(THIRD_PARTY).unwrap_or_default())
}

/// Which half of the window is showing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Own,
    ThirdParty,
}

/// What the window remembers between frames.
#[derive(Debug, Default)]
pub struct State {
    pub open: bool,
    pub tab: Tab,
    /// Narrows the dependency list. With 400-odd entries, finding one by
    /// scrolling is not finding it.
    filter: String,
}

/// Draws the window while it is open, and lets its close button shut it.
pub fn show(ctx: &Context, strings: &Strings, state: &mut State) {
    let mut open = state.open;
    egui::Window::new(strings.licenses_title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(440.0)
        .show(ctx, |ui| body(ui, strings, state));
    state.open = open;
}

fn body(ui: &mut Ui, strings: &Strings, state: &mut State) {
    ui.horizontal(|ui| {
        for (option, label) in [
            (Tab::Own, strings.licenses_own),
            (Tab::ThirdParty, strings.licenses_third_party),
        ] {
            if ui.selectable_label(state.tab == option, label).clicked() {
                state.tab = option;
            }
        }
    });
    ui.separator();

    match state.tab {
        Tab::Own => own(ui),
        Tab::ThirdParty => third_party(ui, strings, state),
    }
}

fn own(ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Monospace and line by line: a licence text's own line breaks
            // are the layout it was written with, and reflowing it would be
            // reformatting a legal notice.
            for line in OWN.lines() {
                ui.label(egui::RichText::new(line).monospace());
            }
        });
}

fn third_party(ui: &mut Ui, strings: &Strings, state: &mut State) {
    let notice = notice();

    ui.label(strings.licenses_third_party_intro);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(strings.licenses_filter);
        ui.text_edit_singleline(&mut state.filter);
        if !state.filter.is_empty() && ui.button("✕").clicked() {
            state.filter.clear();
        }
    });

    let needle = state.filter.trim().to_lowercase();
    let matches = |haystack: &str| needle.is_empty() || haystack.to_lowercase().contains(&needle);
    let shown: Vec<&Listed> = notice
        .crates
        .iter()
        .filter(|entry| matches(&entry.name) || matches(&entry.license))
        .collect();

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            strings
                .licenses_count
                .replace("{}", &shown.len().to_string())
                .replace("{total}", &notice.crates.len().to_string()),
        )
        .weak(),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("third_party_crates")
                .num_columns(3)
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for entry in &shown {
                        match &entry.repository {
                            Some(url) => ui.hyperlink_to(&entry.name, url),
                            None => ui.label(&entry.name),
                        };
                        ui.label(egui::RichText::new(&entry.version).weak());
                        ui.label(&entry.license);
                        ui.end_row();
                    }
                });

            if shown.len() == notice.crates.len() {
                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new(strings.licenses_texts).strong());
                ui.label(egui::RichText::new(strings.licenses_texts_intro).weak());
                ui.add_space(4.0);

                // Collapsed by default, and a closed header draws none of its
                // body — which is what keeps 700 KiB of licence text off
                // every frame without any laziness of our own.
                for (index, group) in notice.groups.iter().enumerate() {
                    let heading = match group.users.split_first() {
                        Some((first, [])) => first.clone(),
                        Some((first, rest)) => format!("{first} + {}", rest.len()),
                        None => String::new(),
                    };
                    egui::CollapsingHeader::new(heading)
                        .id_salt(("licence_text", index))
                        .show(ui, |ui| {
                            for who in &group.users {
                                ui.label(egui::RichText::new(who).weak().small());
                            }
                            ui.add_space(4.0);
                            for line in group.text.lines() {
                                ui.label(egui::RichText::new(line).monospace());
                            }
                        });
                }

                if !notice.undocumented.is_empty() {
                    ui.add_space(10.0);
                    egui::CollapsingHeader::new(strings.licenses_no_file)
                        .id_salt("licence_no_file")
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(strings.licenses_no_file_intro).weak());
                            ui.add_space(4.0);
                            for who in &notice.undocumented {
                                ui.label(who);
                            }
                        });
                }
            }
        });
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_licence_is_the_one_the_repository_ships() {
        // `include_str!` makes this true by construction; the test is here so
        // that emptying or renaming the file fails loudly rather than
        // shipping a window with nothing in it.
        assert!(OWN.contains("MIT License"));
        assert!(OWN.contains("Romain VOLPI"));
        assert!(OWN.contains("without restriction"));
    }

    #[test]
    fn the_generated_notice_parses_and_is_not_empty() {
        // `unwrap_or_default` in `notice()` means a broken file would show an
        // empty window rather than crash — which is the right behaviour and
        // exactly why it needs a test to say it hasn't happened.
        let notice = notice();
        assert!(notice.crates.len() > 100, "the dependency list is missing");
        assert!(!notice.groups.is_empty(), "no licence texts were collected");
    }

    #[test]
    fn every_dependency_declares_a_licence() {
        // The generator writes this marker rather than guessing. One
        // appearing means a dependency arrived with no terms attached, which
        // is a decision to make and not a line to scroll past.
        let offenders: Vec<&str> = notice()
            .crates
            .iter()
            .filter(|entry| entry.license == "NOT DECLARED")
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(offenders.is_empty(), "no declared licence: {offenders:?}");
    }

    #[test]
    fn every_licence_text_says_which_crates_it_covers() {
        // A text with no crates against it is an attribution attached to
        // nothing, which would be worse than not listing it.
        for group in &notice().groups {
            assert!(!group.users.is_empty());
            assert!(!group.text.trim().is_empty());
        }
    }
}
