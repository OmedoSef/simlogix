//! The on-disk project format: what gets saved/loaded.
//!
//! A project is a **zip container**: `project.json` lists the circuits in
//! order, and each circuit has its own file under `circuits/`. Splitting it
//! up isn't for speed — the whole document is read into memory either way —
//! it's so the format has somewhere to put things that aren't JSON. The
//! roadmap includes user-drawn component symbols, and a container is much
//! cheaper to introduce now than once there are projects on disk.
//!
//! Structural only — which components, where, how their pins are wired
//! together, and how each wire is routed — never runtime state (a button's
//! pressed state, a net's current signal). Loading a project starts it cold,
//! like opening a fresh Logisim circuit.
//!
//! A project holds one or more named circuits. They're independent for now:
//! one can't yet be placed inside another as a component, which is what
//! would make the list a genuine hierarchy.
//!
//! This is also what an undo step is made of (see `SimLogixApp::record_edit`):
//! a snapshot is just a `SavedProject`, so undo and save/load share one
//! definition of "everything that makes up a circuit" instead of drifting.

use serde::{Deserialize, Serialize};

use crate::canvas::Rotation;
use crate::palette::ComponentKind;
use crate::properties::Properties;

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
/// - `4` — the document became a zip container rather than one JSON file.
/// - `5` — a project carries a `library` name, and components are saved
///   qualified by library (`simlogix:And`). Both exist so that a circuit
///   imported from another project can be told apart from a local one of
///   the same name.
/// - `6` — circuits can be filed in folders.
/// - `7` — a component can carry properties (a name, and per-kind settings
///   such as a button's resting state or a LED's colour). Absent means the
///   behaviour that was there before, so nothing needed migrating.
/// - `8` — a wire can carry a colour of its own.
pub const CURRENT_VERSION: u32 = 8;

/// What a project is saved as.
pub const PROJECT_EXTENSION: &str = "slgx";

/// What projects used to be saved as. Nothing writes it any more, but the
/// open dialog still offers it — those files load fine, since the format is
/// recognised from the bytes rather than the name.
pub const LEGACY_EXTENSION: &str = "simlogix";

