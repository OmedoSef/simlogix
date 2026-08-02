//! The left palette panel: pick a component kind to place next.

use egui::Ui;
use serde::{Deserialize, Serialize};

/// Which kind of component the palette currently has queued for placement.
/// Also the tag saved in a project file (see `project.rs`) to say which
/// concrete component a saved entry should become on load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    Button,
    Led,
    NTransistor,
    PTransistor,
    Ground,
    Power,
    Probe,
    Clock,
}

impl ComponentKind {
    pub fn label(self) -> &'static str {
        match self {
            ComponentKind::Button => "Button",
            ComponentKind::Led => "LED",
            ComponentKind::NTransistor => "NMOS",
            ComponentKind::PTransistor => "PMOS",
            ComponentKind::Ground => "GND",
            ComponentKind::Power => "PWR",
            ComponentKind::Probe => "Probe",
            ComponentKind::Clock => "Clock",
        }
    }
}

/// Draws the palette. Returns `Some(kind)` the frame a palette entry is
/// clicked, requesting that kind be queued for placement. `pending` is shown
/// back as a hint so the user knows what's queued.
pub fn show(ui: &mut Ui, pending: Option<ComponentKind>) -> Option<ComponentKind> {
    ui.heading("Palette");
    ui.add_space(8.0);

    let mut clicked = None;
    if ui.button("Button").clicked() {
        clicked = Some(ComponentKind::Button);
    }
    if ui.button("LED").clicked() {
        clicked = Some(ComponentKind::Led);
    }
    if ui.button("NMOS").clicked() {
        clicked = Some(ComponentKind::NTransistor);
    }
    if ui.button("PMOS").clicked() {
        clicked = Some(ComponentKind::PTransistor);
    }
    if ui.button("GND").clicked() {
        clicked = Some(ComponentKind::Ground);
    }
    if ui.button("PWR").clicked() {
        clicked = Some(ComponentKind::Power);
    }
    if ui.button("Probe").clicked() {
        clicked = Some(ComponentKind::Probe);
    }
    if ui.button("Clock").clicked() {
        clicked = Some(ComponentKind::Clock);
    }

    ui.add_space(8.0);
    if let Some(kind) = pending {
        ui.label(format!("Click the canvas to place a {}", kind.label()));
    }

    clicked
}
