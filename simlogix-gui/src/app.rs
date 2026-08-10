//! The SimLogix application: state and the `eframe::App` loop.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use simlogix_core::{
    And, Buffer, BusTransceiver, Button, Circuit, CircuitAnchor, CircuitOutput, CircuitPort, Clock,
    Component, ComponentId, Led, Nand, NetId, Nor, Not, Or, Pin, PinDirection, Probe, Rail,
    SrLatch, Transistor, TriStateBuffer, Xnor, Xor,
};

use crate::appearance::Appearance;
use crate::canvas::{self, BOX_SIZE};
use crate::circuit_tree::{self, RenameTarget, TreeAction};
use crate::i18n::{Language, Strings};
use crate::palette::{self, ComponentKind};
use crate::placed_component::{InstancePort, InstanceWiring, PlacedComponent};
use crate::project::{self, SavedCircuit, SavedComponent, SavedEndpoint, SavedProject, SavedWire};
use crate::properties::{self, Properties};
use crate::toolbar::{self, Tool};

/// Logical ticks the circuit advances per real second — the resolution the
/// whole simulation runs at. A `Clock`'s period is expressed in these ticks
/// (see `CLOCK_PERIOD_TICKS`), so this constant is what ties "one clock
/// toggle" to a specific amount of real time.
const TICKS_PER_SECOND: f32 = 60.0;
/// A placed `Clock` toggles once every this many ticks — with
/// `TICKS_PER_SECOND` above, that's once per real second.
const CLOCK_PERIOD_TICKS: u64 = 60;
/// How far an interactive change (a press, a new wire, a deletion) advances
/// the circuit to settle. Deliberately small: unlike `Circuit::run` (which
/// would let a `Clock` anywhere in the circuit burn through a huge chunk of
/// its future ticks in one synchronous burst), this barely nudges real time,
/// while still being generous enough for a modest chain of gates to fully
/// propagate in one go.
pub(crate) const SETTLE_TICKS: u64 = 32;

/// What is picked while a symbol is being drawn.
///
/// Shapes and pins are held apart rather than in one list of an enum: they
/// are edited by different panels and moved by different steps — a pin has
/// to land on a grid dot, a shape only on a quarter of one — so every reader
/// wants one or the other anyway.
///
/// Indices, not ids: shapes are a plain drawing order and pins follow the
/// ports, so there is nothing an id would keep stable that matters. The two
/// are cleared together whenever either list can shift.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SymbolSelection {
    pub shapes: Vec<usize>,
    pub pins: Vec<usize>,
}

impl SymbolSelection {
    fn is_empty(&self) -> bool {
        self.shapes.is_empty() && self.pins.is_empty()
    }

    /// Adds or removes one, for a shift-click.
    fn toggle(list: &mut Vec<usize>, index: usize) {
        if let Some(at) = list.iter().position(|held| *held == index) {
            list.remove(at);
        } else {
            list.push(index);
        }
    }
}

/// Side of the square grab area on a pin in the appearance view.
const APPEARANCE_PIN_HANDLE: f32 = 14.0;

/// How near a click has to land to pick a shape.
const SHAPE_PICK_RADIUS: f32 = 6.0;

/// Size a label is dropped at, before the panel is used to change it.
const DEFAULT_SHAPE_TEXT_SIZE: f32 = 10.0;

/// Marks a clipboard payload as shapes of a symbol, so pasting a URL or a
/// stretch of prose does nothing rather than being half-read.
const SYMBOL_CLIPBOARD_TAG: &str = "simlogix_symbol:";
/// How many undo steps are kept. Each one is a whole `SavedProject`, so this
/// bounds memory rather than letting a long session grow without limit.
const MAX_UNDO_STEPS: usize = 64;
/// Zoom limits, as scene-to-screen scale. Bounded in both directions so the
/// circuit can't be shrunk to a dot or blown up past the point where a grid
/// step fills the window.
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
/// How much one notch of the wheel zooms. Applied as `exp(-scroll * this)`,
/// so zooming compounds evenly instead of accelerating as you go in.
const WHEEL_ZOOM_SENSITIVITY: f32 = 0.0015;
/// How much empty room to leave around the drawing when framing it, so a
/// component at the edge doesn't sit flush against the panel beside it.
const FIT_MARGIN: f32 = 3.0 * canvas::GRID_SPACING;
/// How close a click has to land to a wire to count as hitting it.
const WIRE_HIT_RADIUS: f32 = 6.0;
/// How close a dropped loose end has to land to a pin or another wire's
/// point to re-attach there instead of staying loose. Wider than
/// [`WIRE_HIT_RADIUS`]: this is a deliberate drop, so it should forgive a
/// few pixels.
const REATTACH_RADIUS: f32 = 12.0;

/// One end of a [`Wire`] that isn't a component pin: a tap into another
/// wire's waypoint (a junction/contact point), created by finishing a new
/// wire on top of an existing one's waypoint instead of a pin. Tracked by
/// the host wire's id and which of its waypoints — resolved fresh every
/// frame (see `ui()`'s pre-pass) — so moving that point (even dragging the
/// host's own implicit default-route point, which materializes into a real
/// waypoint the moment it's first dragged) drags this tap along with it,
/// same as any other point on the host would. A wire can only tap into an
/// *earlier* one (you can only tap something that already exists), which is
/// what makes that pre-pass a single forward sweep instead of needing to
/// handle cycles.
#[derive(Clone, Copy, PartialEq)]
enum WireEndpoint {
    Pin(ComponentId, usize),
    Junction {
        wire: u64,
        waypoint: usize,
    },
    /// A loose end, left where a junction used to be after its host wire (or
    /// the contact point itself) was deleted. The wire stays visible and
    /// editable instead of vanishing — deleting one thing shouldn't quietly
    /// take unrelated wiring with it.
    Free(egui::Pos2),
}

/// A single user-drawn wire: two endpoints — either of which may be a pin, a
/// tap onto another wire, or a loose point — plus every waypoint between
/// them, grid-snapped, in order. Both ends are the same kind of thing so a
/// wire outlives the component it was drawn to, and so splitting one can
/// leave a piece that begins nowhere in particular.
/// Replaces inferring wire topology from net membership (a "star" from an
/// arbitrary anchor pin to every other pin sharing a net): that model had no
/// way to represent a wire ending on a point that isn't a pin at all, which
/// is exactly what a junction is.
struct Wire {
    id: u64,
    from: WireEndpoint,
    to: WireEndpoint,
    waypoints: Vec<egui::Pos2>,
    /// A colour of the user's own, drawn as a casing around the signal
    /// colour so that a wire stays recognisable where it crosses another.
    ///
    /// Held per wire even though it *means* per net, because a `NetId` is
    /// only valid within one frame — see `rebuild_nets`. Setting it writes
    /// it to every wire in the net, and `inherit_wire_colors` gives it to a
    /// wire newly joined to one, so the distinction stays invisible.
    color: Option<[u8; 3]>,
}

/// What is currently selected, components and wires together.
///
/// These used to be two mutually exclusive `Option`s, with `Delete` checking
/// the wire first so a stale wire selection couldn't shadow a component. A
/// rubber band picks up whatever it lands on, so "which of the two is
/// selected" stopped being a question worth asking.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Selection {
    components: HashSet<ComponentId>,
    wires: HashSet<u64>,
}

impl Selection {
    fn is_empty(&self) -> bool {
        self.components.is_empty() && self.wires.is_empty()
    }

    fn len(&self) -> usize {
        self.components.len() + self.wires.len()
    }

    fn clear(&mut self) {
        self.components.clear();
        self.wires.clear();
    }

    /// Replaces the whole selection with one component, or adds it to what's
    /// already there when `add` — the Shift-click gesture.
    fn pick_component(&mut self, id: ComponentId, add: bool) {
        if add {
            if !self.components.remove(&id) {
                self.components.insert(id);
            }
        } else {
            self.clear();
            self.components.insert(id);
        }
    }

    fn pick_wire(&mut self, id: u64, add: bool) {
        if add {
            if !self.wires.remove(&id) {
                self.wires.insert(id);
            }
        } else {
            self.clear();
            self.wires.insert(id);
        }
    }

    /// The single selected component, if that's all there is — what the
    /// properties panel edits. Several selected means no one set of
    /// properties to show.
    fn lone_component(&self) -> Option<ComponentId> {
        match (self.components.len(), self.wires.len()) {
            (1, 0) => self.components.iter().copied().next(),
            _ => None,
        }
    }

    fn lone_wire(&self) -> Option<u64> {
        match (self.components.len(), self.wires.len()) {
            (0, 1) => self.wires.iter().copied().next(),
            _ => None,
        }
    }
}

/// The preferences that outlive a session, written by `eframe`'s storage.
///
/// Deliberately only the things the user *chose*. The theme and the panel
/// sizes are egui's own state and it persists them itself once the feature
/// is on; the document, the camera and the selection are not preferences at
/// all. Everything is optional so a settings file written by an older build
/// still loads — the same rule the project format follows.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    /// `None` keeps following the OS locale, which is the default until the
    /// user picks one — so "I never chose" and "I chose English" stay
    /// different answers.
    #[serde(default)]
    language: Option<Language>,
    #[serde(default)]
    left_drag_pans: bool,
    /// Projects opened or saved recently, most recent first.
    ///
    /// A preference, not document state: it says what *you* have been
    /// working on, so it survives opening a project — which resets almost
    /// everything else.
    #[serde(default)]
    recent: Vec<PathBuf>,
}

/// How many projects the *Open recent* menu remembers.
///
/// Enough to cover a few days of moving between projects, short enough that
/// the list stays something you read rather than search.
const MAX_RECENT: usize = 8;

/// The key `eframe` files our settings under in its storage.
const SETTINGS_KEY: &str = "simlogix_settings";

/// A copied fragment of a circuit, in the same form a project file uses.
///
/// It goes on the **system** clipboard as JSON rather than into a field of
/// our own, and not for the obvious reason. egui only reports a paste when
/// the system clipboard actually holds text — so an in-app clipboard would
/// have meant `Ctrl+V` silently doing nothing whenever the real clipboard
/// was empty, which is most of the time. Putting the fragment there makes
/// the event fire, and pasting between two windows works as a consequence.
#[derive(Serialize, Deserialize, Clone)]
struct Fragment {
    /// Marks the text as ours. Pasting anything else — a URL, some prose —
    /// parses to nothing and does nothing, rather than being half-read.
    simlogix_fragment: u32,
    components: Vec<SavedComponent>,
    wires: Vec<SavedWire>,
}

/// A wire being placed click by click: the pin and net it started from, the
/// screen position of that pin, and every waypoint confirmed so far
/// (grid-snapped, in order) — the segment from the last of these to the
/// current pointer position is drawn as a live preview until the wire is
/// finished or cancelled.
struct WireInProgress {
    from: WireEndpoint,
    /// The net the start already sits on, if it started on something. A wire
    /// begun on empty canvas has none until one of its ends is connected.
    net: Option<NetId>,
    anchor: egui::Pos2,
    waypoints: Vec<egui::Pos2>,
}

/// Where a wire being drawn is about to tap into an existing one: either a
/// contact point that's already there, or one to create at that spot.
///
/// Deciding this inside the render loop but applying it afterwards keeps
/// `self.wires` untouched (stable length, no reallocation) for the whole
/// iteration — and lets the snapshot for undo be taken before the tap
/// modifies anything.
enum JunctionTarget {
    Existing {
        wire: u64,
        waypoint: usize,
    },
    Insert {
        wire: u64,
        waypoint: usize,
        /// The host's full waypoint list with the new contact point already
        /// in it, ready to be swapped in wholesale.
        waypoints: Vec<egui::Pos2>,
    },
}

/// A destructive action held back by the unsaved-changes confirmation until
/// the user says what to do about the current circuit.
/// Not `Copy`: `OpenRecent` carries the path it is going to open.
#[derive(Clone, PartialEq)]
enum PendingAction {
    New,
    Open,
    /// A project chosen from the recent list. It goes through the same
    /// unsaved-changes guard as *Open* — the file being already named is no
    /// reason to throw work away without asking.
    OpenRecent(PathBuf),
    Quit,
}

/// Pick a kind from the palette, click the canvas to drop it (snapped to the
/// grid), then click one pin to start a wire, click the canvas as many times
/// as you like to lay down grid-snapped waypoints, and click a pin — or an
/// existing wire's waypoint, to tap into it as a junction — to finish it.
/// The wires are the record of what's connected; the nets in `circuit` are
/// recomputed from them after every edit (see `rebuild_nets`). Escape
/// cancels a wire in progress.
pub struct SimLogixApp {
    show_about: bool,
    /// Whether wires are coloured by the signal they carry. Off draws them
    /// as plain structure — see the Simulation menu. Session state, like
    /// `running`: not a stored preference.
    show_signal_state: bool,
    /// Whether the shortcuts-and-gestures window is up. View state, like
    /// `show_about`: never saved, never an undo step.
    show_shortcuts: bool,
    licenses: crate::licenses::State,
    circuit: Circuit,
    placed: Vec<PlacedComponent>,
    /// What the next canvas click does — placing, wiring, or selecting.
    tool: Tool,
    selection: Selection,
    /// The last fragment copied *here*, so the Edit menu has something to
    /// paste — see [`SimLogixApp::copy_to_clipboard`].
    clipboard: Option<String>,
    /// Whether the language came from the Settings menu rather than the OS
    /// locale. Only a chosen one is worth storing — see `Settings`.
    language_chosen: bool,
    /// Whether dragging the canvas with the left button moves the view
    /// instead of sweeping a selection.
    ///
    /// A preference because the right answer depends on what you spend the
    /// day doing. Whichever it takes away stays one toolbar click away —
    /// `Tool::Marquee` and `Tool::Pan` each force one of the two — so this
    /// only decides which is free.
    left_drag_pans: bool,
    /// The recent projects, most recent first. Persisted with the settings.
    recent: Vec<PathBuf>,
    /// Where a rubber-band drag began, in scene coordinates, while one is in
    /// progress. View state: never saved, never part of an undo step.
    band_origin: Option<egui::Pos2>,
    /// A wire currently being placed click by click, if one is in progress.
    wiring_from: Option<WireInProgress>,
    /// Every wire the user has drawn (or that was reconstructed on project
    /// load) — the source of truth for both rendering and editing.
    wires: Vec<Wire>,
    /// Monotonically increasing, so each `Wire` gets a stable id independent
    /// of its position in `wires` (which changes on deletion).
    next_wire_id: u64,
    /// Where this circuit was last saved to or loaded from. `None` means it
    /// has never been written anywhere, so "Save" has to ask for a path the
    /// same as "Save As".
    current_path: Option<std::path::PathBuf>,
    /// Whether the circuit has edits since the last save — drives both the
    /// `*` in the window title and the confirmation before anything that
    /// would discard them.
    dirty: bool,
    /// A destructive action waiting on that confirmation, if one is open.
    pending_action: Option<PendingAction>,
    /// Document states to step back to, oldest first — see
    /// [`SimLogixApp::record_edit`]. Snapshots are `SavedProject`s, the same
    /// thing save/load uses, so undo can't drift out of sync with what a
    /// circuit is made of.
    undo_stack: Vec<SavedProject>,
    /// States undone away from, ready to step forward into. Cleared by any
    /// fresh edit, since that makes the old forward path meaningless.
    redo_stack: Vec<SavedProject>,
    /// The last title pushed to the window manager, so the viewport command
    /// is only sent when it actually changes rather than every frame.
    window_title: String,
    /// The last save/load failure, if any, shown in a dismissible window.
    error: Option<String>,
    /// Fractional logical ticks owed to `circuit` from real elapsed time,
    /// carried between frames so `TICKS_PER_SECOND` isn't rounded away.
    tick_budget: f32,
    /// The UI's current language, overridable from the Settings menu.
    language: Language,
    /// A component that has just been placed or dropped, waiting to be
    /// checked against loose wire ends. Deliberately handled a frame late:
    /// pin positions are computed while drawing, so on the frame a drag ends
    /// they still describe where the component was before it snapped.
    pending_attach: Option<ComponentId>,
    /// Whether the simulation is advancing. Editing still works while
    /// paused — only time stops.
    running: bool,
    /// The net that refused to settle, if the engine reported one. Set only
    /// alongside `running = false`: a fault pauses rather than crashing or
    /// silently looping, and stays on screen until the user resumes.
    unstable_net: Option<NetId>,
    /// A hash of what is connected to what, so the nets are only recomputed
    /// when the drawing's *connectivity* actually changed — not on every
    /// frame, and not while a waypoint is merely being dragged.
    net_fingerprint: u64,
    /// What other projects will refer to this one's circuits by — see
    /// `SavedProject::library`. Empty until the project is first saved or
    /// opened, at which point it's named after its file *once*, and stops
    /// following it thereafter.
    library: String,
    /// The folders circuits can be filed in — see `SavedProject::folders`.
    /// Held explicitly so an empty one survives.
    folders: Vec<String>,
    /// Every circuit in the project, in the order the tree lists them.
    ///
    /// The one being edited is `circuits[active]`, and *its* live state is
    /// the flat fields above (`circuit`, `placed`, `wires`, ...) — the entry
    /// here is only refreshed on the way out, by `to_project`. The others
    /// are held purely in their saved form: they aren't simulated and have
    /// no engine of their own until they're opened.
    circuits: Vec<SavedCircuit>,
    /// Which of `circuits` is open. Always a valid index.
    active: usize,
    /// Which way the component queued for placement is facing.
    ///
    /// Reset when a different kind is picked, kept while the same one is
    /// being dropped repeatedly: having turned a gate once, the next one you
    /// place almost certainly wants the same orientation.
    place_rotation: canvas::Rotation,
    /// Set when the view should be re-framed on what the open circuit
    /// actually contains: opening a project, or switching circuits.
    ///
    /// Deliberately *not* set by undo. Stepping back through your own edits
    /// must not move you somewhere else — the camera is view state, and
    /// that is exactly why `reopen` preserves it.
    refit_view: bool,
    /// Set for the one frame after the open circuit changes, so the tree
    /// scrolls it into view. Without it, adding a circuit to a list longer
    /// than the panel leaves the new one below the fold — it *is* open, but
    /// nothing on screen says so.
    reveal_active: bool,
    /// Which circuits are being flattened right now, innermost last. Only
    /// ever non-empty inside [`SimLogixApp::flatten`], where it is the
    /// guard against a circuit that contains itself.
    flattening: Vec<String>,
    /// What is being renamed and the name as typed so far, if any.
    renaming: Option<(RenameTarget, String)>,
    /// The region of the circuit currently framed by the canvas, in scene
    /// coordinates — the whole of the zoom/pan state. `egui::Scene` derives
    /// its transform from this and writes back whatever the user pans or
    /// zooms to. Everything else (component centres, waypoints) is stored
    /// in scene coordinates too, so nothing else has to know about zoom.
    scene_rect: egui::Rect,
    /// Which side of the circuit is on the canvas — its schematic, or the
    /// symbol it shows when used inside another circuit.
    view: toolbar::View,
    /// What a click does in the appearance view.
    shape_tool: toolbar::ShapeTool,
    /// What a click does while watching a circuit run.
    sim_tool: toolbar::SimTool,
    /// What is picked in the appearance view.
    symbol_selection: SymbolSelection,
    /// The polyline being drawn click by click, in symbol coordinates.
    drawing: Option<Vec<(f32, f32)>>,
    /// Where a drag in the appearance view began, in symbol coordinates —
    /// the far corner of a rectangle, or the centre of a circle.
    shape_drag: Option<(f32, f32)>,
    /// Where a selection sweep in the appearance view began, in scene
    /// coordinates.
    shape_band: Option<egui::Pos2>,
    /// Where the pointer was last seen while a shape is being moved, in
    /// *scene* coordinates.
    ///
    /// Not the raw pointer delta: that one is in screen pixels, so a zoomed
    /// view moved the shape by the wrong amount and it drifted away from the
    /// cursor. Being `Some` also means the move began with a press on the
    /// canvas, which is what stops a drag on the panel's size slider from
    /// dragging the shape along with it.
    ///
    /// The flag is whether it has actually moved yet, as opposed to merely
    /// being held: a click that never moves must not snapshot for undo, and
    /// must not snap anything on release either.
    moving_shape: Option<(egui::Pos2, bool)>,
    /// Canvas coordinates to screen coordinates, as of the last frame.
    ///
    /// Test-only, and deliberately absent otherwise: nothing in the running
    /// application asks where a canvas position is on screen, and carrying a
    /// field that nobody reads is how dead state accumulates. The unit tests
    /// compile the same crate, so what they exercise is the real thing with
    /// one extra assignment.
    #[cfg(test)]
    canvas_to_screen: egui::emath::TSTransform,
    /// The layer the canvas was drawn into last frame.
    ///
    /// Test-only for the same reason as `canvas_to_screen`: the running
    /// application never asks, and the one test that does needs it to work
    /// out where the labels sit in egui's ordering.
    #[cfg(test)]
    canvas_layer: Option<egui::LayerId>,
    /// The camera belonging to the view that *isn't* showing. Swapped on
    /// switch: the two views look at unrelated places, so carrying one
    /// camera between them would drop you somewhere arbitrary.
    idle_scene_rect: egui::Rect,
}

impl Default for SimLogixApp {
    fn default() -> Self {
        Self {
            show_about: false,
            show_signal_state: true,
            show_shortcuts: false,
            licenses: crate::licenses::State::default(),
            circuit: Circuit::default(),
            placed: Vec::new(),
            tool: Tool::default(),
            selection: Selection::default(),
            clipboard: None,
            language_chosen: false,
            left_drag_pans: false,
            recent: Vec::new(),
            band_origin: None,
            wiring_from: None,
            wires: Vec::new(),
            next_wire_id: 0,
            current_path: None,
            dirty: false,
            pending_action: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            window_title: String::new(),
            error: None,
            tick_budget: 0.0,
            // Everything else defaults trivially; only the language needs a
            // real default, detected once from the OS locale at startup.
            language: Language::detect_from_os(),
            pending_attach: None,
            net_fingerprint: 0,
            running: true,
            unstable_net: None,
            library: String::new(),
            // A project always has at least one circuit: there has to be
            // something to edit, and the tree has to have something to show.
            circuits: vec![SavedCircuit {
                name: "main".to_string(),
                folder: String::new(),
                components: Vec::new(),
                wires: Vec::new(),
                appearance: None,
            }],
            folders: Vec::new(),
            active: 0,
            place_rotation: canvas::Rotation::default(),
            refit_view: false,
            reveal_active: false,
            flattening: Vec::new(),
            renaming: None,
            // Empty means "not framed yet" — the first frame sets it to the
            // canvas's own size, which is the only value that gives a zoom
            // of exactly 1. See where it's filled in.
            scene_rect: egui::Rect::ZERO,
            #[cfg(test)]
            canvas_to_screen: egui::emath::TSTransform::IDENTITY,
            #[cfg(test)]
            canvas_layer: None,
            view: toolbar::View::default(),
            shape_tool: toolbar::ShapeTool::default(),
            sim_tool: toolbar::SimTool::default(),
            symbol_selection: SymbolSelection::default(),
            drawing: None,
            shape_drag: None,
            shape_band: None,
            moving_shape: None,
            idle_scene_rect: egui::Rect::ZERO,
        }
    }
}