/// The container's index, `project.json`: the document's version and the
/// circuits it holds, in order.
///
/// Authoritative for the name-to-file mapping. A circuit's name is free
/// text and may contain characters no file name can, so the two are allowed
/// to differ and only this reconciles them.
#[derive(Debug, Serialize, Deserialize)]
struct Index {
    version: u32,
    #[serde(default)]
    library: String,
    #[serde(default)]
    folders: Vec<String>,
    circuits: Vec<IndexEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexEntry {
    name: String,
    /// Relative to `circuits/`.
    file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProject {
    pub version: u32,
    /// The name other projects use to refer to this one's circuits — the
    /// namespace half of a qualified component name.
    ///
    /// Deliberately **not** the file name. A file gets renamed, and two
    /// machines can easily both hold a `test.slgx` with different contents;
    /// either would silently repoint or collide every reference made to it.
    /// Stored once, it survives both, and a clash between two projects is
    /// something the user can actually fix by editing it.
    ///
    /// Empty means "never set" — a project from before v5, or one not yet
    /// saved anywhere. The GUI fills it in from the file name at that point.
    #[serde(default)]
    pub library: String,
    /// The folders circuits can be filed in, as `/`-separated paths.
    ///
    /// Held explicitly, rather than inferred from where the circuits
    /// actually are, so that a folder you've just made survives a save
    /// while it's still empty — otherwise the only way to create one would
    /// be to create a circuit first, which is backwards.
    #[serde(default)]
    pub folders: Vec<String>,
    pub circuits: Vec<SavedCircuit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCircuit {
    pub name: String,
    /// Which folder this circuit is filed in — a `/`-separated path, empty
    /// for the top level.
    ///
    /// Presentation, not identity: a circuit is referred to as
    /// `library:name` wherever it sits, so filing it somewhere else never
    /// invalidates a reference to it. The cost is that names have to be
    /// unique across the whole project rather than per folder.
    #[serde(default)]
    pub folder: String,
    pub components: Vec<SavedComponent>,
    pub wires: Vec<SavedWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedComponent {
    pub kind: ComponentKind,
    pub x: f32,
    pub y: f32,
    pub rotation: Rotation,
    /// What the user has set on this one. Left out of the file entirely
    /// when nothing has been — see [`Properties`].
    #[serde(default, skip_serializing_if = "Properties::is_empty")]
    pub properties: Properties,
}

/// One drawn wire: two endpoints and the route between them. Positions are
/// stored as plain `(x, y)` pairs rather than an egui type, to keep the file
/// format independent of the GUI toolkit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedWire {
    pub from: SavedEndpoint,
    pub to: SavedEndpoint,
    pub waypoints: Vec<(f32, f32)>,
    /// The user's own colour for this wire, if set. Every wire of a net
    /// carries the same one — the net is what's being coloured, but a
    /// `NetId` doesn't survive an edit, so the wires hold it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
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

impl SavedCircuit {
    /// How another circuit in the same project refers to this one:
    /// `adder`, or `alu/adder` when it's filed in a folder.
    pub fn path(&self) -> String {
        if self.folder.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.folder, self.name)
        }
    }

    /// Which `(component index, pin index)` pairs this circuit's wires hold
    /// together — its connectivity, read straight off the saved drawing.
    ///
    /// The same union-find `SimLogixApp::rebuild_nets` runs on the live
    /// drawing, done here on saved data because instantiating this circuit
    /// somewhere else needs its internal connections without opening it.
    /// Groups of one are left out: a lone pin is its own net anyway.
    pub fn pin_groups(&self) -> Vec<Vec<(usize, usize)>> {
        let mut parent = std::collections::HashMap::new();

        for (index, wire) in self.wires.iter().enumerate() {
            let self_node = SavedNode::Wire(index);
            parent.entry(self_node).or_insert(self_node);
            for end in [&wire.from, &wire.to] {
                match *end {
                    SavedEndpoint::Pin(component, pin) => {
                        union(&mut parent, self_node, SavedNode::Pin(component, pin));
                    }
                    SavedEndpoint::Junction { wire: host, .. } => {
                        union(&mut parent, self_node, SavedNode::Wire(host));
                    }
                    // A loose end connects nothing.
                    SavedEndpoint::Free(_, _) => {}
                }
            }
        }

        let mut groups: std::collections::HashMap<SavedNode, Vec<(usize, usize)>> =
            std::collections::HashMap::new();
        let nodes: Vec<SavedNode> = parent.keys().copied().collect();
        for node in nodes {
            if let SavedNode::Pin(component, pin) = node {
                let root = find(&mut parent, node);
                groups.entry(root).or_default().push((component, pin));
            }
        }
        groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect()
    }
}

/// A node of a saved circuit's connectivity graph — the same shape
/// `SimLogixApp::rebuild_nets` uses on the live drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SavedNode {
    Pin(usize, usize),
    Wire(usize),
}

fn find(
    parent: &mut std::collections::HashMap<SavedNode, SavedNode>,
    node: SavedNode,
) -> SavedNode {
    let mut root = node;
    while let Some(&next) = parent.get(&root) {
        if next == root {
            break;
        }
        root = next;
    }
    // Path compression, so a long chain of taps doesn't cost a walk each time.
    let mut walk = node;
    while let Some(&next) = parent.get(&walk) {
        if next == walk {
            break;
        }
        parent.insert(walk, root);
        walk = next;
    }
    root
}

fn union(parent: &mut std::collections::HashMap<SavedNode, SavedNode>, a: SavedNode, b: SavedNode) {
    parent.entry(a).or_insert(a);
    parent.entry(b).or_insert(b);
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent.insert(ra, rb);
    }
}

