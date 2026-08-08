//! The on-disk project format: what gets saved/loaded.
//!
//! Structural only — which components, where, how their pins are wired
//! together, and how each wire is routed — never runtime state (a button's
//! pressed state, a net's current signal). Loading a project starts it cold,
//! like opening a fresh Logisim circuit.
//!
//! A project can eventually hold several circuits (for a sub-circuit
//! hierarchy, not built yet) — the format already supports that, even though
//! today's editor only ever produces/reads one, named `"main"`.
//!
//! This is also what an undo step is made of (see `SimLogixApp::record_edit`):
//! a snapshot is just a `SavedProject`, so undo and save/load share one
//! definition of "everything that makes up a circuit" instead of drifting.

use serde::{Deserialize, Serialize};

use crate::canvas::Rotation;
use crate::palette::ComponentKind;

/// Bump this whenever `SavedProject`'s shape changes, and write a migration
/// from the previous version rather than silently breaking old files.
///
/// - `1` — wires were groups of pins sharing a net, with no shape.
/// - `2` — wires are explicit, each with its own route and optional
///   junction tap (matching the editor's own `Wire`, added when routing
///   became user-controlled).
/// - `3` — a wire's *start* is an endpoint too, not necessarily a pin, so a
///   wire can survive its component being deleted and can begin at a loose
///   point (which is what splitting a wire produces).
pub const CURRENT_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProject {
    pub version: u32,
    pub circuits: Vec<SavedCircuit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCircuit {
    pub name: String,
    pub components: Vec<SavedComponent>,
    pub wires: Vec<SavedWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedComponent {
    pub kind: ComponentKind,
    pub x: f32,
    pub y: f32,
    pub rotation: Rotation,
}

/// One drawn wire: two endpoints and the route between them. Positions are
/// stored as plain `(x, y)` pairs rather than an egui type, to keep the file
/// format independent of the GUI toolkit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedWire {
    pub from: SavedEndpoint,
    pub to: SavedEndpoint,
    pub waypoints: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SavedEndpoint {
    /// `(component index, pin index)`.
    Pin(usize, usize),
    /// A tap onto another wire, by index into this circuit's `wires` and
    /// which of that wire's waypoints.
    Junction { wire: usize, waypoint: usize },
    /// A loose `(x, y)` end, left behind where a junction used to be after
    /// its host was deleted.
    Free(f32, f32),
}

impl SavedProject {
    /// Parses a project file, migrating older versions forward. The version
    /// is read before anything else, since it's what decides how the rest of
    /// the document should even be interpreted.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|err| err.to_string())?;
        let version = value
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "missing format version".to_string())?;

        match version {
            1 => serde_json::from_value::<v1::SavedProject>(value)
                .map_err(|err| err.to_string())
                .map(Self::from_v1)
                .map(Self::from_v2),
            2 => serde_json::from_value::<v2::SavedProject>(value)
                .map_err(|err| err.to_string())
                .map(Self::from_v2),
            3 => serde_json::from_value(value).map_err(|err| err.to_string()),
            other => Err(format!("unsupported project format version {other}")),
        }
    }

    /// Rebuilds v1's pin groups as explicit wires: a star from each group's
    /// first pin out to every other, unrouted. That's exactly what v1 drew,
    /// so an old file still opens looking like it did.
    fn from_v1(old: v1::SavedProject) -> v2::SavedProject {
        let circuits = old
            .circuits
            .into_iter()
            .map(|circuit| v2::SavedCircuit {
                name: circuit.name,
                components: circuit.components,
                wires: circuit
                    .wires
                    .into_iter()
                    .flat_map(|group| {
                        let mut group = group.into_iter();
                        let anchor = group.next();
                        group.filter_map(move |(ci, pi)| {
                            anchor.map(|from| v2::SavedWire {
                                from,
                                to: SavedEndpoint::Pin(ci, pi),
                                waypoints: Vec::new(),
                            })
                        })
                    })
                    .collect(),
            })
            .collect();

        v2::SavedProject { circuits }
    }

    /// v2 wires always started at a pin; v3 lets either end be any endpoint,
    /// so an old start becomes an explicit `Pin`.
    fn from_v2(old: v2::SavedProject) -> Self {
        let circuits = old
            .circuits
            .into_iter()
            .map(|circuit| SavedCircuit {
                name: circuit.name,
                components: circuit.components,
                wires: circuit
                    .wires
                    .into_iter()
                    .map(|wire| SavedWire {
                        from: SavedEndpoint::Pin(wire.from.0, wire.from.1),
                        to: wire.to,
                        waypoints: wire.waypoints,
                    })
                    .collect(),
            })
            .collect();

        Self {
            version: CURRENT_VERSION,
            circuits,
        }
    }
}