impl SimLogixApp {
    /// Registers a new component of `kind` in `circuit` and adds it to
    /// `placed` at `center`. Returns its id — used both for interactive
    /// placement and to rebuild a saved project (see `project.rs`).
    fn place(&mut self, kind: ComponentKind, center: egui::Pos2) -> ComponentId {
        let placed = match kind {
            ComponentKind::Button => {
                let net = self.circuit.add_net();
                let (button, pressed) = Button::new();
                let id = self.circuit.add_component(
                    Box::new(button),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::button(id, center, pressed)
            }
            ComponentKind::Switch => {
                let net = self.circuit.add_net();
                // The same engine component a button uses: at that level
                // they are both "a source whose level the GUI owns".
                let (switch, on) = Button::new();
                let id = self.circuit.add_component(
                    Box::new(switch),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::switch(id, center, on)
            }
            ComponentKind::Led => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Led),
                    vec![Pin {
                        direction: PinDirection::Input,
                        net,
                    }],
                );
                PlacedComponent::led(id, center)
            }
            ComponentKind::NTransistor | ComponentKind::PTransistor => {
                let gate = self.circuit.add_net();
                let source = self.circuit.add_net();
                let drain = self.circuit.add_net();
                let transistor = if kind == ComponentKind::NTransistor {
                    Transistor::n_type()
                } else {
                    Transistor::p_type()
                };
                let id = self.circuit.add_component(
                    Box::new(transistor),
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: gate,
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: source,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: drain,
                        },
                    ],
                );
                PlacedComponent::transistor(id, center, kind)
            }
            ComponentKind::Ground | ComponentKind::Power => {
                let net = self.circuit.add_net();
                let rail = if kind == ComponentKind::Ground {
                    Rail::ground()
                } else {
                    Rail::power()
                };
                let id = self.circuit.add_component(
                    Box::new(rail),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::rail(id, center, kind)
            }
            ComponentKind::Probe => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Probe),
                    vec![Pin {
                        direction: PinDirection::Input,
                        net,
                    }],
                );
                PlacedComponent::probe(id, center)
            }
            ComponentKind::Clock => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Clock::new()),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                // Periodic, not schedule_now: a Clock must keep re-triggering
                // itself forever (see Circuit::schedule_periodic), which the
                // per-frame advance() call in `ui()` then processes over time.
                self.circuit.schedule_periodic(id, CLOCK_PERIOD_TICKS);
                PlacedComponent::clock(id, center)
            }
            // Every 2-input combinational gate shares this shape (see
            // `PlacedComponent::TwoInputGate`'s doc comment).
            ComponentKind::And
            | ComponentKind::Or
            | ComponentKind::Nand
            | ComponentKind::Nor
            | ComponentKind::Xor
            | ComponentKind::Xnor => {
                let a = self.circuit.add_net();
                let b = self.circuit.add_net();
                let out = self.circuit.add_net();
                let component: Box<dyn Component> = if kind == ComponentKind::And {
                    Box::new(And)
                } else if kind == ComponentKind::Or {
                    Box::new(Or)
                } else if kind == ComponentKind::Nand {
                    Box::new(Nand)
                } else if kind == ComponentKind::Nor {
                    Box::new(Nor)
                } else if kind == ComponentKind::Xor {
                    Box::new(Xor)
                } else {
                    Box::new(Xnor)
                };
                let id = self.circuit.add_component(
                    component,
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: a,
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: b,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: out,
                        },
                    ],
                );
                PlacedComponent::two_input_gate(id, center, kind)
            }
            // The 1-input mirror of the above (see
            // `PlacedComponent::OneInputGate`'s doc comment).
            ComponentKind::BusTransceiver | ComponentKind::BusTransceiverOe => {
                // A and B are `InOut`: each reads the bus it sits on and
                // drives it only when the direction says to.
                let nets: Vec<_> = (0..4).map(|_| self.circuit.add_net()).collect();
                let part = if kind == ComponentKind::BusTransceiverOe {
                    BusTransceiver::active_low()
                } else {
                    BusTransceiver::active_high()
                };
                let id = self.circuit.add_component(
                    Box::new(part),
                    vec![
                        Pin {
                            direction: PinDirection::InOut,
                            net: nets[0],
                        },
                        Pin {
                            direction: PinDirection::InOut,
                            net: nets[1],
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: nets[2],
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: nets[3],
                        },
                    ],
                );
                PlacedComponent::bus_transceiver(id, center, kind)
            }
            ComponentKind::TriStateBuffer => {
                let data = self.circuit.add_net();
                let enable = self.circuit.add_net();
                let out = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(TriStateBuffer),
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: data,
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: enable,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: out,
                        },
                    ],
                );
                // Two inputs at 0/1 and one output at 2 — the exact shape
                // `TwoInputGate` already draws and wires generically, so it
                // needs no variant of its own.
                PlacedComponent::two_input_gate(id, center, kind)
            }
            ComponentKind::InputPort | ComponentKind::OutputPort | ComponentKind::InOutPort => {
                let net = self.circuit.add_net();
                // The pin points the way the value crosses the boundary: an
                // input drives the internal net, an output reads it, and a
                // bidirectional port does both.
                let (component, direction, level): (Box<dyn Component>, _, _) = match kind {
                    ComponentKind::InputPort => {
                        let (port, level) = CircuitPort::input();
                        (Box::new(port), PinDirection::Output, Some(level))
                    }
                    ComponentKind::OutputPort => {
                        (Box::new(CircuitOutput), PinDirection::Input, None)
                    }
                    _ => {
                        let (port, level) = CircuitPort::bidirectional();
                        (Box::new(port), PinDirection::InOut, Some(level))
                    }
                };
                let id = self
                    .circuit
                    .add_component(component, vec![Pin { direction, net }]);
                self.circuit.schedule_now(id);
                PlacedComponent::port(id, center, kind, level)
            }
            ComponentKind::Circuit(path) => {
                // Refusing (a missing circuit, or one that contains itself)
                // still places the box, empty: an instance you can see and
                // delete beats a click that silently does nothing, and
                // `flatten` has already said why in the status window.
                let (ports, inner_groups) = self.flatten(&path).unwrap_or_default();
                let pins = ports
                    .iter()
                    .map(|_| Pin {
                        // `InOut` whatever the port's own direction: the
                        // anchor drives nothing, and its pin has to be able
                        // to both carry a value in and read one back out.
                        direction: PinDirection::InOut,
                        net: self.circuit.add_net(),
                    })
                    .collect();
                let id = self
                    .circuit
                    .add_component(Box::new(CircuitAnchor::new(ports.len())), pins);
                self.circuit.schedule_now(id);
                let appearance = self.appearance_of(&path, &ports);
                PlacedComponent::instance(id, center, path, ports, inner_groups, appearance)
            }
            ComponentKind::SrLatch => {
                let nets = [
                    self.circuit.add_net(),
                    self.circuit.add_net(),
                    self.circuit.add_net(),
                    self.circuit.add_net(),
                ];
                let id = self.circuit.add_component(
                    Box::new(SrLatch::new()),
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: nets[0],
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: nets[1],
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: nets[2],
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: nets[3],
                        },
                    ],
                );
                PlacedComponent::sr_latch(id, center)
            }
            ComponentKind::Not | ComponentKind::Buffer => {
                let input = self.circuit.add_net();
                let output = self.circuit.add_net();
                let component: Box<dyn Component> = if kind == ComponentKind::Not {
                    Box::new(Not)
                } else {
                    Box::new(Buffer)
                };
                let id = self.circuit.add_component(
                    component,
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: input,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: output,
                        },
                    ],
                );
                PlacedComponent::one_input_gate(id, center, kind)
            }
        };
        // Process just this component's own initial schedule (if any) --
        // never circuit.run(), which would let a just-placed Clock's endless
        // self-reschedule burn through a huge chunk of its future ticks
        // instantly instead of over real time.
        let _ = self.circuit.advance(0);
        let id = placed.id();
        self.placed.push(placed);
        id
    }

    /// Registers a new `Wire` and returns its id.
    fn add_wire(
        &mut self,
        from: WireEndpoint,
        to: WireEndpoint,
        waypoints: Vec<egui::Pos2>,
    ) -> u64 {
        let id = self.next_wire_id;
        self.next_wire_id += 1;
        self.wires.push(Wire {
            id,
            from,
            to,
            waypoints,
            // Left unset: a wire joined to a coloured net picks the colour
            // up on the next rebuild (`inherit_wire_colors`).
            color: None,
        });
        id
    }

    /// The net a wire currently carries, read back from its endpoints rather
    /// than cached on the wire. `rebuild_nets` reallocates every net from
    /// scratch after each edit, so any stored copy would go stale as soon as
    /// something elsewhere in the drawing changed — which is exactly how
    /// wires ended up drawn in the wrong signal colour before.
    ///
    /// Either end may be the one that reaches a pin (a wire can be left
    /// dangling at the other), so both are tried. `None` means neither does:
    /// the wire is pure drawing at the moment and carries nothing.
    fn wire_net(&self, wire: &Wire) -> Option<NetId> {
        self.endpoint_net(wire.from, &mut Vec::new())
            .or_else(|| self.endpoint_net(wire.to, &mut Vec::new()))
    }

    /// The net behind a single endpoint, following a junction to its host.
    /// `visited` guards against a tap cycle, which nothing creates on
    /// purpose but which mustn't be able to hang the UI either.
    fn endpoint_net(&self, endpoint: WireEndpoint, visited: &mut Vec<u64>) -> Option<NetId> {
        match endpoint {
            WireEndpoint::Pin(component, pin_index) => self
                .circuit
                .try_pins(component)
                .and_then(|pins| pins.get(pin_index))
                .map(|pin| pin.net),
            WireEndpoint::Junction { wire, .. } => {
                if visited.contains(&wire) {
                    return None;
                }
                visited.push(wire);
                let host = self.wires.iter().find(|w| w.id == wire)?;
                self.endpoint_net(host.from, visited)
                    .or_else(|| self.endpoint_net(host.to, visited))
            }
            WireEndpoint::Free(_) => None,
        }
    }

    /// Snapshots the open circuit's layout and wiring. Runtime state
    /// (button presses, signal values) is deliberately left out; see
    /// `project.rs`.
    fn active_circuit(&self) -> SavedCircuit {
        let components = self
            .placed
            .iter()
            .map(|placed| {
                let center = placed.center();
                SavedComponent {
                    kind: placed.kind(),
                    x: center.x,
                    y: center.y,
                    rotation: placed.rotation(),
                    properties: placed.properties().clone(),
                }
            })
            .collect();

        // Wires reference components and each other by index in the saved
        // lists, not by runtime id — ids aren't stable across a reload.
        let component_index: HashMap<ComponentId, usize> = self
            .placed
            .iter()
            .enumerate()
            .map(|(index, placed)| (placed.id(), index))
            .collect();
        let wire_index: HashMap<u64, usize> = self
            .wires
            .iter()
            .enumerate()
            .map(|(index, wire)| (wire.id, index))
            .collect();

        let wires = self
            .wires
            .iter()
            .filter_map(|wire| {
                let save = |endpoint: WireEndpoint| match endpoint {
                    WireEndpoint::Pin(component, pin) => {
                        Some(SavedEndpoint::Pin(*component_index.get(&component)?, pin))
                    }
                    WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    } => Some(SavedEndpoint::Junction {
                        wire: *wire_index.get(&host)?,
                        waypoint,
                    }),
                    WireEndpoint::Free(pos) => Some(SavedEndpoint::Free(pos.x, pos.y)),
                };
                Some(SavedWire {
                    color: wire.color,
                    from: save(wire.from)?,
                    to: save(wire.to)?,
                    waypoints: wire.waypoints.iter().map(|p| (p.x, p.y)).collect(),
                })
            })
            .collect();

        SavedCircuit {
            name: self.circuits[self.active].name.clone(),
            folder: self.circuits[self.active].folder.clone(),
            components,
            wires,
            // Carried across untouched: the flat live state is the *schematic*
            // of the open circuit, and its symbol isn't part of that.
            appearance: self.circuits[self.active].appearance.clone(),
        }
    }

    /// The whole project as it stands: every circuit, with the open one
    /// refreshed from its live state. This is both what gets written to
    /// disk and what an undo step is made of — the closed circuits ride
    /// along in their saved form, so an edit to one circuit never quietly
    /// drops the others from the file or from the history.
    fn to_project(&self) -> SavedProject {
        let mut circuits = self.circuits.clone();
        circuits[self.active] = self.active_circuit();

        SavedProject {
            version: crate::project::CURRENT_VERSION,
            library: self.library.clone(),
            folders: self.folders.clone(),
            circuits,
        }
    }

    /// Rebuilds a fresh app from a saved project, opening `open` for
    /// editing: re-registers each of that circuit's components, then
    /// replays its wires and their routes. The rest of the project is kept
    /// in saved form, ready to be opened in turn.
    ///
    /// `open` is clamped rather than trusted — it can come from a snapshot
    /// taken when the project had more circuits in it.
    fn from_project(project: &SavedProject, open: usize) -> Self {
        let mut app = Self::default();

        app.library.clone_from(&project.library);
        app.folders.clone_from(&project.folders);
        if !project.circuits.is_empty() {
            app.circuits = project.circuits.clone();
            app.active = open.min(app.circuits.len() - 1);
        }
        let Some(circuit) = project.circuits.get(app.active) else {
            return app;
        };

        let ids: Vec<ComponentId> = circuit
            .components
            .iter()
            .map(|saved| {
                let id = app.place(saved.kind.clone(), egui::pos2(saved.x, saved.y));
                if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == id) {
                    placed.set_rotation(saved.rotation);
                    placed.set_properties(saved.properties.clone());
                }
                id
            })
            .collect();

        // Saved wire index -> runtime id, so a junction can resolve the wire
        // it taps. Saved order is preserved and a junction only ever refers
        // to an earlier wire, so this fills in before it's needed.
        let mut wire_ids: Vec<u64> = Vec::with_capacity(circuit.wires.len());
        for saved in &circuit.wires {
            let load = |endpoint: &SavedEndpoint| match *endpoint {
                SavedEndpoint::Pin(component_index, pin_index) => ids
                    .get(component_index)
                    .map(|&id| WireEndpoint::Pin(id, pin_index)),
                SavedEndpoint::Junction { wire, waypoint } => {
                    wire_ids.get(wire).map(|&host| WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    })
                }
                SavedEndpoint::Free(x, y) => Some(WireEndpoint::Free(egui::pos2(x, y))),
            };
            let (Some(from), Some(to)) = (load(&saved.from), load(&saved.to)) else {
                continue;
            };

            let waypoints = saved
                .waypoints
                .iter()
                .map(|&(x, y)| egui::pos2(x, y))
                .collect();
            let id = app.add_wire(from, to, waypoints);
            if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == id) {
                wire.color = saved.color;
            }
            wire_ids.push(id);
        }

        // The wires are the record of what's connected; the nets come from
        // them, exactly as they do after any edit.
        app.rebuild_nets();
        app.net_fingerprint = app.connectivity_fingerprint();
        let _ = app.circuit.advance(SETTLE_TICKS);

        app
    }

    /// Saves to the current path if there is one, otherwise falls back to
    /// [`Self::save_project_as`]. Returns whether the circuit actually made
    /// it to disk — the confirmation flow needs to know, so that cancelling
    /// the dialog doesn't silently go on to discard the work anyway.
    fn save_project(&mut self) -> bool {
        match self.current_path.clone() {
            Some(path) => self.write_project_to(&path),
            None => self.save_project_as(),
        }
    }

    fn save_project_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SimLogix project", &[project::PROJECT_EXTENSION])
            .set_file_name(format!("circuit.{}", project::PROJECT_EXTENSION))
            .save_file()
        else {
            return false;
        };
        self.write_project_to(&path)
    }

    /// Names the project after its file, the first time it has one.
    ///
    /// The library name has to start as *something*, and the file name is
    /// both the only thing to hand and what the user already thinks of as
    /// the project's name. It stops following the file from here on: that's
    /// the whole point of storing it, so renaming the file doesn't repoint
    /// every reference another project makes to these circuits.
    fn name_library_after(&mut self, path: &std::path::Path) {
        if !self.library.is_empty() {
            return;
        }
        if let Some(stem) = path.file_stem() {
            self.library = stem.to_string_lossy().into_owned();
        }
    }

    /// Puts every preference in this menu back where it started.
    ///
    /// Only what the Settings menu itself offers. The panel sizes and the
    /// window geometry are persisted too, by egui rather than by us, and a
    /// button labelled "settings" rearranging the window would be a
    /// surprise — that belongs to a separate "reset the layout" if it's ever
    /// wanted.
    fn reset_settings(&mut self, ctx: &egui::Context) {
        self.left_drag_pans = false;
        // Back to *following* the OS locale, not to a fixed language:
        // clearing the choice is the reset, and re-detecting is what that
        // means the next time the machine's locale differs.
        self.language_chosen = false;
        self.language = Language::detect_from_os();
        ctx.set_theme(egui::ThemePreference::System);
    }

    /// The area the drawing occupies, or `None` when there's nothing in it.
    ///
    /// Components contribute their own box — an instance is taller than one
    /// grid cell — and wires contribute the points that don't follow a pin,
    /// since those can sit well outside every component.
    /// Puts the other side of the circuit on the canvas.
    ///
    /// The two views look at unrelated places — a schematic sits wherever it
    /// was drawn, a symbol always sits on the origin — so each keeps its own
    /// camera rather than one being carried across and landing nowhere.
    fn switch_view(&mut self, view: toolbar::View) {
        if view == self.view {
            return;
        }
        self.view = view;
        std::mem::swap(&mut self.scene_rect, &mut self.idle_scene_rect);
        // Never framed before: `ui()` reads a zero rect as "frame me".
        if self.scene_rect.width() <= 0.0 {
            self.refit_view = true;
        }
        // Shape indices belong to whichever symbol was showing.
        self.symbol_selection = SymbolSelection::default();
        self.drawing = None;
        self.shape_drag = None;
        self.shape_band = None;
        self.moving_shape = None;
        // Neither drawing a symbol nor watching a circuit has anything to
        // place, wire or select with.
        if view != toolbar::View::Schematic {
            self.tool = toolbar::Tool::Select;
            self.wiring_from = None;
        }
    }

    /// The symbol the open circuit is currently showing: its own if it has
    /// been given one, and the generated box otherwise.
    ///
    /// Read through `active_circuit()` so the port order is the one
    /// `port_slots` produces everywhere else — the open circuit's entry in
    /// `circuits` is stale by construction, and a second ordering rule here
    /// is exactly how the symbol and the instance would come to disagree.
    fn active_appearance(&self) -> (Vec<InstancePort>, Appearance) {
        let saved = self.active_circuit();
        let ports: Vec<InstancePort> = Self::port_slots(&saved)
            .into_iter()
            .map(|(_, port)| port)
            .collect();
        let appearance = self.circuits[self.active]
            .appearance
            .clone()
            .unwrap_or_else(|| Appearance::generated(&ports));
        (ports, appearance)
    }

    /// The appearance view: the open circuit's symbol, with a handle on
    /// every pin.
    ///
    /// The symbol is drawn on the origin because it has no position of its
    /// own — where it ends up on a canvas is the instance's business.
    fn appearance_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
    ) {
        let (ports, mut appearance) = self.active_appearance();
        let center = egui::Pos2::ZERO;
        let names: Vec<&str> = ports.iter().map(|port| port.name.as_str()).collect();
        let pin_positions = appearance.draw(
            painter,
            center,
            canvas::Rotation::Deg0,
            ui.visuals().strong_text_color(),
            &names,
            &crate::symbol::TextLayer::for_ui(ui),
        );

        // Drawn here too, and not only on an instance: a checkbox whose
        // effect you can't see while ticking it is a checkbox you have to
        // guess at.
        if appearance.show_name {
            crate::symbol::TextLayer::for_ui(ui).text(
                appearance.name_anchor(center),
                egui::Align2::CENTER_BOTTOM,
                &self.circuits[self.active].name,
                10.0,
                ui.visuals().strong_text_color(),
            );
        }

        // A circuit with no ports has a symbol with no pins — worth saying,
        // since the box looks finished and simply can't connect to anything.
        if ports.is_empty() {
            let strings = Strings::for_language(self.language);
            crate::symbol::TextLayer::for_ui(ui).text(
                egui::pos2(center.x, appearance.rect(center).bottom() + 12.0),
                egui::Align2::CENTER_TOP,
                strings.appearance_no_ports,
                10.0,
                ui.visuals().weak_text_color(),
            );
        }

        let accent = canvas::accent_color(ui.visuals().dark_mode);

        // Everything picked is lit in the accent colour: a shape redrawn over
        // itself, a pin ringed. A symbol is line art, and a box around a
        // diagonal line says less than the line lit up.
        for &index in &self.symbol_selection.shapes {
            let path = appearance.shape_path(index, center);
            if path.len() > 1 {
                painter.line(path, egui::Stroke::new(3.0, accent));
            }
        }
        for &index in &self.symbol_selection.pins {
            if let Some(at) = pin_positions.get(index) {
                painter.circle_stroke(*at, 6.0, egui::Stroke::new(2.0, accent));
            }
        }

        let changed = self.appearance_edit(
            ui,
            painter,
            pointer,
            center,
            &pin_positions,
            &mut appearance,
            accent,
        );

        if changed {
            // The first edit is also what turns the generated box into a
            // symbol of this circuit's own — there is nothing else to store,
            // and nothing was lost, since it is the very box that was on
            // screen a frame ago.
            self.circuits[self.active].appearance = Some(appearance);
        }
    }

    /// Drawing, picking, moving and deleting what a symbol is made of.
    /// Reports whether the symbol changed.
    ///
    /// **No widget is allocated here.** Press and release are read straight
    /// off the pointer, because a full-canvas `ui.interact` would cover the
    /// `Scene`'s own background response — which is what panning goes
    /// through. That is exactly how the rubber band once broke placement and
    /// panning at once, and the fix then was the same: add no widget.
    #[allow(clippy::too_many_arguments)]
    fn appearance_edit(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
        center: egui::Pos2,
        pin_positions: &[egui::Pos2],
        appearance: &mut Appearance,
        accent: egui::Color32,
    ) -> bool {
        let (pressed, released, double_clicked, shift) = ui.ctx().input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
            )
        });
        // Snapped in symbol coordinates, which are centre-relative — the
        // same space the shapes themselves are stored in.
        let aimed = pointer.map(|at| {
            let snap = |v: f32| {
                (v / crate::appearance::SHAPE_SNAP).round() * crate::appearance::SHAPE_SNAP
            };
            (snap(at.x - center.x), snap(at.y - center.y))
        });
        let preview = egui::Stroke::new(1.6, accent.gamma_multiply(0.7));
        let world = |(x, y): (f32, f32)| egui::pos2(center.x + x, center.y + y);
        let typing = ui.ctx().text_edit_focused();
        let mut changed = false;

        // Escape backs out of a shape in progress first, then the selection —
        // one step at a time, as it does on the schematic.
        if !typing && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.drawing.take().is_none() {
                self.symbol_selection = SymbolSelection::default();
            }
            self.shape_drag = None;
        }

        changed |= self.appearance_keys(ui, appearance, typing);

        match self.shape_tool {
            toolbar::ShapeTool::Pan => {}
            toolbar::ShapeTool::Select => {
                changed |= self.appearance_select(
                    ui,
                    painter,
                    pointer,
                    center,
                    pin_positions,
                    appearance,
                    accent,
                    (pressed, released, shift),
                );
            }
            toolbar::ShapeTool::Text => {
                if let (Some(at), true) = (aimed, pressed) {
                    self.record_edit();
                    let strings = Strings::for_language(self.language);
                    appearance.shapes.push(crate::appearance::Shape::Text {
                        at,
                        align: crate::appearance::TextAlign::Center,
                        size: DEFAULT_SHAPE_TEXT_SIZE,
                        // Placed with something readable on it rather than
                        // empty: an empty label draws nothing, so it would
                        // land invisible and there would be no way to tell a
                        // missed click from a placed one.
                        text: strings.shape_text_default.to_string(),
                    });
                    self.symbol_selection = SymbolSelection {
                        shapes: vec![appearance.shapes.len() - 1],
                        pins: Vec::new(),
                    };
                    // Back to Select so the label just dropped can be typed
                    // and moved straight away, instead of the next click
                    // dropping a second one.
                    self.shape_tool = toolbar::ShapeTool::Select;
                    changed = true;
                }
            }
            toolbar::ShapeTool::Line => {
                // A double-click lands as a press as well, so whether this
                // click ends the line is settled before another point can be
                // added to it.
                let finish = double_clicked
                    || (!typing && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)));

                if let Some(at) = aimed {
                    if let Some(points) = &self.drawing {
                        let mut path: Vec<egui::Pos2> = points.iter().map(|&p| world(p)).collect();
                        path.push(world(at));
                        painter.line(path, preview);
                    }
                    if pressed && !finish {
                        self.drawing.get_or_insert_with(Vec::new).push(at);
                    }
                }

                if finish {
                    if let Some(points) = self.drawing.take() {
                        // One point is a click, not a line.
                        if points.len() > 1 {
                            self.record_edit();
                            appearance.shapes.push(crate::appearance::Shape::Polyline {
                                points,
                                closed: false,
                            });
                            changed = true;
                        }
                    }
                }
            }
            toolbar::ShapeTool::Arc => {
                // Click the two ends, then move to bulge it and click again.
                // The middle point is the one being chosen in that last step,
                // which is exactly what the shape stores.
                if let Some(at) = aimed {
                    match self.drawing.as_deref() {
                        Some([start]) => {
                            painter.line(vec![world(*start), world(at)], preview);
                        }
                        Some([start, end]) => {
                            let arc = crate::appearance::Shape::Arc {
                                start: *start,
                                mid: at,
                                end: *end,
                            };
                            appearance.shapes.push(arc);
                            let index = appearance.shapes.len() - 1;
                            let path = appearance.shape_path(index, center);
                            appearance.shapes.pop();
                            painter.line(path, preview);
                        }
                        _ => {}
                    }
                    if pressed {
                        let points = self.drawing.get_or_insert_with(Vec::new);
                        points.push(at);
                        if points.len() == 3 {
                            let (start, end, mid) = (points[0], points[1], points[2]);
                            self.drawing = None;
                            self.record_edit();
                            appearance.shapes.push(crate::appearance::Shape::Arc {
                                start,
                                mid,
                                end,
                            });
                            changed = true;
                        }
                    }
                }
            }
            toolbar::ShapeTool::Rect | toolbar::ShapeTool::Circle => {
                if pressed {
                    self.shape_drag = aimed;
                }
                if let (Some(from), Some(to)) = (self.shape_drag, aimed) {
                    let shape = if self.shape_tool == toolbar::ShapeTool::Rect {
                        crate::appearance::Shape::Polyline {
                            // A rectangle is a closed four-point polyline —
                            // no shape of its own, because it isn't one.
                            points: vec![from, (to.0, from.1), to, (from.0, to.1)],
                            closed: true,
                        }
                    } else {
                        crate::appearance::Shape::Circle {
                            center: from,
                            radius: (to.0 - from.0).hypot(to.1 - from.1),
                        }
                    };
                    if released {
                        self.shape_drag = None;
                        // A click that never moved is a miss, not an empty
                        // shape nobody can see or select.
                        if from != to {
                            self.record_edit();
                            appearance.shapes.push(shape);
                            changed = true;
                        }
                    } else {
                        appearance.shapes.push(shape);
                        let index = appearance.shapes.len() - 1;
                        let path = appearance.shape_path(index, center);
                        appearance.shapes.pop();
                        painter.line(path, preview);
                    }
                }
            }
        }
        changed
    }

    /// Picking, moving and sweeping a band — everything the Select tool does.
    #[allow(clippy::too_many_arguments)]
    fn appearance_select(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
        center: egui::Pos2,
        pin_positions: &[egui::Pos2],
        appearance: &mut Appearance,
        accent: egui::Color32,
        (pressed, released, shift): (bool, bool, bool),
    ) -> bool {
        let mut changed = false;

        if let (Some(at), true) = (pointer, pressed) {
            // A pin wins a tie with a shape: its target is the smaller of
            // the two, so aiming at one is the more deliberate act.
            let pin = pin_positions
                .iter()
                .enumerate()
                .map(|(index, position)| (index, position.distance(at)))
                .filter(|(_, distance)| *distance <= APPEARANCE_PIN_HANDLE / 2.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index);
            let shape = pin.is_none().then(|| {
                (0..appearance.shapes.len())
                    .map(|index| (index, appearance.distance_to_shape(index, at, center)))
                    .filter(|(_, distance)| *distance <= SHAPE_PICK_RADIUS)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| index)
            });
            let shape = shape.flatten();

            changed |= self.drop_emptied_labels(appearance, shape);

            match (pin, shape) {
                (Some(index), _) if shift => {
                    SymbolSelection::toggle(&mut self.symbol_selection.pins, index)
                }
                (Some(index), _) => {
                    if !self.symbol_selection.pins.contains(&index) {
                        self.symbol_selection = SymbolSelection {
                            shapes: Vec::new(),
                            pins: vec![index],
                        };
                    }
                }
                (None, Some(index)) if shift => {
                    SymbolSelection::toggle(&mut self.symbol_selection.shapes, index)
                }
                (None, Some(index)) => {
                    if !self.symbol_selection.shapes.contains(&index) {
                        self.symbol_selection = SymbolSelection {
                            shapes: vec![index],
                            pins: Vec::new(),
                        };
                    }
                }
                // Nothing under the pointer: a press on empty canvas starts
                // a band, and only clears the selection if it stays a click.
                (None, None) => self.shape_band = Some(at),
            }
        }

        // Moving whatever is picked, by the pointer's position in *scene*
        // coordinates. Not by the raw pointer delta: that one is in screen
        // pixels, so a zoomed view moved things by the wrong amount and they
        // drifted away from the cursor. Requiring the pointer to be over the
        // canvas is also what stops a drag on the panel's size slider from
        // dragging the selection along with it.
        let held = ui.ctx().input(|i| i.pointer.primary_down());
        if let (Some(now), true, None) = (pointer, held, self.shape_band) {
            if !self.symbol_selection.is_empty() {
                let (from, moved_before) = *self.moving_shape.get_or_insert((now, false));
                let by = now - from;
                if by != egui::Vec2::ZERO {
                    // Snapshotted on the first movement rather than on the
                    // press: picking something up to look at it isn't an
                    // edit, and taking it here still catches the state before
                    // anything has moved, since the translation is next.
                    if !moved_before {
                        self.record_edit();
                    }
                    self.translate_selection(appearance, by);
                    self.moving_shape = Some((now, true));
                    changed = true;
                }
            }
        }

        if let (Some(origin), Some(now)) = (self.shape_band, pointer) {
            let rect = egui::Rect::from_two_pos(origin, now);
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, accent),
                egui::StrokeKind::Inside,
            );
        }

        if released {
            if let Some(origin) = self.shape_band.take() {
                let now = pointer.unwrap_or(origin);
                let rect = egui::Rect::from_two_pos(origin, now);
                if rect.width() > 1.0 || rect.height() > 1.0 {
                    // Anything the band touched at all, not only what it
                    // swallowed whole: a long line drawn across the symbol
                    // would otherwise be impossible to catch.
                    self.symbol_selection = SymbolSelection {
                        shapes: (0..appearance.shapes.len())
                            .filter(|&index| {
                                appearance
                                    .shape_path(index, center)
                                    .iter()
                                    .any(|point| rect.contains(*point))
                            })
                            .collect(),
                        pins: pin_positions
                            .iter()
                            .enumerate()
                            .filter(|(_, at)| rect.contains(**at))
                            .map(|(index, _)| index)
                            .collect(),
                    };
                } else if !shift {
                    // A press that never became a sweep is a click on empty
                    // canvas, which is how you deselect.
                    self.symbol_selection = SymbolSelection::default();
                }
            }
            if matches!(self.moving_shape, Some((_, true))) {
                self.snap_selection(appearance);
                changed = true;
            }
            self.moving_shape = None;
        }

        changed
    }

    /// Keyboard: nudging, deleting, and copy/paste of shapes.
    fn appearance_keys(
        &mut self,
        ui: &mut egui::Ui,
        appearance: &mut Appearance,
        typing: bool,
    ) -> bool {
        if typing {
            return false;
        }
        let mut changed = false;

        // Arrow keys move by one step of whatever the thing snaps to, so a
        // nudge lands where a drag would have — the point of having them is
        // to place something exactly, not to place it slightly off.
        let nudge = ui.ctx().input(|i| {
            let mut by = egui::Vec2::ZERO;
            for (key, step) in [
                (egui::Key::ArrowLeft, egui::vec2(-1.0, 0.0)),
                (egui::Key::ArrowRight, egui::vec2(1.0, 0.0)),
                (egui::Key::ArrowUp, egui::vec2(0.0, -1.0)),
                (egui::Key::ArrowDown, egui::vec2(0.0, 1.0)),
            ] {
                if i.key_pressed(key) {
                    by += step;
                }
            }
            by
        });
        if nudge != egui::Vec2::ZERO && !self.symbol_selection.is_empty() {
            self.record_edit();
            for &index in &self.symbol_selection.shapes {
                appearance.translate_shape(index, nudge * crate::appearance::SHAPE_SNAP);
            }
            for &index in &self.symbol_selection.pins {
                if let Some(pin) = appearance.pins.get_mut(index) {
                    let by = nudge * canvas::GRID_SPACING;
                    pin.at = (pin.at.0 + by.x, pin.at.1 + by.y);
                }
            }
            changed = true;
        }

        if ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && !self.symbol_selection.shapes.is_empty()
        {
            self.record_edit();
            // Removed from the back so the earlier indices stay valid.
            let mut doomed = self.symbol_selection.shapes.clone();
            doomed.sort_unstable();
            for index in doomed.into_iter().rev() {
                if index < appearance.shapes.len() {
                    appearance.shapes.remove(index);
                }
            }
            self.symbol_selection = SymbolSelection::default();
            changed = true;
        }

        // Only shapes travel. A pin belongs to a port, so there is nothing
        // for a copy of one to be a pin *of*.
        let (copy, paste) = ui.ctx().input(|i| {
            let mut copy = false;
            let mut paste = None;
            for event in &i.events {
                match event {
                    egui::Event::Copy | egui::Event::Cut => copy = true,
                    egui::Event::Paste(text) => paste = Some(text.clone()),
                    _ => {}
                }
            }
            (copy, paste)
        });
        if copy && !self.symbol_selection.shapes.is_empty() {
            let picked: Vec<&crate::appearance::Shape> = self
                .symbol_selection
                .shapes
                .iter()
                .filter_map(|&index| appearance.shapes.get(index))
                .collect();
            if let Ok(json) = serde_json::to_string(&picked) {
                ui.ctx().copy_text(format!("{SYMBOL_CLIPBOARD_TAG}{json}"));
            }
        }
        if let Some(text) = paste {
            if let Some(json) = text.strip_prefix(SYMBOL_CLIPBOARD_TAG) {
                if let Ok(shapes) = serde_json::from_str::<Vec<crate::appearance::Shape>>(json) {
                    if !shapes.is_empty() {
                        self.record_edit();
                        let first = appearance.shapes.len();
                        appearance.shapes.extend(shapes);
                        // Offset so the copy is visibly its own thing rather
                        // than hidden exactly beneath the original.
                        for index in first..appearance.shapes.len() {
                            appearance.translate_shape(
                                index,
                                egui::Vec2::splat(crate::appearance::SHAPE_SNAP * 2.0),
                            );
                        }
                        self.symbol_selection = SymbolSelection {
                            shapes: (first..appearance.shapes.len()).collect(),
                            pins: Vec::new(),
                        };
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    /// Moves everything picked by the same amount.
    fn translate_selection(&self, appearance: &mut Appearance, by: egui::Vec2) {
        for &index in &self.symbol_selection.shapes {
            appearance.translate_shape(index, by);
        }
        for &index in &self.symbol_selection.pins {
            if let Some(pin) = appearance.pins.get_mut(index) {
                pin.at = (pin.at.0 + by.x, pin.at.1 + by.y);
            }
        }
    }

    /// Puts everything picked back on its own step, once a drag has ended.
    ///
    /// A pin snaps to the *whole* grid and a shape to a quarter of it: a pin
    /// is what a wire attaches to and has to land on a dot.
    fn snap_selection(&self, appearance: &mut Appearance) {
        for &index in &self.symbol_selection.shapes {
            appearance.snap_shape(index);
        }
        for &index in &self.symbol_selection.pins {
            if let Some(pin) = appearance.pins.get_mut(index) {
                let snapped = canvas::snap_to_grid(egui::pos2(pin.at.0, pin.at.1));
                pin.at = (snapped.x, snapped.y);
            }
        }
    }

    /// Drops any selected label whose text was cleared, now that the click
    /// has moved on from it.
    ///
    /// An empty label draws nothing, so leaving it would put an invisible
    /// shape on the symbol that you'd have to remember the position of to
    /// ever select again — and moving on is the point at which "I didn't
    /// want it" is actually settled.
    fn drop_emptied_labels(&mut self, appearance: &mut Appearance, keeping: Option<usize>) -> bool {
        let emptied: Vec<usize> = self
            .symbol_selection
            .shapes
            .iter()
            .copied()
            .filter(|index| Some(*index) != keeping)
            .filter(|index| {
                matches!(
                    appearance.shapes.get(*index),
                    Some(crate::appearance::Shape::Text { text, .. }) if text.trim().is_empty()
                )
            })
            .collect();
        if emptied.is_empty() {
            return false;
        }
        let mut doomed = emptied;
        doomed.sort_unstable();
        for index in doomed.into_iter().rev() {
            appearance.shapes.remove(index);
        }
        // Indices below the removals have shifted; rather than repair them,
        // the selection is dropped — the click that got here is about to set
        // a new one anyway.
        self.symbol_selection = SymbolSelection::default();
        true
    }

    /// Where a canvas position is on screen, as of the last frame drawn.
    #[cfg(test)]
    ///
    /// The mapping depends on how the panels divided the window and on what
    /// `Scene` made of the rest, so it is recorded while drawing rather than
    /// worked out again — a second copy of that arithmetic would be a second
    /// thing to keep in step with egui.
    fn screen_pos(&self, canvas: egui::Pos2) -> egui::Pos2 {
        self.canvas_to_screen * canvas
    }

    fn content_rect(&self) -> Option<egui::Rect> {
        if self.view == toolbar::View::Appearance {
            let (_, appearance) = self.active_appearance();
            return Some(appearance.rect(egui::Pos2::ZERO));
        }

        let mut bounds: Option<egui::Rect> = None;
        let mut include = |rect: egui::Rect| {
            bounds = Some(match bounds {
                Some(current) => current.union(rect),
                None => rect,
            });
        };

        for placed in &self.placed {
            include(placed.rect());
        }
        for wire in &self.wires {
            for point in &wire.waypoints {
                include(egui::Rect::from_center_size(*point, egui::Vec2::ZERO));
            }
            for end in [wire.from, wire.to] {
                if let WireEndpoint::Free(at) = end {
                    include(egui::Rect::from_center_size(at, egui::Vec2::ZERO));
                }
            }
        }
        bounds
    }

    /// Whether a left drag on empty canvas moves the view right now — the
    /// hand tool always, and the arrow when the preference says so.
    ///
    /// False while a symbol is being drawn, along with its mirror below:
    /// there the primary button belongs to the drawing, and the middle one
    /// still pans. Gating it in these two is what makes it hold everywhere —
    /// the selection band is started from the scene's *background response*,
    /// well outside the appearance view's own code, so it went on sweeping a
    /// rectangle underneath every line being traced.
    fn pans_on_left_drag(&self) -> bool {
        if self.view == toolbar::View::Appearance {
            // The only tool there that gives the primary button back to the
            // view: every other one is drawing with it.
            return self.shape_tool == toolbar::ShapeTool::Pan;
        }
        if self.view == toolbar::View::Simulation {
            return self.sim_tool == toolbar::SimTool::Pan;
        }
        self.tool == Tool::Pan || (self.tool == Tool::Select && self.left_drag_pans)
    }

    /// The mirror: whether it sweeps a selection. The two are deliberately
    /// separate rather than one negated, because most tools do neither — a
    /// left drag while wiring is not a band and not a pan.
    fn bands_on_left_drag(&self) -> bool {
        self.view == toolbar::View::Schematic
            && (self.tool == Tool::Marquee || (self.tool == Tool::Select && !self.left_drag_pans))
    }

    /// Copies the selection.
    ///
    /// A wire comes along only when **both** its ends land inside the copied
    /// set — a wire whose far end is a pin you didn't copy has nowhere to
    /// attach, and guessing a loose end for it would paste something you
    /// never selected. So the rule is the predictable one: select the
    /// components and the wires between them.
    /// Builds `path`'s contents into this circuit's engine, and reports how
    /// to reach its ports.
    ///
    /// The innards end up in the engine but **not** in `placed`: they are
    /// not part of this drawing and must never be selected, moved or saved
    /// here. `place` is reused to build them — it knows how to turn a kind
    /// into a component, and duplicating that match is how the two would
    /// drift — and the entries it appends are then dropped again. The
    /// `Rc<Cell<…>>` handles go with them, which is exactly right: a switch
    /// inside a sub-circuit isn't yours to click from out here, and the
    /// engine component keeps its own clone of the cell.
    /// The symbol a circuit shows when it is placed: its own if it has been
    /// given one, and the generated box otherwise.
    ///
    /// Resolved once, when the instance is built — the same moment its
    /// innards are flattened, so a symbol and the circuit behind it are
    /// always the pair that was read together.
    fn appearance_of(&self, path: &str, ports: &[InstancePort]) -> Appearance {
        self.circuits
            .iter()
            .find(|circuit| circuit.path() == path)
            .and_then(|circuit| circuit.appearance.clone())
            .unwrap_or_else(|| Appearance::generated(ports))
    }

    fn flatten(&mut self, path: &str) -> Option<InstanceWiring> {
        // A circuit that contains itself, however indirectly, would flatten
        // forever. The stack is the whole guard.
        if self.flattening.iter().any(|open| open == path) {
            let strings = Strings::for_language(self.language);
            self.error = Some(strings.error_circuit_recursion.replace("{}", path));
            return None;
        }
        let saved = self
            .circuits
            .iter()
            .find(|circuit| circuit.path() == path)?
            .clone();
        self.flattening.push(path.to_string());

        let is_port = |kind: &ComponentKind| {
            matches!(
                kind,
                ComponentKind::InputPort | ComponentKind::OutputPort | ComponentKind::InOutPort
            )
        };

        // Built in saved order so an index maps straight across; ports are
        // skipped, so their slot stays empty.
        let first_inner = self.placed.len();
        let mut ids: Vec<Option<ComponentId>> = Vec::with_capacity(saved.components.len());
        for component in &saved.components {
            if is_port(&component.kind) {
                ids.push(None);
                continue;
            }
            let id = self.place(component.kind.clone(), egui::pos2(component.x, component.y));
            if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
                placed.set_rotation(component.rotation);
                // Applied before the entry is dropped: this is what puts a
                // switch's position or a port's resting level into the cell
                // the engine reads.
                placed.set_properties(component.properties.clone());
            }
            ids.push(Some(id));
        }
        // A sub-circuit may itself contain sub-circuits, and each of those
        // arrived as a `PlacedComponent` carrying the only record of how its
        // own innards are wired — a record about to be dropped with it.
        //
        // Its port pins are folded into its groups here, exactly as
        // `rebuild_nets` does for an instance sitting on the open drawing,
        // and the result is carried up. Without this, one level of nesting
        // worked and two did not: the inner circuit was built into the
        // engine and then connected to nothing.
        //
        // Carrying it up rather than flattening recursively into one list is
        // what makes the depth unbounded — each level hands its parent a
        // finished description of everything below it.
        let mut nested: Vec<Vec<(ComponentId, usize)>> = Vec::new();
        for placed in &self.placed[first_inner..] {
            let Some((ports, groups)) = placed.instance_wiring() else {
                continue;
            };
            let mut folded = groups.to_vec();
            for (index, port) in ports.iter().enumerate() {
                if let Some(group) = port.group.and_then(|g| folded.get_mut(g)) {
                    group.push((placed.id(), index));
                }
            }
            nested.extend(folded);
        }

        self.placed.truncate(first_inner);
        self.flattening.pop();

        let live = |group: &Vec<(usize, usize)>| -> Vec<(ComponentId, usize)> {
            group
                .iter()
                .filter_map(|&(component, pin)| Some((ids.get(component).copied().flatten()?, pin)))
                .collect()
        };
        let groups = saved.pin_groups();

        let mut ports = Self::port_slots(&saved);
        for (index, port) in &mut ports {
            port.group = groups.iter().position(|g| g.contains(&(*index, 0)));
        }
        // Kept whole — not filtered down to the groups that have live pins —
        // so a port's index still lands on the right one. A group with no
        // live pins at all is not noise: it's a net that exists only to join
        // ports to each other.
        // Ours first, so a port's `group` index still lands on the right
        // one; anything carried up from a nested instance is appended after.
        let mut inner_groups: Vec<Vec<(ComponentId, usize)>> = groups.iter().map(live).collect();
        inner_groups.extend(nested);
        Some((
            ports.into_iter().map(|(_, port)| port).collect(),
            inner_groups,
        ))
    }

    /// A saved circuit's ports, paired with their index in it, in the order
    /// a box lays them out.
    ///
    /// Read without building anything, so the placement preview can draw the
    /// right box before a single component exists — and so the preview and
    /// the real thing can't disagree about which pins there are or in what
    /// order. `inner` is left empty; only `flatten` can fill it.
    fn port_slots(saved: &SavedCircuit) -> Vec<(usize, InstancePort)> {
        let mut ports: Vec<(f32, usize, InstancePort)> = saved
            .components
            .iter()
            .enumerate()
            .filter(|(_, component)| {
                matches!(
                    component.kind,
                    ComponentKind::InputPort | ComponentKind::OutputPort | ComponentKind::InOutPort
                )
            })
            .map(|(index, component)| {
                (
                    component.y,
                    index,
                    InstancePort {
                        // An unnamed port still needs something on the box.
                        // Its index is the honest answer — better a number
                        // you can match to a position than a guess at intent.
                        name: component
                            .properties
                            .label()
                            .map(str::to_string)
                            .unwrap_or_else(|| index.to_string()),
                        kind: component.kind.clone(),
                        // Only `flatten` knows the sub-circuit's nets.
                        group: None,
                    },
                )
            })
            .collect();
        // Ordered by where the port sits in the sub-circuit, so moving a port
        // up there moves its pin up on the box out here — the only ordering
        // the user can see and control.
        ports.sort_by(|a, b| a.0.total_cmp(&b.0));
        ports
            .into_iter()
            .map(|(_, index, port)| (index, port))
            .collect()
    }

    /// Puts the selection on the system clipboard, and keeps a copy.
    ///
    /// The copy is what the Edit menu's Paste uses: a menu item has no way
    /// to read the system clipboard, since egui only ever surfaces it
    /// through the `Ctrl+V` event. So the two paths differ on purpose —
    /// `Ctrl+V` pastes whatever is really on the clipboard, including from
    /// another window, and the menu pastes what this window last copied.
    fn copy_to_clipboard(&mut self, ctx: &egui::Context) {
        if let Some(fragment) = self.copied_fragment() {
            ctx.copy_text(fragment.clone());
            self.clipboard = Some(fragment);
        }
    }

    /// Returns the fragment as JSON for the system clipboard, or `None` when
    /// there's nothing to copy. Kept separate from the clipboard call itself
    /// so the interesting half can be tested without a UI.
    fn copied_fragment(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }

        let ids: Vec<ComponentId> = self
            .placed
            .iter()
            .filter(|placed| self.selection.components.contains(&placed.id()))
            .map(|placed| placed.id())
            .collect();
        let index_of: HashMap<ComponentId, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let components: Vec<SavedComponent> = self
            .placed
            .iter()
            .filter(|placed| index_of.contains_key(&placed.id()))
            .map(|placed| {
                let center = placed.center();
                SavedComponent {
                    kind: placed.kind(),
                    x: center.x,
                    y: center.y,
                    rotation: placed.rotation(),
                    properties: placed.properties().clone(),
                }
            })
            .collect();

        // Creation order is kept, so a junction still refers to a wire
        // earlier in the list — which is what lets paste resolve them in one
        // forward pass, exactly as loading a project does.
        let kept: Vec<&Wire> = self
            .wires
            .iter()
            .filter(|wire| self.selection.wires.contains(&wire.id))
            .collect();
        let wire_index: HashMap<u64, usize> = kept
            .iter()
            .enumerate()
            .map(|(i, wire)| (wire.id, i))
            .collect();
        let save = |endpoint: WireEndpoint| match endpoint {
            WireEndpoint::Pin(component, pin) => index_of
                .get(&component)
                .map(|&i| SavedEndpoint::Pin(i, pin)),
            WireEndpoint::Junction { wire, waypoint } => wire_index
                .get(&wire)
                .map(|&i| SavedEndpoint::Junction { wire: i, waypoint }),
            WireEndpoint::Free(at) => Some(SavedEndpoint::Free(at.x, at.y)),
        };
        let wires: Vec<SavedWire> = kept
            .iter()
            .filter_map(|wire| {
                Some(SavedWire {
                    from: save(wire.from)?,
                    to: save(wire.to)?,
                    waypoints: wire.waypoints.iter().map(|p| (p.x, p.y)).collect(),
                    color: wire.color,
                })
            })
            .collect();

        serde_json::to_string(&Fragment {
            simlogix_fragment: crate::project::CURRENT_VERSION,
            components,
            wires,
        })
        .ok()
    }

    /// Pastes the clipboard one grid step down and right, and leaves the
    /// pasted copy selected — so a second paste, or a drag, acts on it
    /// rather than on what you copied from.
    fn paste_fragment(&mut self, text: &str) {
        let Ok(clip) = serde_json::from_str::<Fragment>(text) else {
            // Not ours: something else is on the clipboard, and pasting it
            // here means nothing.
            return;
        };
        if clip.components.is_empty() && clip.wires.is_empty() {
            return;
        }
        self.record_edit();

        let offset = egui::vec2(canvas::GRID_SPACING, canvas::GRID_SPACING);
        let ids: Vec<ComponentId> = clip
            .components
            .iter()
            .map(|saved| {
                let id = self.place(saved.kind.clone(), egui::pos2(saved.x, saved.y) + offset);
                if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
                    placed.set_rotation(saved.rotation);
                    placed.set_properties(saved.properties.clone());
                }
                id
            })
            .collect();

        let mut wire_ids: Vec<u64> = Vec::with_capacity(clip.wires.len());
        for saved in &clip.wires {
            let load = |endpoint: &SavedEndpoint| match *endpoint {
                SavedEndpoint::Pin(component, pin) => {
                    ids.get(component).map(|&id| WireEndpoint::Pin(id, pin))
                }
                SavedEndpoint::Junction { wire, waypoint } => {
                    wire_ids.get(wire).map(|&host| WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    })
                }
                SavedEndpoint::Free(x, y) => Some(WireEndpoint::Free(egui::pos2(x, y) + offset)),
            };
            let (Some(from), Some(to)) = (load(&saved.from), load(&saved.to)) else {
                continue;
            };
            let waypoints = saved
                .waypoints
                .iter()
                .map(|&(x, y)| egui::pos2(x, y) + offset)
                .collect();
            let id = self.add_wire(from, to, waypoints);
            if let Some(wire) = self.wires.iter_mut().find(|wire| wire.id == id) {
                wire.color = saved.color;
            }
            wire_ids.push(id);
        }

        self.selection.clear();
        self.selection.components.extend(ids);
        self.selection.wires.extend(wire_ids);

        self.rebuild_nets();
        self.net_fingerprint = self.connectivity_fingerprint();
        self.advance_circuit(SETTLE_TICKS);
        self.dirty = true;
    }

    /// Turns a placed component into its sibling kind — an NMOS into a
    /// PMOS, a transceiver's enable from `EN` to `OE`.
    ///
    /// Done by rewriting the saved form and reopening, rather than by
    /// swapping the component inside `Circuit`: a component's identity there
    /// is its `ComponentId`, which every wire endpoint refers to, so
    /// replacing it in place would mean remapping all of them. Going through
    /// the document keeps the wires, the routes and everything else exactly
    /// as they are, since they're stored by *index*. It costs a rebuild —
    /// the same one undo and switching circuits already pay.
    ///
    /// Only valid between kinds with the same pins; `properties::VARIANTS`
    /// is the list of pairs that qualify.
    fn change_kind(&mut self, id: ComponentId, kind: ComponentKind) {
        let Some(index) = self.placed.iter().position(|placed| placed.id() == id) else {
            return;
        };

        let mut project = self.to_project();
        let Some(component) = project
            .circuits
            .get_mut(self.active)
            .and_then(|circuit| circuit.components.get_mut(index))
        else {
            return;
        };
        component.kind = kind;

        let open = self.active;
        self.reopen(&project, open);
        // Ids are handed out afresh by the rebuild, so the selection is
        // recovered by position — otherwise changing the type would
        // deselect the thing you're editing.
        self.selection.clear();
        if let Some(placed) = self.placed.get(index) {
            self.selection.components.insert(placed.id());
        }
        self.dirty = true;
    }

    /// Renames the project's library. Refuses an empty name — every
    /// reference to a circuit in this project is qualified by it.
    fn rename_project(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() || name == self.library {
            return;
        }
        self.record_edit();
        self.library = name.to_string();
    }

    fn write_project_to(&mut self, path: &std::path::Path) -> bool {
        self.name_library_after(path);
        let project = self.to_project();
        let result = project
            .to_container()
            .and_then(|bytes| std::fs::write(path, bytes).map_err(|err| err.to_string()));
        match result {
            Ok(()) => {
                self.current_path = Some(path.to_path_buf());
                // Saving somewhere new is as much "this is what I'm working
                // on" as opening is, and a project written once and not
                // reopened would otherwise never appear in the list.
                self.remember_recent(path);
                self.dirty = false;
                true
            }
            Err(message) => {
                let strings = Strings::for_language(self.language);
                self.error = Some(strings.error_save_failed.replace("{}", &message));
                false
            }
        }
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            // Projects written before the container format are still offered:
            // they open fine, since the format is read from the bytes.
            .add_filter(
                "SimLogix project",
                &[project::PROJECT_EXTENSION, project::LEGACY_EXTENSION],
            )
            .pick_file()
        else {
            return;
        };

        self.open_path(path);
    }

    /// Opens a project from a known path — what the dialog hands over, and
    /// what the recent list replays.
    fn open_path(&mut self, path: PathBuf) {
        let result = std::fs::read(&path)
            .map_err(|err| err.to_string())
            .and_then(|bytes| SavedProject::from_bytes(&bytes));
        match result {
            Ok(project) => {
                // Loading a project resets everything else, but the
                // language is a UI preference, not part of the circuit.
                let preferences = (self.language, self.language_chosen, self.left_drag_pans);
                let recent = std::mem::take(&mut self.recent);
                *self = Self::from_project(&project, 0);
                (self.language, self.language_chosen, self.left_drag_pans) = preferences;
                self.recent = recent;
                self.refit_view = true;
                self.name_library_after(&path);
                self.remember_recent(&path);
                self.current_path = Some(path);
            }
            Err(message) => {
                let strings = Strings::for_language(self.language);
                self.error = Some(strings.error_open_failed.replace("{}", &message));
                // A file that cannot be read is not one to keep offering.
                // Failing *is* the answer to "is this still here?", so there
                // is no separate check to keep in step with it.
                self.forget_recent(&path);
            }
        }
    }

    /// Puts a path at the head of the recent list.
    ///
    /// Any earlier mention of the same file is removed first, so reopening
    /// something moves it up rather than filling the list with itself.
    fn remember_recent(&mut self, path: &std::path::Path) {
        // Resolved where possible, so the same file reached two ways — a
        // relative path, a symlink — is one entry and not two.
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|held| held != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }

    fn forget_recent(&mut self, path: &std::path::Path) {
        let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|held| held != path && held != &resolved);
    }

    /// Removes `roots`, disconnecting each one's own pin.
    ///
    /// Wires tapped onto a removed one are **kept**, with their junction
    /// frozen in place (`WireEndpoint::Free`) — `resolved` says where each
    /// wire's waypoints currently are, so the loose end lands exactly where
    /// the contact point was. Deleting these instead (which is what an
    /// earlier version did, to stop an orphaned tap resolving to nothing and
    /// silently vanishing) meant deleting one gate could wipe out wiring
    /// that had nothing to do with it.
    fn remove_wires(&mut self, roots: Vec<u64>, resolved: &HashMap<u64, Vec<egui::Pos2>>) {
        for &id in &roots {
            let host_waypoints = resolved.get(&id);
            for wire in &mut self.wires {
                let WireEndpoint::Junction {
                    wire: host,
                    waypoint,
                } = wire.to
                else {
                    continue;
                };
                if host != id {
                    continue;
                }
                // Fall back to this wire's own last corner if the host's
                // geometry isn't known (it always is for a wire that was
                // just on screen), so a tap can never be left unresolvable.
                let at = host_waypoints
                    .and_then(|points| points.get(waypoint).copied())
                    .or_else(|| wire.waypoints.last().copied());
                if let Some(at) = at {
                    wire.to = WireEndpoint::Free(at);
                }
            }
        }

        // Nothing to disconnect by hand: dropping the wire from the drawing
        // is the edit, and the nets are recomputed from what's left.
        self.wires.retain(|wire| !roots.contains(&wire.id));
    }

    /// Shifts every junction tapped onto `host` at or past `from` by
    /// `delta`, so taps keep pointing at the same physical point when that
    /// wire's waypoint list grows or shrinks ahead of them.
    fn shift_junctions(&mut self, host: u64, from: usize, delta: isize) {
        for wire in &mut self.wires {
            if let WireEndpoint::Junction { wire: w, waypoint } = &mut wire.to {
                if *w == host && *waypoint >= from {
                    *waypoint = waypoint.saturating_add_signed(delta);
                }
            }
        }
    }

    /// Collapses waypoints of a wire that have ended up on the same spot.
    ///
    /// Two points at one position make a zero-length segment and, worse,
    /// two overlapping drag handles competing for the same click — you
    /// could never separate them again. Junctions follow onto the survivor:
    /// `shift_junctions` maps a tap on the removed point down onto the one
    /// that stays, which is at the very same place, so nothing appears to
    /// move.
    fn dedupe_waypoints(&mut self, wire_id: u64) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let mut at = 1;
        while at < self.wires[index].waypoints.len() {
            if self.wires[index].waypoints[at] == self.wires[index].waypoints[at - 1] {
                self.wires[index].waypoints.remove(at);
                self.shift_junctions(wire_id, at, -1);
            } else {
                at += 1;
            }
        }
    }

    /// Drops waypoint `index` from `wire_id`.
    ///
    /// Anything tapped onto exactly the point being removed is left where
    /// that point was, since it no longer has one to hold on to.
    fn remove_waypoint(&mut self, wire_id: u64, index: usize, resolved: &[egui::Pos2]) {
        let Some(position) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        if index >= self.wires[position].waypoints.len() {
            return;
        }
        self.record_edit();
        self.wires[position].waypoints.remove(index);

        // Anything tapped onto the point just removed is left where that
        // point was, rather than deleted along with it.
        let at = resolved.get(index).copied();
        for wire in &mut self.wires {
            if let WireEndpoint::Junction {
                wire: host,
                waypoint,
            } = wire.to
            {
                if host == wire_id && waypoint == index {
                    if let Some(at) = at {
                        wire.to = WireEndpoint::Free(at);
                    }
                }
            }
        }
        self.shift_junctions(wire_id, index, -1);
    }

    /// Starts or stops the simulation. Resuming also clears a fault: the
    /// user has presumably just fixed the circuit, and if they haven't, the
    /// very next tick reports it again.
    fn toggle_running(&mut self) {
        self.running = !self.running;
        if self.running {
            self.unstable_net = None;
        }
    }

    /// Advances the simulation, unless it's paused or already stopped on a
    /// fault. An engine that reports it can't settle pauses the simulation
    /// and surfaces the offending net in the status bar — the alternative
    /// would be looping forever or taking the whole program down, and the
    /// circuit is still there to be inspected and fixed either way.
    fn advance_circuit(&mut self, ticks: u64) {
        if !self.running || self.unstable_net.is_some() {
            return;
        }
        if let Err(unstable) = self.circuit.advance(ticks) {
            self.unstable_net = Some(unstable.net);
            self.running = false;
        }
    }

    /// Joins a wire's loose end to any other loose end sitting at the very
    /// same place.
    ///
    /// A cut can leave two ends stacked: when one wire tapped the cut point
    /// from *both* sides, joining it onto the first piece also carries its
    /// other end over, which then lands on the second piece's end. That
    /// second one has nothing left to join against by name — only by
    /// position, which is what this does.
    fn join_touching_loose_end(&mut self, wire_id: u64, is_from: bool) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let end = if is_from {
            self.wires[index].from
        } else {
            self.wires[index].to
        };
        let WireEndpoint::Free(at) = end else {
            return;
        };

        let touching = self.wires.iter().find_map(|other| {
            if other.id == wire_id {
                return None;
            }
            [(true, other.from), (false, other.to)]
                .into_iter()
                .find_map(|(other_is_from, other_end)| match other_end {
                    // Everything here is grid-snapped, so this only forgives
                    // floating-point dust, not genuinely separate points.
                    WireEndpoint::Free(pos) if pos.distance(at) < 0.5 => {
                        Some((other.id, other_is_from))
                    }
                    _ => None,
                })
        });

        if let Some((other_id, other_is_from)) = touching {
            self.join_wires(wire_id, is_from, other_id, other_is_from, at);
            self.dedupe_waypoints(wire_id);
        }
    }

    /// Cuts one segment out of a wire, leaving the piece before it and the
    /// piece after as separate wires, each ending loose where the cut was.
    /// Cutting at either extreme leaves a single piece; cutting the only
    /// segment of an unrouted wire removes it entirely.
    ///
    /// `path` is the wire as currently drawn — `from`, then its waypoints,
    /// then `to` — so the cut segment runs from `path[cut]` to
    /// `path[cut + 1]`. Passing the drawn path (rather than reading the
    /// stored waypoints) is what lets a wire still on its automatic route
    /// keep that shape in the pieces.
    ///
    /// The pieces are no longer joined, so whatever the wire connected is
    /// disconnected in the circuit too, exactly as if it had been deleted.
    fn split_wire(&mut self, wire_id: u64, cut: usize, path: &[egui::Pos2]) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        if path.len() < 2 || cut + 1 >= path.len() {
            return;
        }
        self.record_edit();

        let to = self.wires[index].to;
        let waypoints = &path[1..path.len() - 1];
        let last = waypoints.len();

        // A piece needs two points to exist: nothing survives before a cut
        // at the very start, or after one at the very end.
        let head = (cut >= 1).then(|| waypoints[..cut - 1].to_vec());
        let tail = (cut < last).then(|| waypoints[cut + 1..].to_vec());

        // The original wire becomes whichever piece survives — keeping its
        // id means taps and the current selection still refer to something.
        let (head_id, tail_id) = match (&head, &tail) {
            (Some(head_waypoints), _) => {
                self.wires[index].to = WireEndpoint::Free(path[cut]);
                self.wires[index].waypoints = head_waypoints.clone();
                let tail_id = tail.as_ref().map(|tail_waypoints| {
                    self.add_wire(
                        WireEndpoint::Free(path[cut + 1]),
                        to,
                        tail_waypoints.clone(),
                    )
                });
                (Some(wire_id), tail_id)
            }
            (None, Some(tail_waypoints)) => {
                self.wires[index].from = WireEndpoint::Free(path[cut + 1]);
                self.wires[index].waypoints = tail_waypoints.clone();
                (None, Some(wire_id))
            }
            (None, None) => {
                self.remove_wires(vec![wire_id], &HashMap::new());
                (None, None)
            }
        };

        // Re-home every tap: points kept by a piece move to it, while the
        // two bordering the cut are now loose ends, which can't be tapped —
        // anything attached there is cut loose in turn.
        // The two points bordering the cut stop being waypoints — each
        // becomes a piece's loose end — so taps on them have no waypoint
        // left to name. Rather than cutting those wires adrift, they're
        // noted here and joined onto the piece afterwards: they meet it end
        // to end at exactly that point, which is the one case `join_wires`
        // exists for. The connection is what matters; that two wires become
        // one is the same outcome as dropping their ends together by hand.
        let mut border_taps: Vec<(u64, bool, bool)> = Vec::new();
        for other in &mut self.wires {
            let tap_id = other.id;
            for (tap_is_from, end) in [(true, &mut other.from), (false, &mut other.to)] {
                let WireEndpoint::Junction {
                    wire: host,
                    waypoint,
                } = *end
                else {
                    continue;
                };
                if host != wire_id {
                    continue;
                }
                *end = match waypoint {
                    // Kept by the head, at the same index.
                    j if j + 1 < cut => match head_id {
                        Some(id) => WireEndpoint::Junction {
                            wire: id,
                            waypoint: j,
                        },
                        None => WireEndpoint::Free(path[j + 1]),
                    },
                    // Kept by the tail, shifted down past the cut.
                    j if j > cut => match tail_id {
                        Some(id) => WireEndpoint::Junction {
                            wire: id,
                            waypoint: j - (cut + 1),
                        },
                        None => WireEndpoint::Free(path[j + 1]),
                    },
                    // On the cut's own border: `j + 1 == cut` is the head's
                    // new end, otherwise it's the tail's new start.
                    j => {
                        border_taps.push((tap_id, tap_is_from, j + 1 == cut));
                        WireEndpoint::Free(path[j + 1])
                    }
                };
            }
        }

        for (tap_id, tap_is_from, on_head) in border_taps {
            let (Some(piece), at) = (
                if on_head { head_id } else { tail_id },
                if on_head { path[cut] } else { path[cut + 1] },
            ) else {
                continue;
            };
            // The head meets the cut at its `to`, the tail at its `from`.
            let piece_is_from = !on_head;
            // A piece has one end to give: if two wires tapped the same
            // point, the first takes it and the rest stay loose.
            let free = self.wires.iter().find(|w| w.id == piece).is_some_and(|w| {
                let end = if piece_is_from { w.from } else { w.to };
                matches!(end, WireEndpoint::Free(_))
            });
            if free {
                self.join_wires(piece, piece_is_from, tap_id, tap_is_from, at);
                self.dedupe_waypoints(piece);
            }
        }

        // Those joins can have brought a wire's far end to rest on the other
        // piece's end; nothing names it, so it's matched by position.
        if let Some(head) = head_id {
            self.join_touching_loose_end(head, false);
        }
        if let Some(tail) = tail_id {
            self.join_touching_loose_end(tail, true);
        }
    }

    /// Flips a wire end for end. Only its own geometry changes — what it
    /// connects is the same — but taps on it are mirrored so they stay on
    /// the point they were on.
    ///
    /// Used to line two wires up before joining them: a join needs one
    /// wire's loose end to be its `to` and the other's to be its `from`,
    /// and which is which depends on how each was drawn.
    fn reverse_wire(&mut self, wire_id: u64) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let count = self.wires[index].waypoints.len();
        let wire = &mut self.wires[index];
        std::mem::swap(&mut wire.from, &mut wire.to);
        wire.waypoints.reverse();

        for other in &mut self.wires {
            for end in [&mut other.from, &mut other.to] {
                if let WireEndpoint::Junction { wire, waypoint } = end {
                    if *wire == wire_id && *waypoint < count {
                        *waypoint = count - 1 - *waypoint;
                    }
                }
            }
        }
    }

    /// Joins two wires meeting at a loose end into a single one — the
    /// inverse of [`Self::split_wire`], so cutting a wire and dropping the
    /// pieces back together gives the wire back.
    ///
    /// `keep` survives and absorbs `absorb`; the point they meet at becomes
    /// an ordinary waypoint. Both are turned to face the same way first,
    /// and taps on `absorb` follow onto `keep` at their shifted position.
    fn join_wires(
        &mut self,
        keep: u64,
        keep_end_is_from: bool,
        absorb: u64,
        absorb_end_is_from: bool,
        at: egui::Pos2,
    ) {
        if keep == absorb {
            return;
        }
        // `keep` must end at the meeting point and `absorb` must start there.
        if keep_end_is_from {
            self.reverse_wire(keep);
        }
        if !absorb_end_is_from {
            self.reverse_wire(absorb);
        }

        let (Some(keep_index), Some(absorb_index)) = (
            self.wires.iter().position(|w| w.id == keep),
            self.wires.iter().position(|w| w.id == absorb),
        ) else {
            return;
        };

        let absorbed = self.wires.remove(absorb_index);
        let keep_index = self
            .wires
            .iter()
            .position(|w| w.id == keep)
            .unwrap_or(keep_index);
        let offset = self.wires[keep_index].waypoints.len() + 1;

        self.wires[keep_index].waypoints.push(at);
        self.wires[keep_index]
            .waypoints
            .extend(absorbed.waypoints.iter().copied());
        self.wires[keep_index].to = absorbed.to;

        for other in &mut self.wires {
            for end in [&mut other.from, &mut other.to] {
                if let WireEndpoint::Junction { wire, waypoint } = end {
                    if *wire == absorb {
                        *wire = keep;
                        *waypoint += offset;
                    }
                }
            }
        }
    }

    /// Rebuilds every net from the wires as they are currently drawn.
    ///
    /// This is the whole point of the geometric model: connectivity is
    /// *derived* from the drawing rather than accumulated as wires come and
    /// go, so nothing has to work out what a deletion should undo. Two
    /// parallel wires between the same pins, one removed, simply produce the
    /// same grouping again.
    ///
    /// The grouping is a union-find over three kinds of node: a component
    /// pin, a wire, and nothing at all (a loose end joins nothing). Each
    /// wire unions itself with whatever its two ends touch, so a junction —
    /// which unions with its *host wire* — transitively picks up everything
    /// that wire reaches, however deep the chain goes and in whatever order
    /// they were drawn.
    fn rebuild_nets(&mut self) {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Node {
            Pin(ComponentId, usize),
            Wire(u64),
            /// One of an instance's internal nets, which has no pin of this
            /// drawing's own to stand for it.
            Inner(ComponentId, usize),
        }

        let mut parent: HashMap<Node, Node> = HashMap::new();
        fn find(parent: &mut HashMap<Node, Node>, node: Node) -> Node {
            let mut root = node;
            while let Some(&next) = parent.get(&root) {
                if next == root {
                    break;
                }
                root = next;
            }
            // Path compression, so a long chain of taps stays cheap.
            let mut walk = node;
            while let Some(&next) = parent.get(&walk) {
                if next == root {
                    break;
                }
                parent.insert(walk, root);
                walk = next;
            }
            parent.entry(node).or_insert(root);
            root
        }
        fn union(parent: &mut HashMap<Node, Node>, a: Node, b: Node) {
            let (a, b) = (find(parent, a), find(parent, b));
            if a != b {
                parent.insert(a, b);
            }
        }

        for placed in &self.placed {
            let pin_count = self
                .circuit
                .try_pins(placed.id())
                .map(|pins| pins.len())
                .unwrap_or(0);
            for index in 0..pin_count {
                let node = Node::Pin(placed.id(), index);
                parent.entry(node).or_insert(node);
            }
        }

        for wire in &self.wires {
            let self_node = Node::Wire(wire.id);
            parent.entry(self_node).or_insert(self_node);
            for end in [wire.from, wire.to] {
                match end {
                    WireEndpoint::Pin(component, index) => {
                        union(&mut parent, self_node, Node::Pin(component, index));
                    }
                    WireEndpoint::Junction { wire: host, .. } => {
                        union(&mut parent, self_node, Node::Wire(host));
                    }
                    // A loose end connects nothing, so it contributes no
                    // union at all.
                    WireEndpoint::Free(_) => {}
                }
            }
        }

        // An instance's innards are not in this drawing, so what held them
        // together has to be put back by hand: the sub-circuit's own groups,
        // and each anchor pin joined to the net its port sat on.
        for placed in &self.placed {
            let Some((ports, inner_groups)) = placed.instance_wiring() else {
                continue;
            };
            // Each internal net gets a node of its own, and everything on it
            // — the innards' pins, and the instance's pins standing in for
            // the ports — is declared a member. One rule instead of two, and
            // it still holds when a net has no pins to link together.
            for (group, members) in inner_groups.iter().enumerate() {
                for &(component, pin) in members {
                    union(
                        &mut parent,
                        Node::Inner(placed.id(), group),
                        Node::Pin(component, pin),
                    );
                }
            }
            for (index, port) in ports.iter().enumerate() {
                if let Some(group) = port.group {
                    union(
                        &mut parent,
                        Node::Inner(placed.id(), group),
                        Node::Pin(placed.id(), index),
                    );
                }
            }
        }

        let mut groups: HashMap<Node, Vec<(ComponentId, usize)>> = HashMap::new();
        let nodes: Vec<Node> = parent.keys().copied().collect();
        for node in nodes {
            if let Node::Pin(component, index) = node {
                let root = find(&mut parent, node);
                groups.entry(root).or_default().push((component, index));
            }
        }

        // A lone pin is its own net anyway, which `rewire` already does for
        // anything it isn't told about.
        let groups: Vec<Vec<(ComponentId, usize)>> = groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();
        self.circuit.rewire(&groups);

        let mut wire_groups: HashMap<Node, Vec<u64>> = HashMap::new();
        for wire in &self.wires {
            let root = find(&mut parent, Node::Wire(wire.id));
            wire_groups.entry(root).or_default().push(wire.id);
        }
        self.inherit_wire_colors(wire_groups.into_values());
    }

    /// Gives a wire the colour of the net it has just joined.
    ///
    /// Only when the group agrees on one: joining two differently coloured
    /// nets leaves both colours in place rather than picking a winner. A
    /// two-tone net is visible and can be re-coloured, whereas a silent
    /// choice is neither.
    fn inherit_wire_colors(&mut self, groups: impl Iterator<Item = Vec<u64>>) {
        for group in groups {
            let mut colors = group
                .iter()
                .filter_map(|id| self.wires.iter().find(|wire| wire.id == *id))
                .filter_map(|wire| wire.color);
            let Some(first) = colors.next() else {
                continue;
            };
            if colors.any(|color| color != first) {
                continue;
            }
            for id in group {
                if let Some(wire) = self.wires.iter_mut().find(|wire| wire.id == id) {
                    wire.color = Some(first);
                }
            }
        }
    }

    /// Paints every wire of one net, which is what "colour a wire" means:
    /// the wires of a net are one conductor, so they get one colour.
    fn color_net(&mut self, wire_id: u64, color: Option<[u8; 3]>) {
        let Some(net) = self
            .wires
            .iter()
            .find(|wire| wire.id == wire_id)
            .and_then(|wire| self.wire_net(wire))
        else {
            // A wire with both ends loose carries no net; it's still a wire
            // on screen, so it takes the colour on its own.
            if let Some(wire) = self.wires.iter_mut().find(|wire| wire.id == wire_id) {
                wire.color = color;
            }
            return;
        };

        let members: Vec<u64> = self
            .wires
            .iter()
            .filter(|wire| self.wire_net(wire) == Some(net))
            .map(|wire| wire.id)
            .collect();
        for wire in self.wires.iter_mut().filter(|w| members.contains(&w.id)) {
            wire.color = color;
        }
    }

    /// A hash of the connectivity alone: which components exist, and what
    /// each wire's two ends attach to. Deliberately blind to positions and
    /// waypoint indices — dragging a corner point doesn't change what is
    /// connected to what, and shouldn't cost a rebuild.
    fn connectivity_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for placed in &self.placed {
            placed.id().hash(&mut hasher);
        }
        for wire in &self.wires {
            wire.id.hash(&mut hasher);
            for end in [wire.from, wire.to] {
                match end {
                    WireEndpoint::Pin(component, index) => {
                        (0u8, component, index).hash(&mut hasher)
                    }
                    // Which waypoint is tapped doesn't matter: any of them
                    // reaches the same wire.
                    WireEndpoint::Junction { wire, .. } => (1u8, wire).hash(&mut hasher),
                    WireEndpoint::Free(_) => 2u8.hash(&mut hasher),
                }
            }
        }
        hasher.finish()
    }

    /// Snapshots the document so the edit about to happen can be undone.
    /// **Call this before mutating**, not after — it records the state being
    /// left behind. For a drag, that means calling it the frame the drag
    /// starts, which is why `interact_box` deliberately doesn't move
    /// anything on that first frame.
    fn record_edit(&mut self) {
        self.undo_stack.push(self.to_project());
        if self.undo_stack.len() > MAX_UNDO_STEPS {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    fn undo(&mut self) {
        let Some(previous) = self.undo_stack.pop() else {
            return;
        };
        self.redo_stack.push(self.to_project());
        self.restore(&previous);
    }

    fn redo(&mut self) {
        let Some(next) = self.redo_stack.pop() else {
            return;
        };
        self.undo_stack.push(self.to_project());
        self.restore(&next);
    }

    /// Rebuilds the circuit from a snapshot. Which circuit stays open is
    /// matched by *name*, not index: the edit being undone may itself have
    /// added or removed a circuit, which shifts every index after it.
    fn restore(&mut self, project: &SavedProject) {
        let open_name = self.circuits.get(self.active).map(|c| c.name.clone());
        let open = open_name
            .and_then(|name| project.circuits.iter().position(|c| c.name == name))
            .unwrap_or(self.active);
        self.reopen(project, open);

        // Deliberately coarse: stepping back to whatever was last saved
        // still counts as dirty. Over-reporting only ever costs a redundant
        // save prompt, whereas under-reporting loses work.
        self.dirty = true;
    }

    /// Rebuilds from `project` with `open` as the circuit being edited,
    /// keeping everything that isn't part of the document itself: which file
    /// this is, whether it has unsaved edits, the undo history, the camera,
    /// and UI preferences.
    ///
    /// **Known cost**: this goes through [`Self::from_project`], so it
    /// rebuilds the `Circuit` from scratch and runtime state starts cold —
    /// a held button releases, a clock's phase resets. Accepted because it
    /// keeps undo defined by exactly one thing (the saved document) instead
    /// of needing every `Circuit` mutation to be individually invertible;
    /// switching circuits inherits the same trade, so a clock in a circuit
    /// you leave stops, and restarts from phase zero when you come back.
    fn reopen(&mut self, project: &SavedProject, open: usize) {
        // Preferences aren't document state: a rebuild must not quietly put
        // them back to their defaults.
        let preferences = (self.language, self.language_chosen, self.left_drag_pans);
        let recent = std::mem::take(&mut self.recent);
        let clipboard = self.clipboard.take();
        let dirty = self.dirty;
        let current_path = self.current_path.take();
        let undo_stack = std::mem::take(&mut self.undo_stack);
        let redo_stack = std::mem::take(&mut self.redo_stack);
        let window_title = std::mem::take(&mut self.window_title);
        // The camera is view state, not document state -- undoing an edit
        // shouldn't also throw away where you were looking. Which *view* is
        // showing is the same kind of thing, and matters more: undo goes
        // through here, so without this, undoing a line you just drew would
        // also throw you out of the appearance view and back to the
        // schematic.
        let scene_rect = self.scene_rect;
        let view = (
            self.view,
            self.idle_scene_rect,
            self.shape_tool,
            self.sim_tool,
        );

        *self = Self::from_project(project, open);
        self.scene_rect = scene_rect;
        (
            self.view,
            self.idle_scene_rect,
            self.shape_tool,
            self.sim_tool,
        ) = view;
        self.dirty = dirty;

        (self.language, self.language_chosen, self.left_drag_pans) = preferences;
        self.recent = recent;
        self.clipboard = clipboard;
        self.current_path = current_path;
        self.undo_stack = undo_stack;
        self.redo_stack = redo_stack;
        self.window_title = window_title;
    }

    /// Opens a different circuit for editing. The one being left is folded
    /// back into `circuits` first (that's what `to_project` does), so its
    /// layout survives the switch. Not an edit: nothing about the document
    /// changes, only which part of it is on screen.
    fn switch_to(&mut self, index: usize) {
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
    fn create_circuit(&mut self, in_folder: String) {
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
    fn create_folder(&mut self, parent: &str) {
        self.record_edit();
        let base = Strings::for_language(self.language).folder_default_name;
        let path = self.unique_folder_path(parent, base);
        self.folders.push(path);
    }

    /// Renames a folder's own segment, carrying everything filed under it
    /// along — sub-folders and circuits alike, since a path is a prefix.
    fn rename_folder(&mut self, path: &str, leaf: &str) {
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
    fn delete_folder(&mut self, path: &str) {
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
    fn move_circuit(&mut self, index: usize, folder: String) {
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
    fn delete_circuit(&mut self, index: usize) {
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
    fn repoint_instances(&mut self, from: &str, to: &str) {
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
    fn rename_circuit(&mut self, index: usize, name: &str) {
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

    /// Runs a destructive action, but only after the user has had a chance
    /// to rescue unsaved work: with edits pending it just opens the
    /// confirmation and returns, and the dialog calls back into
    /// [`Self::run_action`] once the answer is in.
    fn request_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.dirty {
            self.pending_action = Some(action);
        } else {
            self.run_action(action, ctx);
        }
    }

    fn run_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::New => {
                let (language, chosen, left_drag_pans) =
                    (self.language, self.language_chosen, self.left_drag_pans);
                let recent = std::mem::take(&mut self.recent);
                *self = Self::default();
                self.language = language;
                self.language_chosen = chosen;
                self.left_drag_pans = left_drag_pans;
                self.recent = recent;
            }
            PendingAction::Open => self.open_project(),
            PendingAction::OpenRecent(path) => self.open_path(path),
            PendingAction::Quit => {
                // `dirty` is already false by the time we get here (either
                // saved, or explicitly discarded), so the close request this
                // sends won't be intercepted and bounced back again.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// `SimLogix — <file name or "Untitled"><* if unsaved>`, so the title bar
    /// answers both "which circuit is this?" and "have I saved it?".
    fn title(&self, strings: &Strings) -> String {
        let name = self
            .current_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| strings.title_untitled.to_string());
        let marker = if self.dirty { "*" } else { "" };
        format!("SimLogix — {name}{marker}")
    }
}

impl SimLogixApp {
    /// Builds the app, applying whatever preferences were stored last time.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        let Some(storage) = cc.storage else {
            return app;
        };
        let settings: Settings = eframe::get_value(storage, SETTINGS_KEY).unwrap_or_default();

        // A stored language wins over the OS locale; no stored language
        // means the user never chose, so keep following the system.
        if let Some(language) = settings.language {
            app.language = language;
        }
        app.left_drag_pans = settings.left_drag_pans;
        app.recent = settings.recent;
        app
    }
}

impl eframe::App for SimLogixApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            SETTINGS_KEY,
            &Settings {
                // Recorded only once it has been picked from the menu, so a
                // machine whose locale changes still follows it.
                language: self.language_chosen.then_some(self.language),
                left_drag_pans: self.left_drag_pans,
                recent: self.recent.clone(),
            },
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The frame is unused — every viewport command this app sends goes
        // through the context. Keeping the body out of the trait method is
        // what lets a test drive the whole application with nothing but a
        // `Ui`, which is the only way to reach the bugs that live between
        // widgets rather than inside them.
        self.draw(ui);
    }
}

impl SimLogixApp {
    /// One frame of the whole application.
    pub(crate) fn draw(&mut self, ui: &mut egui::Ui) {
        // Advance the circuit by real elapsed time every frame, not just
        // after an explicit interaction -- this is what lets a placed Clock
        // keep ticking on its own. Requesting continuous repaint is what
        // makes egui keep calling this at all without new input; the
        // tradeoff is constant redraw (and CPU use) instead of only on
        // interaction, even for a circuit with no Clock in it.
        self.tick_budget += ui.ctx().input(|i| i.stable_dt) * TICKS_PER_SECOND;
        let ticks_due = self.tick_budget.floor();
        if ticks_due > 0.0 {
            self.tick_budget -= ticks_due;
            self.advance_circuit(ticks_due as u64);
        }
        ui.ctx().request_repaint();

        let strings = Strings::for_language(self.language);

        // Keep the OS window title in step with which file is open and
        // whether it still has unsaved edits, but only push it when it
        // actually changed -- this runs every frame.
        let title = self.title(strings);
        if title != self.window_title {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        // Closing the window is just another way to trigger Quit, so it goes
        // through the same unsaved-changes check: bounce this request and
        // put the confirmation up instead.
        if ui.ctx().input(|i| i.viewport().close_requested()) && self.dirty {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_action = Some(PendingAction::Quit);
        }

        let new_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
        let open_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
        let save_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        let save_as_shortcut = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::S,
        );
        // Consumed here, before any widget sees the keystroke, so a shortcut
        // never doubles as canvas input.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&new_shortcut)) {
            self.request_action(PendingAction::New, ui.ctx());
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&open_shortcut)) {
            self.request_action(PendingAction::Open, ui.ctx());
        }
        if ui
            .ctx()
            .input_mut(|i| i.consume_shortcut(&save_as_shortcut))
        {
            self.save_project_as();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&save_shortcut)) {
            self.save_project();
        }

        if !ui.ctx().text_edit_focused()
            && ui.ctx().input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::NONE,
                    egui::Key::C,
                ))
            })
        {
            self.show_signal_state = !self.show_signal_state;
        }

        // Copy and paste act on the canvas selection, so they're guarded the
        // same way as everything else here: Ctrl+C in a name field has to
        // stay Ctrl+C in a name field.
        //
        // Read as *events*, not as a `Ctrl+C` shortcut: egui turns those two
        // chords into `Event::Copy`/`Event::Paste` and never emits the key
        // press, so matching on the chord silently never fires. It also means
        // this follows whatever the platform's copy chord actually is.
        // Copy still works while a circuit is only being watched — reading
        // something out changes nothing. Pasting does, so it doesn't.
        if !ui.ctx().text_edit_focused() {
            let mut copy = false;
            let mut pasted = None;
            ui.ctx().input(|input| {
                for event in &input.events {
                    match event {
                        egui::Event::Copy => copy = true,
                        egui::Event::Paste(text) => pasted = Some(text.clone()),
                        _ => {}
                    }
                }
            });
            if copy {
                self.copy_to_clipboard(ui.ctx());
            }
            if let Some(text) = pasted.filter(|_| self.view == toolbar::View::Schematic) {
                self.paste_fragment(&text);
            }
        }

        // The canvas keeps off the keyboard entirely while text is being
        // typed anywhere — the circuit tree's rename field, a component's
        // name in the properties panel, or anything added later. Asking egui
        // who has focus is what makes that hold for all of them; the first
        // version of this guard knew only about the tree's own field, so
        // Backspace in the properties panel deleted the component.
        //
        // Short-circuited before `consume_shortcut` so the keystroke reaches
        // the field instead of being eaten: a name can contain a space.
        if !ui.ctx().text_edit_focused()
            && ui.ctx().input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::NONE,
                    egui::Key::Space,
                ))
            })
        {
            self.toggle_running();
        }

        let undo_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
        // Both the Ctrl+Shift+Z and the Ctrl+Y conventions, since which one
        // means "redo" depends entirely on what the user came from.
        let redo_shortcut = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        );
        let redo_alt_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
        // Shown in the menu only. These chords never arrive as key presses —
        // egui turns them into `Event::Copy`/`Event::Paste` — so they are
        // labels here rather than something to consume.
        let copy_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
        let paste_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::V);
        // Redo is tested first: Ctrl+Shift+Z would otherwise also match the
        // plain Ctrl+Z pattern and undo instead.
        if ui.ctx().input_mut(|i| {
            i.consume_shortcut(&redo_shortcut) || i.consume_shortcut(&redo_alt_shortcut)
        }) {
            self.redo();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&undo_shortcut)) {
            self.undo();
        }

        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(strings.menu_file, |ui| {
                    let shortcut =
                        |ui: &egui::Ui, s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_new)
                                .shortcut_text(shortcut(ui, &new_shortcut)),
                        )
                        .clicked()
                    {
                        self.request_action(PendingAction::New, ui.ctx());
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_open)
                                .shortcut_text(shortcut(ui, &open_shortcut)),
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
                                .shortcut_text(shortcut(ui, &save_shortcut)),
                        )
                        .clicked()
                    {
                        self.save_project();
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_save_as)
                                .shortcut_text(shortcut(ui, &save_as_shortcut)),
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
                                .shortcut_text(shortcut(ui, &undo_shortcut)),
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
                                .shortcut_text(shortcut(ui, &redo_shortcut)),
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
                                .shortcut_text(shortcut(ui, &copy_shortcut)),
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
                                .shortcut_text(shortcut(ui, &paste_shortcut)),
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

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            let hint = if let Some(net) = self.unstable_net {
                Some(strings.status_unstable.replace("{}", &net.0.to_string()))
            } else if !self.show_signal_state {
                Some(strings.status_signals_hidden.to_string())
            } else if !self.running {
                Some(strings.status_paused.to_string())
            } else if self.view == toolbar::View::Simulation {
                // Everything below this arm offers an editing gesture, and
                // this mode has taken them all away. A selection is still
                // possible here — a component has to answer a click, or a
                // switch could not be flipped — so those hints would
                // otherwise appear and name keys that do nothing.
                Some(strings.hint_simulation.to_string())
            } else if self.wiring_from.is_some() {
                Some(strings.hint_wiring.to_string())
            } else if let Tool::Place(kind) = &self.tool {
                let label = strings.component_kind_label(kind);
                Some(strings.palette_click_to_place.replace("{}", label))
            } else if self.selection.lone_wire().is_some() {
                Some(strings.hint_delete_wire.to_string())
            } else if self.selection.lone_component().is_some() {
                Some(strings.hint_rotate_delete_component.to_string())
            } else if !self.selection.is_empty() {
                Some(
                    strings
                        .hint_selection
                        .replace("{}", &self.selection.len().to_string()),
                )
            } else {
                None
            };
            ui.label(hint.unwrap_or_default());
        });

        // The root of the tree is the project itself, labelled with its
        // library name — the file name only stands in for it before the
        // project has ever been saved. Computed out here because the panel
        // closure below borrows `self`.
        let project_name = if self.library.is_empty() {
            self.current_path
                .as_ref()
                .and_then(|path| path.file_stem())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| strings.title_untitled.to_string())
        } else {
            self.library.clone()
        };
        let mut tree_action = None;

        egui::Panel::left("palette")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| {
                // The circuit tree and the palette share the left column:
                // one is what you're editing, the other is what you put in
                // it. Nested as its own resizable panel so a project with
                // many circuits can't push the palette off the bottom.
                egui::Panel::top("circuit_tree")
                    .resizable(true)
                    .default_size(190.0)
                    .size_range(70.0..=420.0)
                    .show(ui, |ui| {
                        // The tree owns its own scrolling: its heading has to
                        // stay outside the scroll area, so the split belongs
                        // with the layout rather than here.
                        tree_action = circuit_tree::show(
                            ui,
                            strings,
                            &project_name,
                            &self.folders,
                            &self.circuits,
                            self.active,
                            std::mem::take(&mut self.reveal_active),
                            &mut self.renaming,
                        );
                    });

                // A resizable panel only stays at the width the user drags it
                // to if its content actually fills that width — otherwise it
                // re-shrinks to fit content on the next layout (e.g. right
                // after collapsing a category). `ScrollArea` does that, and
                // also means a longer palette scrolls instead of shrinking
                // the whole window.
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    // Greyed out rather than hidden while the circuit is
                    // being watched: a palette that comes and goes is one you
                    // have to re-find, and its being unusable is the point
                    // being made.
                    ui.add_enabled_ui(self.view == toolbar::View::Schematic, |ui| {
                        if let Some(tool) = palette::show(ui, strings, &self.tool) {
                            // Clicking the active entry again drops back to
                            // selecting, so the palette doubles as its own
                            // "never mind" button.
                            // A different component starts upright; the
                            // rotation you dialled in belongs to the one you
                            // were placing, not to the palette.
                            if self.tool != tool {
                                self.place_rotation = canvas::Rotation::default();
                            }
                            self.tool = if self.tool == tool {
                                Tool::Select
                            } else {
                                tool
                            };
                        }
                    });
                });
            });

        // Acted on after the panel closes, so nothing here is rebuilding the
        // app from under a borrow of it.
        match tree_action {
            Some(TreeAction::Open(index)) => self.switch_to(index),
            Some(TreeAction::Create { folder }) => self.create_circuit(folder),
            Some(TreeAction::CreateFolder { parent }) => self.create_folder(&parent),
            Some(TreeAction::BeginRename(target)) => {
                let name = match &target {
                    RenameTarget::Project => self.library.clone(),
                    RenameTarget::Circuit(index) => self
                        .circuits
                        .get(*index)
                        .map(|circuit| circuit.name.clone())
                        .unwrap_or_default(),
                    // Only the folder's own segment is offered for editing:
                    // renaming it must not double as moving it.
                    RenameTarget::Folder(path) => {
                        path.rsplit('/').next().unwrap_or_default().to_string()
                    }
                };
                self.renaming = Some((target, name));
            }
            Some(TreeAction::CommitRename) => match self.renaming.take() {
                Some((RenameTarget::Project, name)) => self.rename_project(&name),
                Some((RenameTarget::Circuit(index), name)) => self.rename_circuit(index, &name),
                Some((RenameTarget::Folder(path), leaf)) => self.rename_folder(&path, &leaf),
                None => {}
            },
            Some(TreeAction::CancelRename) => self.renaming = None,
            Some(TreeAction::Place(index)) => {
                if let Some(circuit) = self.circuits.get(index) {
                    // Straight into the placement tool, so dropping an
                    // instance is the same gesture as dropping any other
                    // component -- preview included.
                    self.tool = Tool::Place(ComponentKind::Circuit(circuit.path()));
                    self.place_rotation = canvas::Rotation::default();
                }
            }
            Some(TreeAction::Delete(index)) => self.delete_circuit(index),
            Some(TreeAction::DeleteFolder(path)) => self.delete_folder(&path),
            Some(TreeAction::MoveCircuit { circuit, folder }) => self.move_circuit(circuit, folder),
            None => {}
        }

        // The panel edits a *copy*: `record_edit` has to snapshot the state
        // before the change, and it can't run while `self.placed` is
        // borrowed by the panel. So the edit is carried back out and applied
        // below, snapshot first.
        let mut pending_properties: Option<(ComponentId, Properties, bool)> = None;
        let mut pending_wire_color: Option<(u64, Option<[u8; 3]>)> = None;
        // The panel edits a copy, as it does for a component: `record_edit`
        // can't run while `self` is borrowed by it, and editing in place
        // would snapshot the state *after* the change for the first frame of
        // every edit.
        let mut pending_shape: Option<(usize, crate::appearance::Shape, bool)> = None;
        let mut pending_pin: Option<(usize, crate::appearance::PinSlot, bool)> = None;
        let mut pending_symbol: Option<(bool, bool)> = None;
        let mut pending_kind: Option<(ComponentId, ComponentKind)> = None;
        egui::Panel::right("properties")
            .resizable(true)
            .default_size(220.0)
            .size_range(180.0..=400.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("properties_scroll")
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        // Greyed out, not emptied, while a circuit is being
                        // watched: what a component is set to is worth
                        // reading, and only changing it is out of bounds. The
                        // reason sits *outside* the disabled region, or it
                        // would be greyed out along with everything it
                        // explains.
                        let editable = self.view != toolbar::View::Simulation;
                        if !editable {
                            ui.label(egui::RichText::new(strings.properties_read_only).weak());
                            ui.separator();
                        }
                        ui.add_enabled_ui(editable, |ui| {
                            // A symbol's shapes are what's selectable while the
                            // appearance is showing; components and wires belong
                            // to the other view and aren't even on screen.
                            if self.view == toolbar::View::Appearance {
                                let (ports, mut appearance) = self.active_appearance();
                                let lone_shape = match self.symbol_selection.shapes.as_slice() {
                                    [only] if self.symbol_selection.pins.is_empty() => Some(*only),
                                    _ => None,
                                };
                                let lone_pin = match self.symbol_selection.pins.as_slice() {
                                    [only] if self.symbol_selection.shapes.is_empty() => {
                                        Some(*only)
                                    }
                                    _ => None,
                                };
                                if let Some(index) = lone_shape {
                                    if let Some(shape) = appearance.shapes.get_mut(index) {
                                        let before = shape.clone();
                                        let outcome = properties::show_shape(ui, strings, shape);
                                        if outcome.edit_started || *shape != before {
                                            pending_shape =
                                                Some((index, shape.clone(), outcome.edit_started));
                                        }
                                    }
                                } else if let Some(index) = lone_pin {
                                    if let Some(pin) = appearance.pins.get_mut(index) {
                                        let before = *pin;
                                        let outcome = properties::show_pin(
                                            ui,
                                            strings,
                                            pin,
                                            ports.get(index),
                                        );
                                        if outcome.edit_started || *pin != before {
                                            pending_pin = Some((index, *pin, outcome.edit_started));
                                        }
                                    }
                                } else if self.symbol_selection.is_empty() {
                                    let before = appearance.show_name;
                                    let outcome =
                                        properties::show_symbol(ui, strings, &mut appearance);
                                    if outcome.edit_started || appearance.show_name != before {
                                        pending_symbol =
                                            Some((appearance.show_name, outcome.edit_started));
                                    }
                                } else {
                                    // Several things picked: no one set of
                                    // properties to show.
                                    ui.label(strings.shape_none_selected);
                                }
                                return;
                            }

                            // Only a lone selection has properties to show: with
                            // several picked there is no one set to edit.
                            let selected_wire = self
                                .selection
                                .lone_wire()
                                .and_then(|id| self.wires.iter().find(|wire| wire.id == id));
                            if let Some(wire) = selected_wire {
                                if let Some(color) = properties::show_wire(ui, strings, wire.color)
                                {
                                    pending_wire_color = Some((wire.id, color));
                                }
                                return;
                            }

                            let selected = self
                                .selection
                                .lone_component()
                                .and_then(|id| self.placed.iter().find(|placed| placed.id() == id));
                            match selected {
                                Some(placed) => {
                                    let mut edited = placed.properties().clone();
                                    let outcome =
                                        properties::show(ui, strings, &placed.kind(), &mut edited);
                                    if let Some(kind) = outcome.change_kind {
                                        pending_kind = Some((placed.id(), kind));
                                    }
                                    if outcome.edit_started || edited != *placed.properties() {
                                        pending_properties =
                                            Some((placed.id(), edited, outcome.edit_started));
                                    }
                                }
                                None => {
                                    ui.label(strings.properties_none_selected);
                                }
                            }
                        });
                    });
            });

        if let Some((show_name, edit_started)) = pending_symbol {
            if edit_started {
                self.record_edit();
            }
            let (_, mut appearance) = self.active_appearance();
            appearance.show_name = show_name;
            self.circuits[self.active].appearance = Some(appearance);
        }

        if let Some((index, pin, edit_started)) = pending_pin {
            if edit_started {
                self.record_edit();
            }
            let (_, mut appearance) = self.active_appearance();
            if let Some(slot) = appearance.pins.get_mut(index) {
                *slot = pin;
                self.circuits[self.active].appearance = Some(appearance);
            }
        }

        if let Some((index, shape, edit_started)) = pending_shape {
            if edit_started {
                self.record_edit();
            }
            let (_, mut appearance) = self.active_appearance();
            if let Some(slot) = appearance.shapes.get_mut(index) {
                *slot = shape;
                self.circuits[self.active].appearance = Some(appearance);
            }
        }

        if let Some((id, kind)) = pending_kind {
            self.record_edit();
            self.change_kind(id, kind);
        }

        if let Some((wire_id, color)) = pending_wire_color {
            self.record_edit();
            self.color_net(wire_id, color);
        }

        if let Some((id, edited, started)) = pending_properties {
            // Only the frame an editing session begins, so a typed-in name
            // is one undo step rather than one per keystroke.
            if started {
                self.record_edit();
            }
            let mut changed = false;
            if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
                if *placed.properties() != edited {
                    placed.set_properties(edited);
                    self.dirty = true;
                    changed = true;
                }
            }
            // Some properties are *inputs* — a switch's position, a port's
            // resting level, a button's rest state — so editing one changes
            // what the component drives. `set_properties` puts the value in
            // the cell the engine reads, but only an evaluation makes the
            // net notice. Scheduling unconditionally is right: a component
            // whose properties don't reach the engine re-evaluates to the
            // same thing and nothing propagates.
            if changed {
                self.circuit.schedule_now(id);
                self.advance_circuit(SETTLE_TICKS);
            }
        }

        // Declared after the palette and the properties panel so it spans
        // only the canvas, not the whole window: panels claim their space in
        // declaration order, and this bar acts on the canvas alone.
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Two rows: which side of the circuit you're on, then the tools
            // that belong to it. On one line the modes read as three more
            // tools among the tools, when they are the thing deciding which
            // tools there are.
            ui.horizontal(|ui| {
                if let Some(view) = toolbar::show_views(ui, strings, self.view) {
                    self.switch_view(view);
                }
            });
            ui.horizontal(|ui| match self.view {
                toolbar::View::Schematic => {
                    if let Some(tool) = toolbar::show(ui, strings, &self.tool) {
                        self.tool = tool;
                    }
                }
                toolbar::View::Simulation => {
                    if let Some(tool) = toolbar::show_sim_tools(ui, strings, self.sim_tool) {
                        self.sim_tool = tool;
                    }
                }
                toolbar::View::Appearance => {
                    if let Some(tool) = toolbar::show_shape_tools(ui, strings, self.shape_tool) {
                        self.shape_tool = tool;
                        self.drawing = None;
                    }
                    ui.separator();
                    if ui
                        .button(strings.appearance_reset)
                        .on_hover_text(strings.appearance_reset_hint)
                        .clicked()
                    {
                        self.record_edit();
                        self.circuits[self.active].appearance = None;
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // Claim the wheel before `Scene` sees it, so it zooms instead of
            // panning (the schematic-editor convention) — but only while the
            // pointer is over the canvas.
            //
            // Relying on the side panels having consumed it first isn't
            // enough: a scroll area only takes the wheel over its *list*, so
            // an event over the circuit tree's heading, or over a list
            // already scrolled to its end, falls through to here and zooms
            // the schematic while the user is plainly working somewhere else.
            let wheel = if ui.rect_contains_pointer(ui.max_rect()) {
                ui.ctx().input_mut(|i| {
                    let dy = i.smooth_scroll_delta.y;
                    if dy != 0.0 {
                        i.smooth_scroll_delta = egui::Vec2::ZERO;
                    }
                    dy
                })
            } else {
                0.0
            };

            let mut zoom_pivot = None;
            // Copied out and written back so the closure can still borrow
            // the rest of `self`; `Scene` mutates it in place as the user
            // pans and zooms.
            let mut scene_rect = self.scene_rect;
            // The primary drag belongs to the rubber band unless the hand
            // tool is out; the middle button always pans, so there's a way to
            // move the view whatever the tool — and no preference to set.
            // The framed region starts equal to the canvas, so the view opens
            // at 1:1.
            //
            // It used to be a fixed 1200x800, which `Scene` then fitted into
            // whatever space the canvas had — so the circuit opened at
            // roughly 60% and *stayed* there until someone zoomed. That is
            // invisible on line art and obvious the moment there is text:
            // `Scene` applies a layer transform, which scales already-drawn
            // glyphs as a texture rather than re-rasterising them, so any
            // factor other than 1 blurs them. No font-size compensation can
            // fix that — rasterised at `g` and shown at `g × zoom`, the two
            // agree only at zoom 1 — so opening *at* 1 is the fix.
            // Assigned to the *local* copy taken just above, not to
            // `self.scene_rect`: that copy is what `Scene` reads and what
            // gets written back afterwards, so touching the field here would
            // be overwritten a few lines later and the framing would be
            // computed every frame and thrown away every frame.
            let unframed = scene_rect.width() <= 0.0 || scene_rect.height() <= 0.0;
            if std::mem::take(&mut self.refit_view) || unframed {
                let canvas = ui.available_size();
                scene_rect = match self.content_rect() {
                    Some(content) => {
                        let content = content.expand(FIT_MARGIN);
                        // Never magnify: a circuit smaller than the canvas is
                        // centred at 1:1 rather than blown up to fill it,
                        // which would open a two-gate circuit at 4x and blur
                        // every label. Only a drawing too big to fit zooms
                        // out.
                        if content.width() <= canvas.x && content.height() <= canvas.y {
                            egui::Rect::from_center_size(content.center(), canvas)
                        } else {
                            content
                        }
                    }
                    None => egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
                };
            }

            let mut pan_buttons = egui::containers::DragPanButtons::MIDDLE;
            if self.pans_on_left_drag() {
                pan_buttons |= egui::containers::DragPanButtons::PRIMARY;
            }
            let scene_response = egui::Scene::new()
                .zoom_range(MIN_ZOOM..=MAX_ZOOM)
                .drag_pan_buttons(pan_buttons)
                .show(ui, &mut scene_rect, |ui| {
                    // Inside the scene everything is in scene coordinates: the
                    // visible area is the clip rect, and raw pointer positions (which
                    // egui reports globally) have to be mapped in.
                    let canvas_rect = ui.clip_rect();
                    let painter = ui.painter().clone();
                    let to_scene = ui
                        .ctx()
                        .layer_transform_from_global(ui.layer_id())
                        .unwrap_or_default();
                    // Recorded because this is the only place it is known:
                    // it depends on how the panels divided the window and on
                    // what `Scene` then did with what was left. Deriving it
                    // again elsewhere would mean copying `Scene`'s own fitting
                    // rule and drifting from it.
                    #[cfg(test)]
                    {
                        self.canvas_to_screen = to_scene.inverse();
                        self.canvas_layer = Some(ui.layer_id());
                    }
                    // Only the pointer while it's actually over the canvas.
                    // Panels are laid out first, so a click on the palette
                    // still reaches this code -- and, mapped into scene
                    // coordinates, would otherwise read as a canvas click and
                    // (for instance) start a wire under the palette.
                    let pointer_scene = ui
                        .ctx()
                        .pointer_latest_pos()
                        .map(|pos| to_scene * pos)
                        .filter(|pos| canvas_rect.contains(*pos));
                    zoom_pivot = pointer_scene;

                    // The band rides the scene's *own* background response
                    // rather than a widget of its own. A full-canvas
                    // `ui.interact` here covered that background, which is
                    // what placement and panning both go through — so it
                    // silently broke both. The origin is set after the scene
                    // closes (see below); this only paints it and notices the
                    // release, which is what has to happen in here, where
                    // every wire's resolved route is known.
                    let mut band_finished = None;
                    let released = ui.ctx().input(|i| i.pointer.primary_released());
                    if self.bands_on_left_drag() {
                        if let (Some(origin), Some(now)) = (self.band_origin, pointer_scene) {
                            let rect = egui::Rect::from_two_pos(origin, now);
                            let accent = canvas::accent_color(ui.visuals().dark_mode);
                            painter.rect_filled(rect, 0.0, accent.gamma_multiply(0.12));
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(1.0, accent),
                                egui::StrokeKind::Inside,
                            );
                            if released {
                                band_finished = Some(rect);
                            }
                        }
                        if released {
                            self.band_origin = None;
                        }
                    } else {
                        self.band_origin = None;
                    }

                    // Faint enough to stay a background on either theme: the
                    // weak text colour already tracks the background, and the
                    // alpha keeps the dots from competing with the circuit.
                    canvas::draw_grid(
                        &painter,
                        canvas_rect,
                        ui.visuals().weak_text_color().gamma_multiply(0.55),
                    );

                    // The appearance view shares the grid, the camera and the
                    // pointer mapping and nothing else: no components, no
                    // wires, no simulation. Branching here rather than at the
                    // panel keeps all of that in one place.
                    if self.view == toolbar::View::Appearance {
                        self.appearance_ui(ui, &painter, pointer_scene);
                        return;
                    }

                    // The simulation view is the *same* drawing, with every
                    // gesture that edits taken away rather than a copy of it
                    // put on screen. One flag, read by each thing that could
                    // change the document — components stop being draggable,
                    // pins stop starting wires, waypoints and loose ends stop
                    // answering at all, and the keyboard stops deleting.
                    let editing = self.view == toolbar::View::Schematic;

                    // Rotation applies to everything selected. Each turns on
                    // its own centre rather than the group's: a component's
                    // pins have to land on the grid, and turning the group as
                    // one body would put them between dots.
                    let rotate_pressed = editing
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| i.key_pressed(egui::Key::R));

                    // While something is queued for placement, `R` turns
                    // *that* rather than the selection: it is the thing under
                    // your pointer, and rotating after dropping means placing
                    // it wrong first on purpose.
                    if rotate_pressed && matches!(self.tool, Tool::Place(_)) {
                        self.place_rotation = self.place_rotation.next_clockwise();
                    } else if rotate_pressed && !self.selection.components.is_empty() {
                        self.record_edit();
                        let chosen = self.selection.components.clone();
                        for placed in &mut self.placed {
                            if chosen.contains(&placed.id()) {
                                placed.rotate();
                            }
                        }
                    }

                    // Shift turns a click into "add to what's already there",
                    // the gesture every list and canvas uses.
                    let extend_selection = ui.ctx().input(|i| i.modifiers.shift);

                    // Set last frame, so the pin positions below are the ones
                    // this component actually came to rest on.
                    let landed = self.pending_attach.take();

                    // Whether this frame's click landed on something selectable (a
                    // component, a wire, a waypoint, a pin). A click that lands on
                    // nothing is a click on empty canvas, which clears the selection
                    // -- see the end of this closure.
                    let mut click_consumed = false;
                    // Collected during the component loop and acted on after it:
                    // `record_edit` needs all of `self`, which isn't available while
                    // `self.placed` is being iterated mutably.
                    let mut grab_started = false;
                    let mut input_changed = false;
                    // Which selected component the pointer is actually
                    // dragging, and by how much: the rest of the selection is
                    // carried by the same amount once the loop is done, so the
                    // group keeps its shape.
                    let mut group_drag: Option<(ComponentId, egui::Vec2)> = None;
                    let mut group_settled = false;
                    // A switch that wants flipping. Applied after the loop
                    // because its position is document data: the undo
                    // snapshot has to be taken while it still holds the old
                    // one, and `record_edit` needs all of `self`.
                    let mut toggled_switch: Option<ComponentId> = None;
                    let chosen = self.selection.components.clone();

                    let mut pin_handles = Vec::new();
                    for placed in &mut self.placed {
                        let is_selected = self.selection.components.contains(&placed.id());
                        let frame = placed.draw_and_interact(
                            ui,
                            &painter,
                            &mut self.circuit,
                            is_selected,
                            editing,
                        );
                        if let Some(id) = frame.clicked {
                            self.selection.pick_component(id, extend_selection);
                            click_consumed = true;
                        }
                        grab_started |= frame.grab_started;
                        input_changed |= frame.input_changed;
                        if frame.toggled {
                            toggled_switch = Some(placed.id());
                        }
                        if is_selected && frame.dragged_by != egui::Vec2::ZERO {
                            group_drag = Some((placed.id(), frame.dragged_by));
                        }
                        if frame.settled {
                            group_settled |= is_selected;
                            self.pending_attach = Some(placed.id());
                        }
                        pin_handles.extend(frame.pins);
                    }

                    if let Some(id) = toggled_switch {
                        self.record_edit();
                        if let Some(placed) = self.placed.iter_mut().find(|p| p.id() == id) {
                            let mut properties = placed.properties().clone();
                            let now_on = !properties.pressed.unwrap_or(false);
                            // Unset when it's back to the default, so a
                            // project that never touched it keeps saying
                            // nothing — the rule every property follows.
                            properties.pressed = now_on.then_some(true);
                            // `set_properties` pushes it into the cell the
                            // engine reads, so there's one path for a flip
                            // by hand and one by the properties panel.
                            placed.set_properties(properties);
                        }
                        self.circuit.schedule_now(id);
                        input_changed = true;
                    }

                    if let Some((mover, delta)) = group_drag {
                        // Everything but the one under the pointer, which
                        // `interact_box` has already moved.
                        for placed in &mut self.placed {
                            if placed.id() != mover && chosen.contains(&placed.id()) {
                                placed.move_by(delta);
                            }
                        }
                        // A selected wire's own points come along; the ends
                        // that sit on pins follow those pins anyway.
                        for wire in &mut self.wires {
                            if self.selection.wires.contains(&wire.id) {
                                for point in &mut wire.waypoints {
                                    *point += delta;
                                }
                                for end in [&mut wire.from, &mut wire.to] {
                                    if let WireEndpoint::Free(at) = end {
                                        *at += delta;
                                    }
                                }
                            }
                        }
                    }
                    if group_settled {
                        for placed in &mut self.placed {
                            if chosen.contains(&placed.id()) {
                                placed.snap();
                            }
                        }
                        for wire in &mut self.wires {
                            if self.selection.wires.contains(&wire.id) {
                                for point in &mut wire.waypoints {
                                    *point = canvas::snap_to_grid(*point);
                                }
                                for end in [&mut wire.from, &mut wire.to] {
                                    if let WireEndpoint::Free(at) = end {
                                        *at = canvas::snap_to_grid(*at);
                                    }
                                }
                            }
                        }
                    }
                    if grab_started {
                        self.record_edit();
                    }
                    // A button press is runtime state, not an edit: it settles
                    // the circuit but never touches undo.
                    if input_changed {
                        self.advance_circuit(SETTLE_TICKS);
                    }
                    // A pin's current on-canvas position this frame, resolved by
                    // identity -- every `Wire` endpoint that's a pin looks itself up
                    // here rather than storing a position directly, so it tracks a
                    // moved component automatically.
                    let pin_position =
                        |component: ComponentId, pin_index: usize| -> Option<egui::Pos2> {
                            pin_handles
                                .iter()
                                .find(|h| h.component == component && h.pin_index == pin_index)
                                .map(|h| h.position)
                        };

                    let click_pos = ui
                        .ctx()
                        .input(|i| i.pointer.primary_clicked())
                        .then_some(pointer_scene)
                        .flatten();
                    // Double-clicking along an existing wire inserts a new waypoint
                    // right there, so a wire can be reshaped in more places than
                    // just its existing points.
                    // Right-clicking a wire cuts the segment under the pointer.
                    // Both are cut off here rather than at each thing they
                    // reach: a cut, a waypoint inserted, a waypoint removed —
                    // one `editing` at the source is one place to be right.
                    let secondary_click_pos = (editing
                        && ui.ctx().input(|i| i.pointer.secondary_clicked()))
                    .then_some(pointer_scene)
                    .flatten();
                    let double_click_pos = (editing
                        && ui.ctx().input(|i| {
                            i.pointer
                                .button_double_clicked(egui::PointerButton::Primary)
                        }))
                    .then_some(pointer_scene)
                    .flatten();

                    let hover_pos = pointer_scene;

                    // Every wire's endpoints and (possibly-defaulted) waypoint
                    // list, resolved once per frame. A junction depends on its
                    // host being resolved first, and a wire can be re-attached
                    // to any other one, so this repeats until a pass resolves
                    // nothing new rather than assuming creation order. Wires
                    // left unresolved are the genuinely unresolvable ones (a
                    // deleted component, or a tap cycle) and simply aren't drawn.
                    struct Resolved {
                        from: egui::Pos2,
                        to: egui::Pos2,
                        waypoints: Vec<egui::Pos2>,
                    }
                    let mut resolved: HashMap<u64, Resolved> = HashMap::new();
                    let mut progressed = true;
                    while progressed {
                        progressed = false;
                        for wire in &self.wires {
                            if resolved.contains_key(&wire.id) {
                                continue;
                            }
                            // Both ends resolve the same way. A junction may
                            // not be resolvable *yet* (its host can come later
                            // in the list); a later pass picks it up.
                            let place =
                                |endpoint: WireEndpoint, resolved: &HashMap<u64, Resolved>| {
                                    match endpoint {
                                        WireEndpoint::Pin(component, pin_index) => {
                                            pin_position(component, pin_index)
                                        }
                                        WireEndpoint::Junction {
                                            wire: host,
                                            waypoint,
                                        } => resolved
                                            .get(&host)
                                            .and_then(|r| r.waypoints.get(waypoint))
                                            .copied(),
                                        WireEndpoint::Free(pos) => Some(pos),
                                    }
                                };
                            let (Some(from_pos), Some(to_pos)) =
                                (place(wire.from, &resolved), place(wire.to, &resolved))
                            else {
                                continue;
                            };
                            // A wire is exactly the points it was given: no
                            // waypoints means a straight run end to end.
                            //
                            // There used to be an implicit mid-point bend here
                            // for wires drawn without any, back when routing
                            // wasn't under the user's control. It bred phantom
                            // points that only became real once dragged — and
                            // for a level wire it produced *two* of them at the
                            // same spot, which is what left a stray point on top
                            // of an end after cutting a segment.
                            let waypoints = wire.waypoints.clone();
                            resolved.insert(
                                wire.id,
                                Resolved {
                                    from: from_pos,
                                    to: to_pos,
                                    waypoints,
                                },
                            );
                            progressed = true;
                        }
                    }

                    // Where every wire's points ended up this frame, kept past
                    // the loop below: deleting a wire has to know where the taps
                    // on it currently sit so it can leave them there.
                    let resolved_waypoints: HashMap<u64, Vec<egui::Pos2>> = resolved
                        .iter()
                        .map(|(&id, r)| (id, r.waypoints.clone()))
                        .collect();
                    // Likewise where each wire's two ends sit, so deleting a
                    // component can leave its wires loose exactly where they
                    // were attached.
                    let wire_ends: HashMap<u64, (egui::Pos2, egui::Pos2)> = resolved
                        .iter()
                        .map(|(&id, r)| (id, (r.from, r.to)))
                        .collect();

                    // Finishing a new wire on top of another wire's waypoint (a
                    // junction tap) is decided inside the loop below but applied
                    // after it, to keep `self.wires` stable (an unchanging length,
                    // no reallocation) for the whole iteration.
                    let mut junction_finish: Option<JunctionTarget> = None;
                    // The mirror of the above for the *start* of a wire: with the
                    // wire tool, clicking an existing wire begins a new one tapped
                    // onto it rather than merely selecting it.
                    let mut junction_start: Option<(Option<NetId>, JunctionTarget, egui::Pos2)> =
                        None;
                    // Deleting a waypoint mutates the wire list, so it's
                    // decided in the loop and applied after it.
                    let mut waypoint_to_remove: Option<(u64, usize, Vec<egui::Pos2>)> = None;
                    // Likewise for cutting a segment out of a wire.
                    let mut segment_to_cut: Option<(u64, usize, Vec<egui::Pos2>)> = None;
                    // And for joining two wires: it removes one of them, which
                    // would leave this loop indexing past the end.
                    let mut wires_to_join: Option<(u64, bool, u64, bool, egui::Pos2)> = None;

                    // Which net the pointer is over, worked out before any
                    // wire is drawn. Hovering has to light up the *whole*
                    // net: following one conductor across a crossing is the
                    // difficulty, and highlighting the single segment under
                    // the cursor doesn't help with it at all.
                    let hovered_net =
                        hover_pos
                            .filter(|_| self.wiring_from.is_none())
                            .and_then(|pos| {
                                self.wires.iter().find_map(|wire| {
                                    let route = resolved.get(&wire.id)?;
                                    let mut path = vec![route.from];
                                    path.extend(route.waypoints.iter().copied());
                                    path.push(route.to);
                                    if canvas::distance_to_path(pos, &path) < WIRE_HIT_RADIUS {
                                        self.wire_net(wire)
                                    } else {
                                        None
                                    }
                                })
                            });

                    for i in 0..self.wires.len() {
                        let wire_id = self.wires[i].id;
                        let wire_color = self.wires[i].color;
                        // `None` while both ends are loose: the wire is drawing,
                        // not yet a connection. Still very much on screen.
                        let net = self.wire_net(&self.wires[i]);
                        let Some(Resolved {
                            from: from_pos,
                            to: to_pos,
                            waypoints,
                        }) = resolved.remove(&wire_id)
                        else {
                            continue; // Stale, already skipped above.
                        };

                        let user_color =
                            wire_color.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b));
                        // With the signal state showing, the core is the
                        // level and a colour of your own rings it. With the
                        // state hidden the core has nothing left to say, so
                        // the colour takes it over — a casing around a core
                        // that reports nothing is just a thicker wire.
                        let color = if self.show_signal_state {
                            match net {
                                Some(net) => {
                                    let level = canvas::signal_color(
                                        self.circuit.signal_at(net),
                                        ui.visuals().dark_mode,
                                    );
                                    // Faded when nothing but a pass
                                    // transistor is holding it up: the level
                                    // is real, the noise margin is gone, and
                                    // that is worth seeing before it bites.
                                    if self.circuit.is_weakly_driven(net) {
                                        level.gamma_multiply(canvas::WEAK_FADE)
                                    } else {
                                        level
                                    }
                                }
                                None => ui.visuals().weak_text_color(),
                            }
                        } else {
                            user_color.unwrap_or_else(|| match net {
                                Some(_) => ui.visuals().strong_text_color(),
                                None => ui.visuals().weak_text_color(),
                            })
                        };
                        let mut path = vec![from_pos];
                        path.extend(waypoints.iter().copied());
                        path.push(to_pos);

                        // Hovering a wire thickens it, so it's obvious which one a
                        // click is about to select out of several crossing ones.
                        // Wires are polylines, so this is a distance test rather
                        // than an `ui.interact` rect like the widgets use.
                        // On the same net as whatever is under the pointer —
                        // or, for a wire that reaches no pin and so has no
                        // net, under the pointer itself.
                        let is_hovered = self.wiring_from.is_none()
                            && match (net, hovered_net) {
                                (Some(net), Some(hovered)) => net == hovered,
                                _ => hover_pos.is_some_and(|pos| {
                                    canvas::distance_to_path(pos, &path) < WIRE_HIT_RADIUS
                                }),
                            };
                        if is_hovered {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }

                        let is_selected_wire = self.selection.wires.contains(&wire_id);
                        let stroke = if is_selected_wire {
                            egui::Stroke::new(3.0, canvas::accent_color(ui.visuals().dark_mode))
                        } else if is_hovered {
                            egui::Stroke::new(3.0, color.gamma_multiply(1.6))
                        } else {
                            egui::Stroke::new(2.0, color)
                        };

                        // The user's colour goes *underneath*, as a casing:
                        // the signal colour keeps the full width of the core,
                        // so the thing that changes during simulation stays
                        // the thing the eye reads first.
                        // Only while the core carries the level. Once the
                        // colour *is* the core, a casing would be the same
                        // colour twice.
                        if self.show_signal_state {
                            if let Some(casing) = user_color {
                                canvas::draw_path(
                                    &painter,
                                    &path,
                                    egui::Stroke::new(stroke.width + 4.0, casing),
                                );
                            }
                        }
                        canvas::draw_path(&painter, &path, stroke);
                        for &point in &waypoints {
                            painter.circle_filled(point, 3.5, stroke.color);
                        }

                        // Only select an existing wire by clicking on it, or reshape
                        // it by dragging a waypoint, while not actively placing a
                        // new one -- otherwise a click meant to add a waypoint to
                        // the new wire would hijack this one instead.
                        if self.wiring_from.is_none() {
                            if let Some(click) = click_pos {
                                if canvas::distance_to_path(click, &path) < WIRE_HIT_RADIUS {
                                    if self.tool == Tool::Wire {
                                        if let Some((segment, _)) =
                                            canvas::closest_segment(&path, click)
                                        {
                                            let at = canvas::snap_to_grid(click);
                                            let mut inserted = waypoints.clone();
                                            inserted.insert(segment, at);
                                            junction_start = Some((
                                                net,
                                                JunctionTarget::Insert {
                                                    wire: wire_id,
                                                    waypoint: segment,
                                                    waypoints: inserted,
                                                },
                                                at,
                                            ));
                                        }
                                    } else {
                                        self.selection.pick_wire(wire_id, extend_selection);
                                    }
                                    click_consumed = true;
                                }
                            }

                            if let Some(pos) = secondary_click_pos {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, pos)
                                {
                                    if distance < WIRE_HIT_RADIUS {
                                        segment_to_cut = Some((wire_id, segment, path.clone()));
                                    }
                                }
                            }

                            if let Some(dbl_pos) = double_click_pos {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, dbl_pos)
                                {
                                    if distance < 6.0 {
                                        self.record_edit();
                                        self.wires[i]
                                            .waypoints
                                            .insert(segment, canvas::snap_to_grid(dbl_pos));
                                        self.dedupe_waypoints(wire_id);
                                        self.selection.pick_wire(wire_id, extend_selection);
                                        click_consumed = true;
                                    }
                                }
                            }
                        }

                        // Connecting to an existing wire shouldn't mean
                        // aiming at one of its dots: while drawing, a click
                        // anywhere along a wire drops a contact point right
                        // there and finishes on it. Only used if no existing
                        // waypoint claimed the click below, so a deliberate
                        // hit on a dot still reuses that dot.
                        if let (Some(in_progress), Some(click)) = (&self.wiring_from, click_pos) {
                            // Finishing is allowed even onto the same net: the
                            // wire is still a real drawn connection, and refusing
                            // silently just left the wire stuck to the cursor with
                            // no hint as to why. The one thing to refuse is
                            // finishing on the very point the wire started from,
                            // which would be a wire of no length.
                            let starting_here = in_progress.waypoints.is_empty()
                                && matches!(
                                    in_progress.from,
                                    WireEndpoint::Junction { wire: host, .. } if host == wire_id
                                );
                            if !starting_here {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, click)
                                {
                                    if distance < WIRE_HIT_RADIUS {
                                        let mut inserted = waypoints.clone();
                                        inserted.insert(segment, canvas::snap_to_grid(click));
                                        junction_finish = Some(JunctionTarget::Insert {
                                            wire: wire_id,
                                            waypoint: segment,
                                            waypoints: inserted,
                                        });
                                    }
                                }
                            }
                        }

                        // Both ends can be loose (splitting a wire makes one of
                        // each), so both get the same treatment: drawn hollow --
                        // it reads as "attached to nothing", unlike the filled
                        // dots of real waypoints -- with a handle to move it or
                        // drop it back onto something.
                        for (is_from, at) in [(true, from_pos), (false, to_pos)] {
                            let end = if is_from {
                                self.wires[i].from
                            } else {
                                self.wires[i].to
                            };
                            if !matches!(end, WireEndpoint::Free(_)) {
                                continue;
                            }

                            painter.circle_stroke(at, 4.0, stroke);
                            // Still drawn, so a loose end is visible while a
                            // circuit runs — just not something that answers.
                            let response = ui.interact(
                                egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)),
                                egui::Id::new(("wire_end", wire_id, is_from)),
                                if editing {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::hover()
                                },
                            );
                            if response.hovered() {
                                painter.circle_stroke(
                                    at,
                                    7.0,
                                    egui::Stroke::new(
                                        1.5,
                                        canvas::accent_color(ui.visuals().dark_mode),
                                    ),
                                );
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if response.drag_started() {
                                self.record_edit();
                            }
                            if response.dragged() {
                                let end = if is_from {
                                    &mut self.wires[i].from
                                } else {
                                    &mut self.wires[i].to
                                };
                                if let WireEndpoint::Free(pos) = end {
                                    *pos += response.drag_delta();
                                }
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            if response.drag_stopped() {
                                // Dropping a loose end on a pin or on another
                                // wire's point re-attaches it there, which is what
                                // makes a dangling wire worth keeping: swap a gate
                                // out and drag its wires onto the new one instead
                                // of redrawing them. Anywhere else, it just stays
                                // put, snapped to the grid.
                                let current = if is_from {
                                    self.wires[i].from
                                } else {
                                    self.wires[i].to
                                };
                                let dropped = match current {
                                    WireEndpoint::Free(pos) => canvas::snap_to_grid(pos),
                                    _ => at,
                                };

                                // Another wire's loose end: the two are joined
                                // into one rather than one tapping the other,
                                // which is what "these two pieces are the same
                                // wire again" actually means.
                                let onto_loose_end = self
                                    .wires
                                    .iter()
                                    .filter(|w| w.id != wire_id)
                                    .find_map(|other| {
                                        let ends = wire_ends.get(&other.id)?;
                                        [(true, other.from, ends.0), (false, other.to, ends.1)]
                                            .into_iter()
                                            .find(|(_, end, point)| {
                                                matches!(end, WireEndpoint::Free(_))
                                                    && point.distance(dropped) < REATTACH_RADIUS
                                            })
                                            .map(|(other_is_from, _, _)| (other.id, other_is_from))
                                    });

                                if let Some((other_id, other_is_from)) = onto_loose_end {
                                    wires_to_join =
                                        Some((wire_id, is_from, other_id, other_is_from, dropped));
                                    continue;
                                }

                                let onto_pin = pin_handles
                                    .iter()
                                    .find(|h| h.position.distance(dropped) < REATTACH_RADIUS)
                                    .map(|h| (WireEndpoint::Pin(h.component, h.pin_index), h.net));

                                let onto_wire = onto_pin
                                    .is_none()
                                    .then(|| {
                                        resolved_waypoints
                                            .iter()
                                            .filter(|(&id, _)| id != wire_id)
                                            .flat_map(|(&id, points)| {
                                                points
                                                    .iter()
                                                    .enumerate()
                                                    .map(move |(index, &point)| (id, index, point))
                                            })
                                            .find(|(_, _, point)| {
                                                point.distance(dropped) < REATTACH_RADIUS
                                            })
                                            .and_then(|(host, index, _)| {
                                                let net = self
                                                    .wires
                                                    .iter()
                                                    .find(|w| w.id == host)
                                                    .and_then(|w| self.wire_net(w))?;
                                                Some((
                                                    WireEndpoint::Junction {
                                                        wire: host,
                                                        waypoint: index,
                                                    },
                                                    net,
                                                ))
                                            })
                                    })
                                    .flatten();

                                // Anywhere along another wire, not just on one of
                                // its points: drop a contact point there and tap
                                // it, exactly as drawing a wire onto another one
                                // does. Without this, aiming between two points
                                // silently did nothing and the end stayed loose.
                                let onto_path = (onto_pin.is_none() && onto_wire.is_none())
                                    .then(|| {
                                        let mut best: Option<(u64, usize, f32)> = None;
                                        for (&host, points) in &resolved_waypoints {
                                            if host == wire_id {
                                                continue;
                                            }
                                            let Some(&(host_from, host_to)) = wire_ends.get(&host)
                                            else {
                                                continue;
                                            };
                                            let mut host_path = vec![host_from];
                                            host_path.extend(points.iter().copied());
                                            host_path.push(host_to);
                                            let Some((segment, distance)) =
                                                canvas::closest_segment(&host_path, dropped)
                                            else {
                                                continue;
                                            };
                                            if distance < REATTACH_RADIUS
                                                && best.is_none_or(|(_, _, best)| distance < best)
                                            {
                                                best = Some((host, segment, distance));
                                            }
                                        }
                                        best.map(|(host, segment, _)| (host, segment))
                                    })
                                    .flatten();

                                if let Some((host, segment)) = onto_path {
                                    if let Some(host_index) =
                                        self.wires.iter().position(|w| w.id == host)
                                    {
                                        self.wires[host_index].waypoints.insert(segment, dropped);
                                        self.shift_junctions(host, segment, 1);
                                        self.dedupe_waypoints(host);
                                        let endpoint = WireEndpoint::Junction {
                                            wire: host,
                                            waypoint: segment,
                                        };
                                        if is_from {
                                            self.wires[i].from = endpoint;
                                        } else {
                                            self.wires[i].to = endpoint;
                                        }
                                        continue;
                                    }
                                }

                                let endpoint = match onto_pin.or(onto_wire) {
                                    Some((endpoint, _)) => endpoint,
                                    None => WireEndpoint::Free(dropped),
                                };
                                if is_from {
                                    self.wires[i].from = endpoint;
                                } else {
                                    self.wires[i].to = endpoint;
                                }
                                // Re-read the net: this wire may have had none at
                                // all while it dangled, in which case the pin it
                                // just landed on is now its net and there's
                                // nothing to merge.
                            }
                            if response.clicked() {
                                self.selection.pick_wire(wire_id, extend_selection);
                                click_consumed = true;
                            }
                        }

                        for (waypoint_index, &point) in waypoints.iter().enumerate() {
                            let handle_rect =
                                egui::Rect::from_center_size(point, egui::vec2(10.0, 10.0));
                            let response = ui.interact(
                                handle_rect,
                                egui::Id::new(("wire_point", wire_id, waypoint_index)),
                                if editing {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::hover()
                                },
                            );

                            // A waypoint doubles as a junction target, so it gets a
                            // ring on hover the same as a pin -- that's the cue that
                            // you can drop a wire onto it, not just drag it.
                            if response.hovered() {
                                painter.circle_stroke(
                                    point,
                                    6.0,
                                    egui::Stroke::new(
                                        1.5,
                                        canvas::accent_color(ui.visuals().dark_mode),
                                    ),
                                );
                                ui.ctx().set_cursor_icon(if self.wiring_from.is_some() {
                                    egui::CursorIcon::Crosshair
                                } else {
                                    egui::CursorIcon::Grab
                                });
                            }

                            if let Some(in_progress) = &self.wiring_from {
                                // A wire is being drawn: clicking another wire's
                                // waypoint taps into it as a junction, finishing the
                                // new wire here instead of on a pin.
                                let starting_here = in_progress.waypoints.is_empty()
                                    && matches!(
                                        in_progress.from,
                                        WireEndpoint::Junction { wire: host, waypoint }
                                            if host == wire_id && waypoint == waypoint_index
                                    );
                                if response.clicked() && !starting_here {
                                    junction_finish = Some(JunctionTarget::Existing {
                                        wire: wire_id,
                                        waypoint: waypoint_index,
                                    });
                                }
                            } else {
                                if response.drag_started() {
                                    self.record_edit();
                                }
                                if response.dragged() {
                                    self.wires[i].waypoints[waypoint_index] +=
                                        response.drag_delta();
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                                if response.drag_stopped() {
                                    if let Some(p) = self.wires[i].waypoints.get_mut(waypoint_index)
                                    {
                                        *p = canvas::snap_to_grid(*p);
                                    }
                                }
                                if response.clicked() {
                                    self.selection.pick_wire(wire_id, extend_selection);
                                    click_consumed = true;
                                }
                                if response.secondary_clicked() {
                                    waypoint_to_remove =
                                        Some((wire_id, waypoint_index, waypoints.clone()));
                                }
                            }
                        }
                    }

                    // A right-click that landed on a waypoint means "remove
                    // that point", not "cut the segment it happens to sit on".
                    // A pin that came to rest on a loose wire end picks it up —
                    // the mirror of dragging that end onto the pin. No
                    // `record_edit` here: the move (or placement) that brought
                    // it here already took the snapshot, so undoing that undoes
                    // this along with it.
                    if let Some(component) = landed {
                        let mut attach: Vec<(u64, bool, usize, NetId)> = Vec::new();
                        for handle in pin_handles.iter().filter(|h| h.component == component) {
                            for wire in &self.wires {
                                let Some(ends) = wire_ends.get(&wire.id) else {
                                    continue;
                                };
                                for (is_from, end, at) in
                                    [(true, wire.from, ends.0), (false, wire.to, ends.1)]
                                {
                                    let already_taken = attach
                                        .iter()
                                        .any(|&(id, from, _, _)| id == wire.id && from == is_from);
                                    if !already_taken
                                        && matches!(end, WireEndpoint::Free(_))
                                        && at.distance(handle.position) < REATTACH_RADIUS
                                    {
                                        attach.push((
                                            wire.id,
                                            is_from,
                                            handle.pin_index,
                                            handle.net,
                                        ));
                                    }
                                }
                            }
                        }

                        for (wire_id, is_from, pin_index, _) in attach {
                            let Some(index) = self.wires.iter().position(|w| w.id == wire_id)
                            else {
                                continue;
                            };
                            let endpoint = WireEndpoint::Pin(component, pin_index);
                            if is_from {
                                self.wires[index].from = endpoint;
                            } else {
                                self.wires[index].to = endpoint;
                            }
                            self.dirty = true;
                        }
                    }

                    if let Some((keep, keep_is_from, absorb, absorb_is_from, at)) = wires_to_join {
                        self.join_wires(keep, keep_is_from, absorb, absorb_is_from, at);
                        self.dedupe_waypoints(keep);
                    }

                    // Whether the right-click was aimed at a wire, in which case
                    // it shouldn't also clear the selection as a side effect.
                    let consumed_secondary =
                        waypoint_to_remove.is_some() || segment_to_cut.is_some();
                    if let Some((wire_id, index, resolved)) = waypoint_to_remove {
                        self.remove_waypoint(wire_id, index, &resolved);
                    } else if let Some((wire_id, segment, path)) = segment_to_cut {
                        self.split_wire(wire_id, segment, &path);
                    }

                    // Applied here rather than where the band ends, because
                    // this is where every wire's resolved route is known.
                    if let Some(rect) = band_finished {
                        if !extend_selection {
                            self.selection.clear();
                        }
                        for placed in &self.placed {
                            // Its own box: an instance is taller than one
                            // grid cell, and a band that missed it would be
                            // the kind of bug nobody thinks to report.
                            if rect.intersects(placed.rect()) {
                                self.selection.components.insert(placed.id());
                            }
                        }
                        for wire in &self.wires {
                            let ends_inside = wire_ends
                                .get(&wire.id)
                                .is_some_and(|(a, b)| rect.contains(*a) || rect.contains(*b));
                            let points_inside = resolved_waypoints
                                .get(&wire.id)
                                .is_some_and(|points| points.iter().any(|p| rect.contains(*p)));
                            if ends_inside || points_inside {
                                self.selection.wires.insert(wire.id);
                            }
                        }
                        // A band that swept over nothing is still a deliberate
                        // gesture, not a click on empty canvas.
                        click_consumed = true;
                    }

                    let delete_pressed = editing
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| {
                            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                        });
                    if delete_pressed && !self.selection.is_empty() {
                        self.record_edit();

                        let doomed_wires: Vec<u64> = self.selection.wires.iter().copied().collect();
                        if !doomed_wires.is_empty() {
                            self.remove_wires(doomed_wires, &resolved_waypoints);
                        }

                        let doomed = self.selection.components.clone();
                        for &component in &doomed {
                            self.circuit.remove_component(component);
                        }
                        self.placed.retain(|placed| !doomed.contains(&placed.id()));

                        // The wires that touched it are kept, cut loose
                        // where the pin used to be: redrawing them is far
                        // more work than deleting one you didn't want, and
                        // a loose end can be dragged straight onto the
                        // replacement component.
                        for wire in &mut self.wires {
                            for (end, at) in [
                                (&mut wire.from, wire_ends.get(&wire.id).map(|e| e.0)),
                                (&mut wire.to, wire_ends.get(&wire.id).map(|e| e.1)),
                            ] {
                                if matches!(*end, WireEndpoint::Pin(c, _) if doomed.contains(&c)) {
                                    if let Some(at) = at {
                                        *end = WireEndpoint::Free(at);
                                    }
                                }
                            }
                        }
                        self.selection.clear();
                    }

                    // A wire being placed click by click: clicking a pin starts one
                    // (or finishes it, if one's already in progress and this pin is
                    // on a different net); clicking empty canvas along the way adds
                    // a grid-snapped waypoint; Escape cancels it.
                    let clicked_pin =
                        pin_handles
                            .iter()
                            .find(|handle| handle.clicked)
                            .map(|handle| {
                                (
                                    handle.component,
                                    handle.pin_index,
                                    handle.net,
                                    handle.position,
                                )
                            });

                    if let Some((component, pin_index, net, position)) = clicked_pin {
                        click_consumed = true;
                        if let Some(in_progress) = self.wiring_from.take() {
                            if in_progress.net != Some(net) {
                                self.record_edit();
                                self.add_wire(
                                    in_progress.from,
                                    WireEndpoint::Pin(component, pin_index),
                                    in_progress.waypoints,
                                );
                            } else {
                                // Clicked back onto the same net (e.g. the wire's
                                // own start pin) -- not a valid finish, keep going.
                                self.wiring_from = Some(in_progress);
                            }
                        } else {
                            self.wiring_from = Some(WireInProgress {
                                from: WireEndpoint::Pin(component, pin_index),
                                net: Some(net),
                                anchor: position,
                                waypoints: Vec::new(),
                            });
                        }
                    } else if let Some(target) = junction_finish {
                        if let Some(in_progress) = self.wiring_from.take() {
                            self.record_edit();

                            let (host_wire, host_waypoint) = match target {
                                JunctionTarget::Existing { wire, waypoint } => (wire, waypoint),
                                JunctionTarget::Insert {
                                    wire,
                                    waypoint,
                                    waypoints,
                                } => {
                                    if let Some(host) = self.wires.iter_mut().find(|w| w.id == wire)
                                    {
                                        host.waypoints = waypoints;
                                    }
                                    // Points at or past the new one shifted
                                    // along; taps on them have to follow.
                                    self.shift_junctions(wire, waypoint, 1);
                                    (wire, waypoint)
                                }
                            };

                            self.add_wire(
                                in_progress.from,
                                WireEndpoint::Junction {
                                    wire: host_wire,
                                    waypoint: host_waypoint,
                                },
                                in_progress.waypoints,
                            );
                        }
                    } else if let Some(pos) = click_pos {
                        let at = canvas::snap_to_grid(pos);
                        match &mut self.wiring_from {
                            Some(in_progress) => {
                                // Clicking the same grid point twice shouldn't
                                // stack two points there.
                                let last = in_progress.waypoints.last().copied();
                                if last != Some(at) && (last.is_some() || at != in_progress.anchor)
                                {
                                    in_progress.waypoints.push(at);
                                }
                            }
                            // With the wire tool, a click on empty canvas starts
                            // a wire there rather than doing nothing: it begins
                            // on a loose end, which can be dropped onto something
                            // later.
                            None if self.tool == Tool::Wire => {
                                // Started on an existing wire: tap it, so the
                                // new wire is connected from its first click
                                // rather than merely beginning next to it.
                                let start = junction_start.take().map(|(host_net, target, at)| {
                                    let (host, waypoint) = match target {
                                        JunctionTarget::Existing { wire, waypoint } => {
                                            (wire, waypoint)
                                        }
                                        JunctionTarget::Insert {
                                            wire,
                                            waypoint,
                                            waypoints,
                                        } => {
                                            self.record_edit();
                                            if let Some(host) =
                                                self.wires.iter_mut().find(|w| w.id == wire)
                                            {
                                                host.waypoints = waypoints;
                                            }
                                            self.shift_junctions(wire, waypoint, 1);
                                            self.dedupe_waypoints(wire);
                                            (wire, waypoint)
                                        }
                                    };
                                    (
                                        WireEndpoint::Junction {
                                            wire: host,
                                            waypoint,
                                        },
                                        host_net,
                                        at,
                                    )
                                });
                                let (from, net, anchor) =
                                    start.unwrap_or((WireEndpoint::Free(at), None, at));
                                self.wiring_from = Some(WireInProgress {
                                    from,
                                    net,
                                    anchor,
                                    waypoints: Vec::new(),
                                });
                                click_consumed = true;
                            }
                            None => {}
                        }
                    }

                    // Enter ends a wire where the pointer is, leaving that end
                    // loose -- the counterpart to Escape, which throws the whole
                    // wire away. Without it a wire could only ever be finished on
                    // something, which defeats drawing one ahead of what it will
                    // connect to.
                    if self.wiring_from.is_some()
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        if let Some(in_progress) = self.wiring_from.take() {
                            let mut waypoints = in_progress.waypoints;
                            // The last point clicked *becomes* the end, rather
                            // than the wire running on to wherever the pointer
                            // happens to be: the rubber-band segment is a
                            // preview of a click not yet made, so Enter drops
                            // it. With nothing clicked at all there's only the
                            // start point, which is no wire.
                            if let Some(end) = waypoints.pop() {
                                self.record_edit();
                                self.add_wire(in_progress.from, WireEndpoint::Free(end), waypoints);
                            }
                        }
                    }

                    // Right-click is the common "let go of what I'm doing" gesture in
                    // most editors, so it backs out the same as Escape -- left-click
                    // can't double as either, since it's already how a waypoint gets
                    // added. One step at a time: a wire in progress is the innermost
                    // thing to back out of, so it goes first; only once there's no
                    // wire being drawn does the same gesture clear the selection.
                    if !consumed_secondary
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| {
                            i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked()
                        })
                    {
                        if self.wiring_from.is_some() {
                            self.wiring_from = None;
                        } else {
                            self.selection.clear();
                            self.tool = Tool::Select;
                        }
                    }

                    // A click that hit nothing selectable is a click on empty
                    // canvas: clear the selection, the way every schematic/drawing
                    // editor does. Skipped while a wire is being drawn (that click
                    // was a waypoint) or a placement is queued (it's about to drop a
                    // component there).
                    if click_pos.is_some()
                        && !click_consumed
                        && self.wiring_from.is_none()
                        && self.tool == Tool::Select
                    {
                        self.selection.clear();
                    }

                    if let Some(in_progress) = &self.wiring_from {
                        // Scene coordinates, like every other point on the
                        // path: the raw pointer position is global, so using
                        // it directly would send the rubber-band line off to
                        // the wrong place as soon as the view is zoomed or
                        // panned away from 1:1.
                        let pointer_pos = pointer_scene.unwrap_or(in_progress.anchor);
                        let mut preview = vec![in_progress.anchor];
                        preview.extend(in_progress.waypoints.iter().copied());
                        preview.push(pointer_pos);
                        canvas::draw_path(
                            &painter,
                            &preview,
                            egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                        );
                        for &waypoint in &in_progress.waypoints {
                            painter.circle_filled(waypoint, 3.0, ui.visuals().strong_text_color());
                        }
                    }

                    if let Tool::Place(kind) = &self.tool {
                        // A translucent preview of what's about to be dropped, at
                        // the grid position it will actually land on -- otherwise
                        // placing is a blind click.
                        if let Some(pos) = hover_pos {
                            let at = canvas::snap_to_grid(pos);
                            let faint = ui.visuals().strong_text_color().gamma_multiply(0.45);

                            // An instance has no fixed symbol: its box is
                            // generated from the ports of the circuit it
                            // refers to, so the preview has to be generated
                            // the same way -- `symbol::draw` has nothing to
                            // show for one, which is why there was no ghost.
                            if let Some(path) = kind.circuit_path() {
                                let ports: Vec<_> = self
                                    .circuits
                                    .iter()
                                    .find(|circuit| circuit.path() == path)
                                    .map(|saved| {
                                        Self::port_slots(saved)
                                            .into_iter()
                                            .map(|(_, port)| port)
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                // Through `appearance_of` like the real
                                // instance, so the ghost shows the circuit's
                                // own symbol when it has one.
                                let appearance = self.appearance_of(path, &ports);
                                crate::symbol::draw_instance(
                                    &painter,
                                    at,
                                    self.place_rotation,
                                    faint,
                                    path,
                                    &ports,
                                    &appearance,
                                    &crate::symbol::TextLayer::for_ui(ui),
                                );
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                                return;
                            }

                            let rect = egui::Rect::from_center_size(at, BOX_SIZE);
                            crate::symbol::draw(
                                &painter,
                                kind,
                                rect,
                                self.place_rotation,
                                ui.visuals().strong_text_color().gamma_multiply(0.45),
                                crate::symbol::SymbolState::default(),
                                &crate::symbol::TextLayer::for_ui(ui),
                            );
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                        }
                    }
                });

            self.scene_rect = scene_rect;

            // A drag that the scene's background saw is a drag on empty
            // canvas -- everything else would have claimed it first. Only the
            // primary button: the middle one is still panning.
            if self.bands_on_left_drag()
                && scene_response
                    .response
                    .drag_started_by(egui::PointerButton::Primary)
            {
                self.band_origin = scene_response.response.interact_pointer_pos();
            }

            // Placing goes through the scene's own background response, so a
            // click that landed on a component or a wire never also drops a
            // new component underneath it.
            if let Tool::Place(kind) = self.tool.clone() {
                if scene_response.response.clicked() {
                    if let Some(pos) = scene_response.response.interact_pointer_pos() {
                        self.record_edit();
                        let id = self.place(kind, canvas::snap_to_grid(pos));
                        if let Some(placed) = self.placed.iter_mut().find(|p| p.id() == id) {
                            placed.set_rotation(self.place_rotation);
                        }
                        self.pending_attach = Some(id);
                        // Holding shift keeps the kind loaded, so a row of
                        // LEDs is one trip to the palette rather than one per
                        // component. Releasing it drops back to selecting,
                        // which is what you want after the last one.
                        if !ui.ctx().input(|i| i.modifiers.shift) {
                            self.tool = Tool::Select;
                        }
                    }
                }
            }

            // Every edit above changed the drawing, never the nets: they're
            // recomputed here, once, from whatever the drawing now says.
            let fingerprint = self.connectivity_fingerprint();
            if fingerprint != self.net_fingerprint {
                self.net_fingerprint = fingerprint;
                self.rebuild_nets();
                self.advance_circuit(SETTLE_TICKS);
            }

            // Wheel zoom, applied to the framed region for the next frame:
            // shrinking it zooms in. Anchored on the pointer so the point
            // under the cursor stays put, which is what makes zooming feel
            // like it's following you rather than the window centre.
            if wheel != 0.0 {
                let pivot = zoom_pivot.unwrap_or_else(|| self.scene_rect.center());
                let factor = (-wheel * WHEEL_ZOOM_SENSITIVITY).exp();
                self.scene_rect = egui::Rect::from_min_max(
                    pivot + (self.scene_rect.min - pivot) * factor,
                    pivot + (self.scene_rect.max - pivot) * factor,
                );
            }
        });

        // The unsaved-changes gate. Modal on purpose: it's answering "what
        // happens to your work", so it shouldn't be possible to click past
        // it and forget it's there.
        if let Some(action) = self.pending_action.clone() {
            egui::Modal::new(egui::Id::new("confirm_discard")).show(ui.ctx(), |ui| {
                ui.heading(strings.confirm_discard_title);
                ui.label(strings.confirm_discard_body);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(strings.confirm_discard_save).clicked() {
                        // Only go through with the action if the save
                        // actually happened -- cancelling the file dialog
                        // must not quietly discard the work anyway.
                        if self.save_project() {
                            self.pending_action = None;
                            self.run_action(action.clone(), ui.ctx());
                        }
                    }
                    if ui.button(strings.confirm_discard_discard).clicked() {
                        self.dirty = false;
                        self.pending_action = None;
                        self.run_action(action, ui.ctx());
                    }
                    if ui.button(strings.confirm_discard_cancel).clicked() {
                        self.pending_action = None;
                    }
                });
            });
        }

        crate::help::show(ui.ctx(), strings, &mut self.show_shortcuts);
        crate::licenses::show(ui.ctx(), strings, &mut self.licenses);

        egui::Window::new(strings.about_title)
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    // Drawn, not a scaled-down copy of the window icon: it's
                    // line work, and a 256 px bitmap shrunk to this would be
                    // soft for no reason.
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(76.0, 76.0), egui::Sense::hover());
                    crate::icon::paint(ui.painter(), rect);

                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.heading("SimLogix");
                        ui.label(
                            egui::RichText::new(
                                strings
                                    .about_version
                                    .replace("{}", env!("CARGO_PKG_VERSION")),
                            )
                            .weak(),
                        );
                        ui.add_space(8.0);
                        ui.label(strings.about_body);
                        ui.label(egui::RichText::new(strings.about_built_with).weak());
                        ui.add_space(4.0);
                        // The terms are two clicks away rather than spelled
                        // out here: About says what this is, the licence
                        // window says what you may do with it.
                        if ui.link(strings.about_license).clicked() {
                            self.licenses.open = true;
                        }
                    });
                    ui.add_space(4.0);
                });
                ui.add_space(4.0);
            });

        let mut error_open = self.error.is_some();
        if let Some(message) = &self.error {
            egui::Window::new(strings.error_title)
                .open(&mut error_open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(message);
                });
        }
        if !error_open {
            self.error = None;
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

// Tests that drive the assembled application. Declared here rather than as
// a sibling module so they can see what everything else may not; the file is
// its own, and its docs say why.
#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_a_circuit_opens_it_without_disturbing_the_others() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));

        app.create_circuit(String::new());

        assert_eq!(app.circuits.len(), 2);
        assert_eq!(app.active, 1);
        // The new circuit starts empty...
        assert!(app.placed.is_empty());
        // ...and the one left behind kept what was drawn in it.
        assert_eq!(app.circuits[0].components.len(), 1);
    }

    #[test]
    fn switching_keeps_each_circuit_to_its_own_layout() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
        app.create_circuit(String::new());
        app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));

        app.switch_to(0);
        assert_eq!(app.placed.len(), 1);
        assert_eq!(app.placed[0].kind(), ComponentKind::Button);

        app.switch_to(1);
        assert_eq!(app.placed.len(), 1);
        assert_eq!(app.placed[0].kind(), ComponentKind::Led);
    }

    #[test]
    fn a_new_circuit_never_takes_a_name_already_in_use() {
        let mut app = SimLogixApp::default();
        app.create_circuit(String::new());
        app.create_circuit(String::new());

        let names: Vec<&str> = app
            .circuits
            .iter()
            .map(|circuit| circuit.name.as_str())
            .collect();
        let distinct: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(
            names.len(),
            distinct.len(),
            "duplicate name among {names:?}"
        );
    }

    #[test]
    fn deleting_the_open_circuit_falls_onto_the_one_taking_its_place() {
        let mut app = SimLogixApp::default();
        app.create_circuit(String::new());
        app.create_circuit(String::new());
        app.switch_to(1);

        app.delete_circuit(1);

        assert_eq!(app.circuits.len(), 2);
        // Index 1 now holds what used to sit at 2.
        assert_eq!(app.active, 1);
    }

    #[test]
    fn deleting_a_circuit_before_the_open_one_keeps_the_same_one_open() {
        let mut app = SimLogixApp::default();
        app.create_circuit(String::new());
        app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));
        let open = app.circuits[app.active].name.clone();

        app.delete_circuit(0);

        assert_eq!(app.active, 0);
        assert_eq!(app.circuits[0].name, open);
        assert_eq!(app.placed.len(), 1);
    }

    #[test]
    fn the_last_circuit_cannot_be_deleted() {
        let mut app = SimLogixApp::default();
        app.delete_circuit(0);
        assert_eq!(app.circuits.len(), 1);
    }

    #[test]
    fn renaming_onto_a_name_already_in_use_is_refused() {
        let mut app = SimLogixApp::default();
        let taken = app.circuits[0].name.clone();
        app.create_circuit(String::new());

        app.rename_circuit(1, &taken);

        assert_ne!(app.circuits[1].name, taken);
        assert!(app.error.is_some(), "the clash should be reported");
    }

    #[test]
    fn undoing_a_new_circuit_goes_back_to_the_project_before_it() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
        app.create_circuit(String::new());

        app.undo();

        assert_eq!(app.circuits.len(), 1);
        assert_eq!(app.placed.len(), 1);
    }

    #[test]
    fn the_library_is_named_after_the_file_once_and_then_stops_following_it() {
        let mut app = SimLogixApp::default();
        assert!(app.library.is_empty());

        app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));
        assert_eq!(app.library, "cpu");

        // Saved somewhere else, or the file renamed: the library name is
        // what other projects refer to these circuits by, so it must not
        // move underneath them.
        app.name_library_after(std::path::Path::new("/tmp/cpu-backup.slgx"));
        assert_eq!(app.library, "cpu");
    }

    #[test]
    fn the_library_name_survives_undo_of_a_later_edit() {
        let mut app = SimLogixApp::default();
        app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));
        app.create_circuit(String::new());

        app.undo();

        assert_eq!(app.library, "cpu");
    }

    #[test]
    fn renaming_the_project_is_undoable_and_refuses_an_empty_name() {
        let mut app = SimLogixApp::default();
        app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));

        app.rename_project("   ");
        assert_eq!(app.library, "cpu", "an empty name is not a name");

        app.rename_project("alu");
        assert_eq!(app.library, "alu");
        app.undo();
        assert_eq!(app.library, "cpu");
    }

    #[test]
    fn the_simulation_view_takes_the_editing_tools_away() {
        let mut app = SimLogixApp {
            tool: Tool::Wire,
            ..Default::default()
        };

        app.switch_view(toolbar::View::Simulation);

        // A tool that places or wires would be a gesture the mode has just
        // promised not to have, left armed from before the switch.
        assert_eq!(app.tool, Tool::Select);
        assert!(app.selection.is_empty());
        // The band is a schematic gesture too; only the hand gives the
        // primary button back to the view here.
        assert!(!app.bands_on_left_drag());
        assert!(!app.pans_on_left_drag());
        app.sim_tool = toolbar::SimTool::Pan;
        assert!(app.pans_on_left_drag());
    }

    #[test]
    fn a_left_drag_does_nothing_schematic_while_a_symbol_is_being_drawn() {
        let mut app = SimLogixApp {
            tool: Tool::Marquee,
            ..Default::default()
        };
        assert!(app.bands_on_left_drag());

        app.switch_view(toolbar::View::Appearance);

        // Both false, whatever the tool or the preference was: the primary
        // button belongs to the drawing here, and the middle one still pans.
        // The band is started from the scene's background response, outside
        // the appearance view's own code, so gating it there is what stops a
        // selection rectangle being swept under every line traced.
        assert!(!app.bands_on_left_drag());
        assert!(!app.pans_on_left_drag());

        app.left_drag_pans = true;
        assert!(!app.pans_on_left_drag());
    }

    #[test]
    fn each_view_keeps_its_own_camera() {
        let mut app = SimLogixApp::default();
        let schematic =
            egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(400.0, 300.0));
        app.scene_rect = schematic;

        app.switch_view(toolbar::View::Appearance);
        // A symbol always sits on the origin while a schematic sits wherever
        // it was drawn, so carrying one camera across would land nowhere.
        assert_ne!(app.scene_rect, schematic);

        app.switch_view(toolbar::View::Schematic);
        assert_eq!(app.scene_rect, schematic, "and it comes back untouched");
    }

    #[test]
    fn the_appearance_view_frames_the_symbol_rather_than_the_drawing() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::InputPort, egui::pos2(600.0, 600.0));
        app.switch_view(toolbar::View::Appearance);

        let framed = app.content_rect().expect("a symbol always has an extent");
        // Around the origin, where a symbol is drawn — not on the schematic
        // 600 away, which is what would put the symbol off screen. Not
        // exactly *on* the origin: a box with a pin down one side only is
        // not symmetric about it, and needn't be.
        assert!(framed.center().to_vec2().length() < canvas::BOX_SIZE.x);
        assert!(!framed.contains(egui::pos2(600.0, 600.0)));
    }

    #[test]
    fn a_circuit_with_a_symbol_of_its_own_stops_showing_the_generated_box() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::InputPort, egui::pos2(40.0, 40.0));
        app.rename_circuit(0, "sub");
        // A symbol reaching further than the generated box would, so the
        // instance's own rect has to come from the symbol rather than from
        // the port count.
        app.circuits[0].appearance = Some(crate::appearance::Appearance {
            shapes: vec![crate::appearance::Shape::Circle {
                center: (0.0, 0.0),
                radius: 90.0,
            }],
            pins: vec![crate::appearance::PinSlot {
                at: (-90.0, 0.0),
                facing: crate::appearance::Facing::Left,
                lead: 10.0,
                show_name: true,
            }],
            show_name: true,
        });

        app.create_circuit(String::new());
        let instance = app.place(
            ComponentKind::Circuit("sub".to_string()),
            egui::pos2(200.0, 200.0),
        );

        let placed = app
            .placed
            .iter()
            .find(|placed| placed.id() == instance)
            .expect("just placed");
        assert_eq!(placed.rect().width(), 180.0, "the symbol decides its size");
    }

    /// Builds `outer` containing an instance of `inner`, each with one input
    /// port and one output port, and returns the app with `main` open and an
    /// instance of `outer` placed in it.
    ///
    /// Two levels deep on purpose: one level has worked since instances
    /// existed, and that is exactly what hid this.
    fn nested_inverters(depth: usize) -> (SimLogixApp, ComponentId, ComponentId) {
        let mut app = SimLogixApp::default();

        // The innermost circuit: in → NOT → out.
        app.rename_circuit(0, "level0");
        let input = app.place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
        let gate = app.place(ComponentKind::Not, egui::pos2(80.0, 0.0));
        let output = app.place(ComponentKind::OutputPort, egui::pos2(160.0, 0.0));
        app.add_wire(
            WireEndpoint::Pin(input, 0),
            WireEndpoint::Pin(gate, 0),
            Vec::new(),
        );
        app.add_wire(
            WireEndpoint::Pin(gate, 1),
            WireEndpoint::Pin(output, 0),
            Vec::new(),
        );

        // Each further level wraps the one below in ports of its own.
        for level in 1..depth {
            app.create_circuit(String::new());
            app.rename_circuit(app.active, &format!("level{level}"));
            let input = app.place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
            let inside = app.place(
                ComponentKind::Circuit(format!("level{}", level - 1)),
                egui::pos2(80.0, 0.0),
            );
            let output = app.place(ComponentKind::OutputPort, egui::pos2(160.0, 0.0));
            app.add_wire(
                WireEndpoint::Pin(input, 0),
                WireEndpoint::Pin(inside, 0),
                Vec::new(),
            );
            app.add_wire(
                WireEndpoint::Pin(inside, 1),
                WireEndpoint::Pin(output, 0),
                Vec::new(),
            );
        }

        // And a circuit that drives the outermost one from a switch.
        app.create_circuit(String::new());
        let switch = app.place(ComponentKind::Switch, egui::pos2(0.0, 200.0));
        let instance = app.place(
            ComponentKind::Circuit(format!("level{}", depth - 1)),
            egui::pos2(120.0, 200.0),
        );
        app.add_wire(
            WireEndpoint::Pin(switch, 0),
            WireEndpoint::Pin(instance, 0),
            Vec::new(),
        );
        app.rebuild_nets();
        app.advance_circuit(SETTLE_TICKS);
        (app, switch, instance)
    }

    #[test]
    fn one_level_of_nesting_inverts() {
        let (app, _, instance) = nested_inverters(1);
        let out = app.circuit.pins(instance)[1].net;
        // The switch rests off, so the inverter's output is high.
        assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);
    }

    #[test]
    fn a_sub_circuit_inside_a_sub_circuit_stays_connected() {
        // Romain's `and` is a `nand` followed by a `not`, so its instances
        // are two levels deep — and at two levels the inner circuit's own
        // wiring went missing, leaving the output driven by nobody.
        let (app, _, instance) = nested_inverters(2);
        let out = app.circuit.pins(instance)[1].net;
        assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);
    }

    #[test]
    fn nesting_has_no_depth_limit() {
        // Eight levels, which is well past anything that could be special
        // cased. Each level only wraps the one below in ports of its own, so
        // there is still exactly one inverter at the bottom however deep it
        // is buried — and that is the point: the depth must cost nothing but
        // depth.
        let (mut app, switch, instance) = nested_inverters(8);
        let out = app.circuit.pins(instance)[1].net;
        assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);

        // Driven, not merely connected: a net can be `High` because
        // something reaches it, and the only proof it is the *input* that
        // reaches it is that changing the input changes it.
        let flipped = Properties {
            pressed: Some(true),
            ..Default::default()
        };
        if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == switch) {
            placed.set_properties(flipped);
        }
        app.circuit.schedule_now(switch);
        app.advance_circuit(SETTLE_TICKS);

        assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::Low);
    }

    #[test]
    fn a_sub_circuit_joining_two_ports_and_nothing_else_still_passes_through() {
        let mut app = SimLogixApp::default();
        // The sub-circuit is a plain feed-through: an input port wired
        // straight to an output port, with no component in between. That
        // inner net has no pin of its own to hang the connection on, which
        // is what made this the one shape that silently went dead.
        let input = app.place(ComponentKind::InputPort, egui::pos2(40.0, 40.0));
        let output = app.place(ComponentKind::OutputPort, egui::pos2(120.0, 40.0));
        app.add_wire(
            WireEndpoint::Pin(input, 0),
            WireEndpoint::Pin(output, 0),
            Vec::new(),
        );
        app.rename_circuit(0, "buf");

        app.create_circuit(String::new());
        let instance = app.place(
            ComponentKind::Circuit("buf".to_string()),
            egui::pos2(200.0, 200.0),
        );
        app.rebuild_nets();

        let pins = app.circuit.pins(instance);
        assert_eq!(pins.len(), 2, "one pin per port");
        assert_eq!(
            pins[0].net, pins[1].net,
            "the two sides of a feed-through are one net, so whatever is \
             wired to one arrives at the other"
        );
    }

    /// A project holding a circuit named `sub`, and — open — a second one
    /// containing an instance of it.
    fn project_with_an_instance() -> SimLogixApp {
        let mut app = SimLogixApp::default();
        app.rename_circuit(0, "sub");
        app.create_circuit(String::new());
        app.place(
            ComponentKind::Circuit("sub".to_string()),
            egui::pos2(80.0, 80.0),
        );
        app
    }

    /// What the open circuit's single instance names.
    fn instance_path(app: &SimLogixApp) -> ComponentKind {
        app.placed[0].kind()
    }

    #[test]
    fn renaming_a_circuit_carries_the_instances_of_it_along() {
        let mut app = project_with_an_instance();

        app.rename_circuit(0, "adder");

        // A reference is a path, so without repointing this instance would
        // still name a circuit that no longer exists — silently, since
        // nothing about the drawing changes.
        assert_eq!(
            instance_path(&app),
            ComponentKind::Circuit("adder".to_string())
        );
    }

    #[test]
    fn re_filing_a_circuit_carries_the_instances_of_it_along() {
        let mut app = project_with_an_instance();
        app.create_folder("");
        let folder = app.folders[0].clone();

        app.move_circuit(0, folder.clone());

        // The folder is part of the address, so moving changes the path
        // exactly as a rename does.
        assert_eq!(
            instance_path(&app),
            ComponentKind::Circuit(format!("{folder}/sub"))
        );
    }

    #[test]
    fn renaming_a_folder_carries_instances_of_everything_inside_it() {
        let mut app = project_with_an_instance();
        app.folders = vec!["alu".to_string()];
        app.circuits[0].folder = "alu".to_string();
        // The instance was placed before the move, so point it at where the
        // circuit is now, as a real session would have.
        app.repoint_instances("sub", "alu/sub");

        app.rename_folder("alu", "arith");

        // Nothing about this instance's own circuit was touched — it is
        // carried by a rename two levels away from it.
        assert_eq!(
            instance_path(&app),
            ComponentKind::Circuit("arith/sub".to_string())
        );
    }

    #[test]
    fn an_instance_in_a_circuit_that_is_not_open_is_repointed_too() {
        let mut app = project_with_an_instance();
        // Close the circuit holding the instance by opening the other one.
        app.switch_to(0);

        app.rename_circuit(0, "adder");

        assert_eq!(
            app.circuits[1].components[0].kind,
            ComponentKind::Circuit("adder".to_string()),
            "a closed circuit is only in its saved form, and is just as \
             capable of holding a reference"
        );
    }

    #[test]
    fn deleting_a_circuit_something_still_uses_is_refused() {
        let mut app = project_with_an_instance();

        app.delete_circuit(0);

        // Unlike a rename there is no new path to offer, so the only
        // alternative to refusing is leaving the instance naming nothing.
        assert_eq!(app.circuits.len(), 2);
        assert!(app.error.is_some());
    }

    #[test]
    fn deleting_a_circuit_nothing_uses_is_allowed() {
        let mut app = project_with_an_instance();
        app.create_circuit(String::new());
        let spare = app.circuits.len() - 1;

        app.delete_circuit(spare);

        assert_eq!(app.circuits.len(), 2);
        assert!(app.error.is_none());
    }

    #[test]
    fn renaming_a_folder_carries_everything_filed_under_it() {
        let mut app = SimLogixApp {
            folders: vec![
                "alu".to_string(),
                "alu/decode".to_string(),
                // Shares a prefix with "alu" but is not inside it. This is
                // the one a naive `starts_with` gets wrong.
                "alu2".to_string(),
            ],
            ..Default::default()
        };
        app.circuits[0].folder = "alu/decode".to_string();

        app.rename_folder("alu", "arith");

        assert_eq!(
            app.folders,
            vec!["arith", "arith/decode", "alu2"],
            "only what was inside should move"
        );
        assert_eq!(app.circuits[0].folder, "arith/decode");
    }

    #[test]
    fn deleting_a_folder_lifts_its_contents_rather_than_taking_them_along() {
        let mut app = SimLogixApp {
            folders: vec!["alu".to_string(), "alu/decode".to_string()],
            ..Default::default()
        };
        app.circuits[0].folder = "alu/decode".to_string();
        app.create_circuit("alu".to_string());
        let lifted = app.circuits.len() - 1;

        app.delete_folder("alu");

        assert_eq!(app.folders, vec!["decode"]);
        // Filing something away is a presentation choice; undoing it must
        // not be able to take circuits with it.
        assert_eq!(app.circuits[0].folder, "decode");
        assert_eq!(app.circuits[lifted].folder, "");
    }

    #[test]
    fn a_new_folder_never_takes_a_path_already_in_use() {
        let mut app = SimLogixApp::default();
        app.create_folder("");
        app.create_folder("");
        app.create_folder("");

        let distinct: std::collections::HashSet<&String> = app.folders.iter().collect();
        assert_eq!(distinct.len(), 3, "got {:?}", app.folders);
    }

    #[test]
    fn moving_a_circuit_changes_where_it_is_filed_but_not_its_name() {
        let mut app = SimLogixApp::default();
        app.create_folder("");
        let folder = app.folders[0].clone();
        let name = app.circuits[0].name.clone();

        app.move_circuit(0, folder.clone());

        assert_eq!(app.circuits[0].folder, folder);
        // The whole point of folders being presentation: a reference to this
        // circuit is `library:name`, and filing it hasn't touched that.
        assert_eq!(app.circuits[0].name, name);

        app.undo();
        assert_eq!(app.circuits[0].folder, "");
    }

    #[test]
    fn a_folder_rename_refuses_a_path_separator() {
        let mut app = SimLogixApp {
            folders: vec!["alu".to_string()],
            ..Default::default()
        };

        // This would move the folder rather than rename it, which isn't
        // what the gesture says it does.
        app.rename_folder("alu", "fpu/inner");

        assert_eq!(app.folders, vec!["alu"]);
    }

    #[test]
    fn two_folders_can_each_hold_a_circuit_of_the_same_name() {
        // What choosing `library:folder/name` as the reference buys: the
        // folder is part of what identifies a circuit, so the name only has
        // to be distinct within it.
        let mut app = SimLogixApp {
            folders: vec!["alu".to_string(), "fpu".to_string()],
            ..Default::default()
        };
        app.create_circuit("alu".to_string());
        app.rename_circuit(app.active, "adder");
        app.create_circuit("fpu".to_string());
        app.rename_circuit(app.active, "adder");

        let filed: Vec<(&str, &str)> = app
            .circuits
            .iter()
            .map(|circuit| (circuit.folder.as_str(), circuit.name.as_str()))
            .collect();
        assert!(filed.contains(&("alu", "adder")), "got {filed:?}");
        assert!(filed.contains(&("fpu", "adder")), "got {filed:?}");
    }

    #[test]
    fn moving_onto_a_name_already_in_that_folder_is_refused() {
        let mut app = SimLogixApp {
            folders: vec!["alu".to_string()],
            ..Default::default()
        };
        app.rename_circuit(0, "adder");
        app.move_circuit(0, "alu".to_string());
        app.create_circuit(String::new());
        let second = app.active;
        app.rename_circuit(second, "adder");

        app.move_circuit(second, "alu".to_string());

        assert_eq!(
            app.circuits[second].folder, "",
            "the move should not happen"
        );
        assert!(app.error.is_some(), "the clash should be reported");
    }

    #[test]
    fn lifting_two_circuits_of_the_same_name_into_one_folder_renames_the_second() {
        // Deleting a folder must not be blocked by a name collision, so the
        // one that lands second gets a free name instead.
        let mut app = SimLogixApp {
            folders: vec!["alu".to_string()],
            ..Default::default()
        };
        app.rename_circuit(0, "adder");
        app.create_circuit("alu".to_string());
        let filed = app.active;
        app.rename_circuit(filed, "adder");

        app.delete_folder("alu");

        assert!(app.folders.is_empty());
        let names: Vec<&str> = app
            .circuits
            .iter()
            .map(|circuit| circuit.name.as_str())
            .collect();
        let distinct: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(distinct.len(), names.len(), "got {names:?}");
    }

    #[test]
    fn properties_survive_a_save_and_load() {
        let mut app = SimLogixApp::default();
        let id = app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));
        if let Some(placed) = app.placed.iter_mut().find(|placed| placed.id() == id) {
            placed.set_properties(Properties {
                name: Some("status".to_string()),
                color: Some([0, 200, 0]),
                ..Default::default()
            });
        }

        let project = app.to_project();
        let reloaded = SimLogixApp::from_project(&project, 0);

        let properties = reloaded.placed[0].properties();
        assert_eq!(properties.label(), Some("status"));
        assert_eq!(properties.color, Some([0, 200, 0]));
    }

    #[test]
    fn a_component_with_nothing_set_writes_no_properties_at_all() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));

        let json = serde_json::to_string(&app.to_project()).expect("serializes");

        // The whole point of every property being optional: a project that
        // never touched them looks exactly as it did before they existed.
        assert!(!json.contains("properties"), "got {json}");
    }

    /// Two wires end to end, so they share a net: colouring one has to
    /// colour both, because a net is one conductor.
    fn two_wires_one_net() -> (SimLogixApp, u64, u64) {
        let mut app = SimLogixApp::default();
        let button = app.place(ComponentKind::Button, egui::pos2(0.0, 0.0));
        let led = app.place(ComponentKind::Led, egui::pos2(200.0, 0.0));

        let first = app.add_wire(
            WireEndpoint::Pin(button, 0),
            WireEndpoint::Junction {
                wire: 0,
                waypoint: 0,
            },
            vec![egui::pos2(100.0, 0.0)],
        );
        let second = app.add_wire(
            WireEndpoint::Junction {
                wire: first,
                waypoint: 0,
            },
            WireEndpoint::Pin(led, 0),
            Vec::new(),
        );
        // The first wire's junction was a placeholder until the second one
        // existed; point it at a real host now.
        if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == first) {
            wire.from = WireEndpoint::Pin(button, 0);
        }
        app.rebuild_nets();
        (app, first, second)
    }

    #[test]
    fn colouring_one_wire_colours_the_whole_net() {
        let (mut app, first, second) = two_wires_one_net();

        app.color_net(first, Some([10, 20, 30]));

        let color_of = |app: &SimLogixApp, id: u64| {
            app.wires
                .iter()
                .find(|wire| wire.id == id)
                .and_then(|wire| wire.color)
        };
        assert_eq!(color_of(&app, first), Some([10, 20, 30]));
        assert_eq!(
            color_of(&app, second),
            Some([10, 20, 30]),
            "the net is one conductor, so it is one colour"
        );
    }

    #[test]
    fn a_wire_joining_a_coloured_net_inherits_its_colour() {
        let (mut app, first, second) = two_wires_one_net();
        app.color_net(first, Some([10, 20, 30]));

        // A fresh wire onto the same net -- it starts with no colour of its
        // own, and picking it up is what keeps "the net is coloured" true.
        let added = app.add_wire(
            WireEndpoint::Junction {
                wire: second,
                waypoint: 0,
            },
            WireEndpoint::Free(egui::pos2(300.0, 40.0)),
            Vec::new(),
        );
        app.rebuild_nets();

        let added_color = app
            .wires
            .iter()
            .find(|wire| wire.id == added)
            .and_then(|wire| wire.color);
        assert_eq!(added_color, Some([10, 20, 30]));
    }

    #[test]
    fn joining_two_differently_coloured_nets_keeps_both_colours() {
        let (mut app, first, second) = two_wires_one_net();
        // Force a disagreement, which is what happens when two coloured nets
        // are joined: no winner is picked, because a silent choice is worse
        // than a visibly two-tone net the user can re-colour.
        if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == first) {
            wire.color = Some([1, 1, 1]);
        }
        if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == second) {
            wire.color = Some([2, 2, 2]);
        }

        app.rebuild_nets();

        let colors: Vec<Option<[u8; 3]>> = app.wires.iter().map(|wire| wire.color).collect();
        assert!(colors.contains(&Some([1, 1, 1])), "got {colors:?}");
        assert!(colors.contains(&Some([2, 2, 2])), "got {colors:?}");
    }

    #[test]
    fn a_wire_colour_survives_a_save_and_load() {
        let (mut app, first, _) = two_wires_one_net();
        app.color_net(first, Some([7, 8, 9]));

        let project = app.to_project();
        let reloaded = SimLogixApp::from_project(&project, 0);

        assert!(reloaded
            .wires
            .iter()
            .all(|wire| wire.color == Some([7, 8, 9])));
    }

    #[test]
    fn changing_a_component_to_its_sibling_kind_keeps_its_wires_and_place() {
        let mut app = SimLogixApp::default();
        let transistor = app.place(ComponentKind::NTransistor, egui::pos2(60.0, 60.0));
        let led = app.place(ComponentKind::Led, egui::pos2(200.0, 60.0));
        app.add_wire(
            WireEndpoint::Pin(transistor, 2),
            WireEndpoint::Pin(led, 0),
            vec![egui::pos2(140.0, 60.0)],
        );

        app.change_kind(transistor, ComponentKind::PTransistor);

        // The rebuild hands out fresh ids, so what's checked is that the
        // drawing survived: same components in the same places, same wire,
        // same route, and the selection still on the thing being edited.
        assert_eq!(app.placed.len(), 2);
        assert_eq!(app.placed[0].kind(), ComponentKind::PTransistor);
        assert_eq!(app.placed[0].center(), egui::pos2(60.0, 60.0));
        assert_eq!(app.wires.len(), 1);
        assert_eq!(app.wires[0].waypoints, vec![egui::pos2(140.0, 60.0)]);
        assert_eq!(app.selection.lone_component(), Some(app.placed[0].id()));
    }

    #[test]
    fn changing_a_kind_is_undoable() {
        let mut app = SimLogixApp::default();
        let id = app.place(ComponentKind::BusTransceiver, egui::pos2(60.0, 60.0));

        app.record_edit();
        app.change_kind(id, ComponentKind::BusTransceiverOe);
        assert_eq!(app.placed[0].kind(), ComponentKind::BusTransceiverOe);

        app.undo();
        assert_eq!(app.placed[0].kind(), ComponentKind::BusTransceiver);
    }

    #[test]
    fn copying_a_selection_and_pasting_it_duplicates_the_wire_between_them() {
        let mut app = SimLogixApp::default();
        let button = app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
        let led = app.place(ComponentKind::Led, egui::pos2(160.0, 40.0));
        let wire = app.add_wire(
            WireEndpoint::Pin(button, 0),
            WireEndpoint::Pin(led, 0),
            vec![egui::pos2(100.0, 40.0)],
        );
        app.selection.components.insert(button);
        app.selection.components.insert(led);
        app.selection.wires.insert(wire);

        let fragment = app.copied_fragment().expect("something is selected");
        app.paste_fragment(&fragment);

        assert_eq!(app.placed.len(), 4);
        assert_eq!(app.wires.len(), 2);
        // The copy lands offset, and is what's selected afterwards, so a
        // drag or a second paste acts on it rather than the original.
        assert_eq!(app.selection.components.len(), 2);
        assert_eq!(app.selection.wires.len(), 1);
        assert!(app
            .placed
            .iter()
            .any(|placed| placed.center() == egui::pos2(60.0, 60.0)));
    }

    #[test]
    fn a_wire_with_one_end_outside_the_selection_is_not_copied() {
        let mut app = SimLogixApp::default();
        let button = app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
        let led = app.place(ComponentKind::Led, egui::pos2(160.0, 40.0));
        let wire = app.add_wire(
            WireEndpoint::Pin(button, 0),
            WireEndpoint::Pin(led, 0),
            Vec::new(),
        );
        // Only one end's component is taken along, so the wire has nowhere
        // to attach — copying it anyway would paste an end you never chose.
        app.selection.components.insert(button);
        app.selection.wires.insert(wire);

        let fragment = app.copied_fragment().expect("something is selected");
        app.paste_fragment(&fragment);

        assert_eq!(app.placed.len(), 3, "the button alone should be pasted");
        assert_eq!(app.wires.len(), 1, "the original wire, and no copy");
    }

    #[test]
    fn pasting_something_that_is_not_a_fragment_does_nothing() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));

        // The system clipboard usually holds someone else's text. Pasting it
        // into the canvas has to be a no-op, not a half-read.
        app.paste_fragment("https://example.com");
        app.paste_fragment("{\"components\": []}");
        app.paste_fragment("");

        assert_eq!(app.placed.len(), 1);
    }

    #[test]
    fn settings_written_by_an_older_build_still_load() {
        // Every field optional or defaulted, so a settings file missing the
        // ones a later build added still reads — the same rule the project
        // format follows, and the reason a preference can be added without
        // resetting everyone's others.
        let settings: Settings = ron::from_str("()").expect("parses");
        assert_eq!(settings.language, None);
        assert!(!settings.left_drag_pans);
        assert!(settings.recent.is_empty());
    }

    #[test]
    fn a_chosen_language_survives_a_round_trip() {
        let stored = Settings {
            language: Some(Language::French),
            left_drag_pans: true,
            recent: vec![PathBuf::from("/tmp/alu.slgx")],
        };

        let text = ron::to_string(&stored).expect("serializes");
        let read: Settings = ron::from_str(&text).expect("parses");

        // `None` has to stay distinguishable from a chosen language: it is
        // what keeps following the OS locale.
        assert_eq!(read.language, Some(Language::French));
        assert!(read.left_drag_pans);
        assert_eq!(read.recent, stored.recent);
    }

    #[test]
    fn reopening_a_project_moves_it_up_the_recent_list_rather_than_repeating_it() {
        let mut app = SimLogixApp::default();
        for name in ["a.slgx", "b.slgx", "c.slgx"] {
            app.remember_recent(&PathBuf::from(format!("/tmp/{name}")));
        }
        assert_eq!(app.recent.len(), 3);

        app.remember_recent(&PathBuf::from("/tmp/c.slgx"));

        assert_eq!(app.recent.len(), 3, "no duplicate entry");
        assert_eq!(
            app.recent[0],
            PathBuf::from("/tmp/c.slgx"),
            "and it is first"
        );
    }

    #[test]
    fn the_recent_list_stops_at_its_limit() {
        let mut app = SimLogixApp::default();
        for index in 0..MAX_RECENT + 4 {
            app.remember_recent(&PathBuf::from(format!("/tmp/{index}.slgx")));
        }

        assert_eq!(app.recent.len(), MAX_RECENT);
        // The newest is kept and the oldest dropped, not the other way round.
        assert_eq!(
            app.recent[0],
            PathBuf::from(format!("/tmp/{}.slgx", MAX_RECENT + 3))
        );
    }

    #[test]
    fn a_project_that_will_not_open_is_dropped_from_the_list() {
        let mut app = SimLogixApp::default();
        let missing = PathBuf::from("/tmp/simlogix-does-not-exist.slgx");
        app.remember_recent(&missing);

        app.open_path(missing.clone());

        // Failing to read *is* the answer to "is this still here?", so there
        // is no separate existence check to keep in step with it.
        assert!(!app.recent.contains(&missing));
        assert!(app.error.is_some());
    }

    #[test]
    fn pasting_is_undoable() {
        let mut app = SimLogixApp::default();
        let id = app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));
        app.selection.components.insert(id);
        let fragment = app.copied_fragment().expect("something is selected");

        app.paste_fragment(&fragment);
        assert_eq!(app.placed.len(), 2);

        app.undo();
        assert_eq!(app.placed.len(), 1);
    }

    #[test]
    fn a_switchs_position_is_saved_where_a_buttons_press_is_not() {
        let mut app = SimLogixApp::default();
        let switch = app.place(ComponentKind::Switch, egui::pos2(40.0, 40.0));
        if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == switch) {
            let mut properties = placed.properties().clone();
            properties.pressed = Some(true);
            placed.set_properties(properties);
        }

        let project = app.to_project();
        let reloaded = SimLogixApp::from_project(&project, 0);

        // The line the document draws: what the user set is kept, what the
        // simulation produced is not. A latched switch is the former.
        assert_eq!(reloaded.placed[0].properties().pressed, Some(true));
    }

    #[test]
    fn the_framed_area_covers_what_is_actually_drawn() {
        let mut app = SimLogixApp::default();
        // Far from the origin, which is exactly the case that used to open
        // outside the visible area.
        app.place(ComponentKind::Led, egui::pos2(2000.0, 3000.0));

        let content = app.content_rect().expect("something is placed");

        assert!(content.contains(egui::pos2(2000.0, 3000.0)));
        assert!(!content.contains(egui::Pos2::ZERO));
    }

    #[test]
    fn a_loose_wire_end_is_framed_too() {
        let mut app = SimLogixApp::default();
        let led = app.place(ComponentKind::Led, egui::pos2(0.0, 0.0));
        app.add_wire(
            WireEndpoint::Pin(led, 0),
            // Nothing anchors this to a component, so only the wire knows
            // the drawing reaches out there.
            WireEndpoint::Free(egui::pos2(900.0, 0.0)),
            vec![egui::pos2(400.0, 0.0)],
        );

        let content = app.content_rect().expect("something is placed");

        assert!(content.contains(egui::pos2(900.0, 0.0)));
    }

    #[test]
    fn an_empty_circuit_has_nothing_to_frame() {
        assert!(SimLogixApp::default().content_rect().is_none());
    }

    #[test]
    fn switching_circuits_asks_for_a_refit_but_undo_does_not() {
        let mut app = SimLogixApp::default();
        app.create_circuit(String::new());
        app.switch_to(0);
        assert!(app.refit_view, "the other circuit may be drawn anywhere");

        app.refit_view = false;
        app.undo();
        // Stepping back through your own edits must not move the view.
        assert!(!app.refit_view);
    }

    #[test]
    fn saving_carries_every_circuit_not_just_the_open_one() {
        let mut app = SimLogixApp::default();
        app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
        app.create_circuit(String::new());
        app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));

        let project = app.to_project();

        assert_eq!(project.circuits.len(), 2);
        assert_eq!(project.circuits[0].components.len(), 1);
        assert_eq!(project.circuits[1].components.len(), 1);
    }
}