impl SavedProject {
    /// Packs the project into its container.
    ///
    /// Entries are **stored, not deflated**. These documents are a few
    /// kilobytes, so compressing them saves nothing worth having, while an
    /// uncompressed entry stays readable from outside the app and lets git
    /// delta successive versions of a project — a one-character edit
    /// rewrites an entire deflate stream, which is what makes a compressed
    /// container so hostile to version control.
    pub fn to_container(&self) -> Result<Vec<u8>, String> {
        use std::io::Write;

        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));

        let mut used = std::collections::HashSet::new();
        let index = Index {
            version: CURRENT_VERSION,
            library: self.library.clone(),
            folders: self.folders.clone(),
            circuits: self
                .circuits
                .iter()
                .map(|circuit| IndexEntry {
                    name: circuit.name.clone(),
                    file: unique_file_path(&circuit.folder, &circuit.name, &mut used),
                })
                .collect(),
        };

        // A plain function rather than a closure: a closure capturing `zip`
        // would hold the borrow for the rest of the block, and the directory
        // entries below need it too.
        fn write<W: std::io::Write + std::io::Seek>(
            zip: &mut zip::ZipWriter<W>,
            options: zip::write::FileOptions<'_, ()>,
            path: &str,
            json: String,
        ) -> Result<(), String> {
            zip.start_file(path, options)
                .map_err(|err| err.to_string())?;
            zip.write_all(json.as_bytes())
                .map_err(|err| err.to_string())
        }

        write(
            &mut zip,
            options,
            "project.json",
            serde_json::to_string_pretty(&index).map_err(|err| err.to_string())?,
        )?;

        // A folder holding nothing has no file to imply it, so it gets a
        // directory entry of its own — browsing the archive should show the
        // project's folders, all of them, not just the ones that happen to
        // have something in them.
        for folder in &self.folders {
            zip.add_directory(format!("circuits/{}", sanitised_path(folder)), options)
                .map_err(|err| err.to_string())?;
        }
        for (circuit, entry) in self.circuits.iter().zip(&index.circuits) {
            write(
                &mut zip,
                options,
                &format!("circuits/{}", entry.file),
                serde_json::to_string_pretty(circuit).map_err(|err| err.to_string())?,
            )?;
        }

        let cursor = zip.finish().map_err(|err| err.to_string())?;
        Ok(cursor.into_inner())
    }

    /// Reads a project from a file's raw bytes, in whichever format it is:
    /// a container, or the single JSON document that came before it.
    ///
    /// Told apart by the bytes rather than the extension, so a renamed file
    /// still opens and an old `.simlogix` needs no handling of its own.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        // Every zip starts with a local file header signature.
        if bytes.starts_with(b"PK") {
            Self::from_container(bytes)
        } else {
            let json = std::str::from_utf8(bytes).map_err(|err| err.to_string())?;
            Self::from_json(json)
        }
    }

    fn from_container(bytes: &[u8]) -> Result<Self, String> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|err| err.to_string())?;

        let index: Index = {
            let file = archive
                .by_name("project.json")
                .map_err(|_| "the project has no project.json".to_string())?;
            serde_json::from_reader(file).map_err(|err| err.to_string())?
        };
        if index.version > CURRENT_VERSION {
            return Err(format!(
                "unsupported project format version {}",
                index.version
            ));
        }

        let mut circuits = Vec::with_capacity(index.circuits.len());
        for entry in &index.circuits {
            let path = format!("circuits/{}", entry.file);
            let file = archive
                .by_name(&path)
                .map_err(|_| format!("the project is missing {path}"))?;
            let mut circuit: SavedCircuit =
                serde_json::from_reader(file).map_err(|err| err.to_string())?;
            // The index names the circuits; a circuit's own file carries a
            // copy of its name for readability, but the index wins.
            circuit.name.clone_from(&entry.name);
            circuits.push(circuit);
        }

        Ok(Self {
            version: CURRENT_VERSION,
            library: index.library,
            folders: index.folders,
            circuits,
        })
    }

    /// Parses the pre-container single-document format, migrating older
    /// versions forward. The version
    /// is read before anything else, since it's what decides how the rest of
    /// the document should even be interpreted.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|err| err.to_string())?;
        let version = value
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "missing format version".to_string())?;

        let mut project = match version {
            1 => serde_json::from_value::<v1::SavedProject>(value)
                .map_err(|err| err.to_string())
                .map(Self::from_v1)
                .map(Self::from_v2),
            2 => serde_json::from_value::<v2::SavedProject>(value)
                .map_err(|err| err.to_string())
                .map(Self::from_v2),
            3 => serde_json::from_value(value).map_err(|err| err.to_string()),
            // 4 and on are containers, never a bare JSON document.
            other => Err(format!("unsupported project format version {other}")),
        }?;

        // `version` records the shape held in memory, which is the current
        // one whatever was read: the file's own version only ever decided
        // how to interpret it. Stamped here rather than in each migration,
        // so the branch that needs no migration can't be the one to forget.
        project.version = CURRENT_VERSION;
        Ok(project)
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
                // Folders arrived in v6; everything older is top level.
                folder: String::new(),
                components: circuit.components,
                wires: circuit
                    .wires
                    .into_iter()
                    .map(|wire| SavedWire {
                        from: SavedEndpoint::Pin(wire.from.0, wire.from.1),
                        to: wire.to,
                        waypoints: wire.waypoints,
                        // Wire colours arrived in v8; older wires have none.
                        color: None,
                    })
                    .collect(),
            })
            .collect();

        Self {
            version: CURRENT_VERSION,
            // Pre-v5 documents have no library of their own; the GUI names
            // them after the file they came from on the way in.
            library: String::new(),
            folders: Vec::new(),
            circuits,
        }
    }
}

