//! The on-disk project format: what gets saved/loaded.
//!
//! Structural only — which components, where, how their pins are wired
//! together — never runtime state (a button's pressed state, a net's current
//! signal). Loading a project starts it cold, like opening a fresh Logisim
//! circuit.
//!
//! A project can eventually hold several circuits (for a sub-circuit
//! hierarchy, not built yet) — the format already supports that, even though
//! today's editor only ever produces/reads one, named `"main"`.

use serde::{Deserialize, Serialize};

use crate::canvas::Rotation;
use crate::palette::ComponentKind;

/// Bump this whenever `SavedProject`'s shape changes, and write a migration
/// from the previous version rather than silently breaking old files.
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedProject {
    pub version: u32,
    pub circuits: Vec<SavedCircuit>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedCircuit {
    pub name: String,
    pub components: Vec<SavedComponent>,
    /// Each inner list is a group of `(component index, pin index)` pairs —
    /// every pin named in one group shares a single net.
    pub wires: Vec<Vec<(usize, usize)>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedComponent {
    pub kind: ComponentKind,
    pub x: f32,
    pub y: f32,
    pub rotation: Rotation,
}