/// The version 2 format: wires were explicit and routed, but always began at
/// a component pin.
mod v2 {
    use super::{SavedComponent, SavedEndpoint};
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct SavedProject {
        pub circuits: Vec<SavedCircuit>,
    }

    #[derive(Deserialize)]
    pub struct SavedCircuit {
        pub name: String,
        pub components: Vec<SavedComponent>,
        pub wires: Vec<SavedWire>,
    }

    #[derive(Deserialize)]
    pub struct SavedWire {
        /// `(component index, pin index)`.
        pub from: (usize, usize),
        pub to: SavedEndpoint,
        pub waypoints: Vec<(f32, f32)>,
    }
}

/// The version 1 format, kept only so old files still open — nothing writes
/// it any more.
mod v1 {
    use super::SavedComponent;
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct SavedProject {
        pub circuits: Vec<SavedCircuit>,
    }

    #[derive(Deserialize)]
    pub struct SavedCircuit {
        pub name: String,
        pub components: Vec<SavedComponent>,
        /// Each inner list is a group of `(component index, pin index)`
        /// pairs — every pin named in one group shares a single net.
        pub wires: Vec<Vec<(usize, usize)>>,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_2_project_round_trips() {
        let project = SavedProject {
            version: CURRENT_VERSION,
            circuits: vec![SavedCircuit {
                name: "main".to_string(),
                components: vec![SavedComponent {
                    kind: ComponentKind::Button,
                    x: 40.0,
                    y: 60.0,
                    rotation: Rotation::Deg90,
                }],
                wires: vec![SavedWire {
                    from: SavedEndpoint::Pin(0, 0),
                    to: SavedEndpoint::Junction {
                        wire: 0,
                        waypoint: 1,
                    },
                    waypoints: vec![(20.0, 20.0), (40.0, 20.0)],
                }],
            }],
        };

        let json = serde_json::to_string(&project).expect("serializes");
        let parsed = SavedProject::from_json(&json).expect("parses");

        assert_eq!(parsed.version, CURRENT_VERSION);
        let circuit = &parsed.circuits[0];
        assert_eq!(circuit.components.len(), 1);
        assert_eq!(circuit.wires[0].waypoints, vec![(20.0, 20.0), (40.0, 20.0)]);
        assert!(matches!(
            circuit.wires[0].to,
            SavedEndpoint::Junction {
                wire: 0,
                waypoint: 1
            }
        ));
    }

    #[test]
    fn a_version_2_wire_start_migrates_to_an_explicit_pin_endpoint() {
        let json = r#"{
            "version": 2,
            "circuits": [{
                "name": "main",
                "components": [
                    {"kind": "Button", "x": 0.0, "y": 0.0, "rotation": "Deg0"},
                    {"kind": "Led", "x": 80.0, "y": 0.0, "rotation": "Deg0"}
                ],
                "wires": [
                    {"from": [0, 0], "to": {"Pin": [1, 0]}, "waypoints": [[20.0, 20.0]]}
                ]
            }]
        }"#;

        let parsed = SavedProject::from_json(json).expect("parses");

        assert_eq!(parsed.version, CURRENT_VERSION);
        let wire = &parsed.circuits[0].wires[0];
        assert!(matches!(wire.from, SavedEndpoint::Pin(0, 0)));
        // The route survives the migration untouched.
        assert_eq!(wire.waypoints, vec![(20.0, 20.0)]);
    }

    #[test]
    fn a_version_1_pin_group_migrates_to_a_star_of_wires() {
        // Three pins sharing one net: v1's single group becomes two wires
        // out of the group's first pin, matching what v1 drew.
        let json = r#"{
            "version": 1,
            "circuits": [{
                "name": "main",
                "components": [
                    {"kind": "Button", "x": 0.0, "y": 0.0, "rotation": "Deg0"},
                    {"kind": "Led", "x": 80.0, "y": 0.0, "rotation": "Deg0"},
                    {"kind": "Probe", "x": 160.0, "y": 0.0, "rotation": "Deg0"}
                ],
                "wires": [[[0, 0], [1, 0], [2, 0]]]
            }]
        }"#;

        let parsed = SavedProject::from_json(json).expect("parses");

        assert_eq!(parsed.version, CURRENT_VERSION);
        let wires = &parsed.circuits[0].wires;
        assert_eq!(wires.len(), 2);
        assert!(wires
            .iter()
            .all(|wire| matches!(wire.from, SavedEndpoint::Pin(0, 0))));
        assert!(wires.iter().all(|wire| wire.waypoints.is_empty()));
        assert!(matches!(wires[0].to, SavedEndpoint::Pin(1, 0)));
        assert!(matches!(wires[1].to, SavedEndpoint::Pin(2, 0)));
    }

    #[test]
    fn an_unknown_version_is_rejected_rather_than_guessed_at() {
        let json = r#"{"version": 99, "circuits": []}"#;
        assert!(SavedProject::from_json(json).is_err());
    }
}