/// One path segment made safe to use as a file name: anything that isn't
/// alphanumeric, `-` or `_` is replaced.
///
/// That's also what stops a name containing `/` or `..` from reaching
/// outside `circuits/` — a segment can't become two, and can't become a
/// parent reference.
fn sanitised_segment(segment: &str) -> String {
    let cleaned: String = segment
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches('_').is_empty() {
        "_".to_string()
    } else {
        cleaned
    }
}

/// A folder path with every segment made safe, keeping the `/` between them
/// so the archive mirrors the tree the user sees.
fn sanitised_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(sanitised_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Where a circuit's file goes inside `circuits/`: its folder, mirrored, then
/// its own name, with a numeric suffix when two land on the same path.
///
/// Legibility is the whole point — `project.json` is what actually maps a
/// name to its file, so nothing breaks if a name has to be mangled beyond
/// recognition, but a project should be browsable with an ordinary zip tool
/// and look like what the editor shows.
fn unique_file_path(
    folder: &str,
    name: &str,
    used: &mut std::collections::HashSet<String>,
) -> String {
    let directory = sanitised_path(folder);
    let stem = sanitised_segment(name);
    let join = |file: &str| {
        if directory.is_empty() {
            file.to_string()
        } else {
            format!("{directory}/{file}")
        }
    };

    let mut candidate = join(&format!("{stem}.json"));
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = join(&format!("{stem}-{suffix}.json"));
        suffix += 1;
    }
    candidate
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

    /// The last shape the single-document format had. Nothing writes it any
    /// more — this guards the reading path, which still has to work.
    #[test]
    fn a_version_3_json_document_still_round_trips() {
        let project = SavedProject {
            version: 3,
            library: String::new(),
            folders: Vec::new(),
            circuits: vec![SavedCircuit {
                name: "main".to_string(),
                folder: String::new(),
                components: vec![SavedComponent {
                    kind: ComponentKind::Button,
                    x: 40.0,
                    y: 60.0,
                    rotation: Rotation::Deg90,
                    properties: Properties::default(),
                }],
                wires: vec![SavedWire {
                    color: None,
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

    fn wire(from: SavedEndpoint, to: SavedEndpoint) -> SavedWire {
        SavedWire {
            from,
            to,
            waypoints: Vec::new(),
            color: None,
        }
    }

    #[test]
    fn pin_groups_follow_a_chain_of_wires_through_a_junction() {
        let mut circuit = circuit("main");
        circuit.components = vec![circuit.components[0].clone(); 3];
        circuit.wires = vec![
            wire(SavedEndpoint::Pin(0, 0), SavedEndpoint::Pin(1, 0)),
            // Tapped onto the first wire rather than onto a pin: the third
            // pin still belongs to the same net, which is the whole reason
            // this is a union-find and not a scan of endpoints.
            wire(
                SavedEndpoint::Junction {
                    wire: 0,
                    waypoint: 0,
                },
                SavedEndpoint::Pin(2, 0),
            ),
        ];

        let groups = circuit.pin_groups();

        assert_eq!(groups.len(), 1);
        let mut group = groups[0].clone();
        group.sort();
        assert_eq!(group, vec![(0, 0), (1, 0), (2, 0)]);
    }

    #[test]
    fn a_wire_with_a_loose_end_groups_nothing() {
        let mut circuit = circuit("main");
        circuit.components = vec![circuit.components[0].clone(); 2];
        circuit.wires = vec![wire(
            SavedEndpoint::Pin(0, 0),
            SavedEndpoint::Free(10.0, 10.0),
        )];

        // One pin on a wire is not a connection: a group of one is no group.
        assert!(circuit.pin_groups().is_empty());
    }

    #[test]
    fn an_unknown_version_is_rejected_rather_than_guessed_at() {
        let json = r#"{"version": 99, "circuits": []}"#;
        assert!(SavedProject::from_json(json).is_err());
    }

    fn circuit(name: &str) -> SavedCircuit {
        SavedCircuit {
            name: name.to_string(),
            folder: String::new(),
            components: vec![SavedComponent {
                kind: ComponentKind::Button,
                x: 20.0,
                y: 40.0,
                rotation: Rotation::Deg0,
                properties: Properties::default(),
            }],
            wires: Vec::new(),
        }
    }

    #[test]
    fn a_container_round_trips_every_circuit_in_order() {
        let project = SavedProject {
            version: CURRENT_VERSION,
            library: "test".to_string(),
            folders: Vec::new(),
            circuits: vec![circuit("main"), circuit("adder")],
        };

        let bytes = project.to_container().expect("packs");
        let parsed = SavedProject::from_bytes(&bytes).expect("unpacks");

        assert_eq!(parsed.version, CURRENT_VERSION);
        assert_eq!(parsed.library, "test");
        let names: Vec<&str> = parsed
            .circuits
            .iter()
            .map(|circuit| circuit.name.as_str())
            .collect();
        assert_eq!(names, ["main", "adder"]);
        assert_eq!(parsed.circuits[0].components.len(), 1);
    }

    #[test]
    fn a_pre_container_json_file_still_opens() {
        // The same document the old format wrote, read straight from bytes:
        // the format is recognised from the content, so an old `.simlogix`
        // needs no handling of its own.
        let json = br#"{
            "version": 3,
            "circuits": [{
                "name": "main",
                "components": [{"kind": "Led", "x": 0.0, "y": 0.0, "rotation": "Deg0"}],
                "wires": []
            }]
        }"#;

        let parsed = SavedProject::from_bytes(json).expect("parses");

        assert_eq!(parsed.version, CURRENT_VERSION);
        assert_eq!(parsed.circuits[0].name, "main");
        assert_eq!(parsed.circuits[0].components.len(), 1);
        // No library of its own: the GUI names it after its file on the way
        // in, which is the one moment a file name is allowed to decide this.
        assert!(parsed.library.is_empty());
    }

    #[test]
    fn names_that_sanitise_to_the_same_file_still_get_one_each() {
        // Two names a file system can't tell apart, plus one that sanitises
        // away entirely. All three have to survive the trip.
        let project = SavedProject {
            version: CURRENT_VERSION,
            library: "test".to_string(),
            folders: Vec::new(),
            circuits: vec![circuit("a/b"), circuit("a:b"), circuit("///")],
        };

        let bytes = project.to_container().expect("packs");
        let parsed = SavedProject::from_bytes(&bytes).expect("unpacks");

        let names: Vec<&str> = parsed
            .circuits
            .iter()
            .map(|circuit| circuit.name.as_str())
            .collect();
        assert_eq!(names, ["a/b", "a:b", "///"]);
    }

    fn archive_paths(bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("is an archive");
        (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|file| file.name().to_string()))
            .collect()
    }

    #[test]
    fn the_archive_mirrors_the_folder_tree() {
        let mut filed = circuit("adder");
        filed.folder = "alu/decode".to_string();
        let project = SavedProject {
            version: CURRENT_VERSION,
            library: "cpu".to_string(),
            // An empty folder has no file to imply it, so it needs an entry
            // of its own or browsing the archive would under-report.
            folders: vec![
                "alu".to_string(),
                "alu/decode".to_string(),
                "scratch".to_string(),
            ],
            circuits: vec![circuit("main"), filed],
        };

        let bytes = project.to_container().expect("packs");
        let paths = archive_paths(&bytes);

        assert!(
            paths.contains(&"circuits/main.json".to_string()),
            "got {paths:?}"
        );
        assert!(
            paths.contains(&"circuits/alu/decode/adder.json".to_string()),
            "got {paths:?}"
        );
        assert!(
            paths
                .iter()
                .any(|path| path.starts_with("circuits/scratch")),
            "the empty folder should still show: {paths:?}"
        );

        // And it all comes back, folders included.
        let parsed = SavedProject::from_bytes(&bytes).expect("unpacks");
        assert_eq!(parsed.folders, project.folders);
        assert_eq!(parsed.circuits[1].folder, "alu/decode");
    }

    #[test]
    fn a_folder_name_cannot_escape_the_circuits_folder_either() {
        let mut filed = circuit("adder");
        filed.folder = "../../etc".to_string();
        let project = SavedProject {
            version: CURRENT_VERSION,
            library: "cpu".to_string(),
            folders: vec!["../../etc".to_string()],
            circuits: vec![filed],
        };

        let bytes = project.to_container().expect("packs");
        let paths = archive_paths(&bytes);

        assert!(
            paths.iter().all(|path| !path.contains("..")),
            "got {paths:?}"
        );
    }

    #[test]
    fn a_circuit_name_cannot_escape_the_circuits_folder() {
        let project = SavedProject {
            version: CURRENT_VERSION,
            library: "test".to_string(),
            folders: Vec::new(),
            circuits: vec![circuit("../../etc/passwd")],
        };

        let bytes = project.to_container().expect("packs");
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).expect("is an archive");
        let paths: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();

        assert!(
            paths.iter().all(|path| !path.contains("..")),
            "got {paths:?}"
        );
    }
}
