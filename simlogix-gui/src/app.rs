//! The SimLogix application: state and the `eframe::App` loop.

use serde::{Deserialize, Serialize};
mod appearance_view;
mod camera;
mod canvas_ui;
mod circuits;
mod menu;
mod placement;
mod wiring;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use simlogix_core::{
    And, Buffer, BusTransceiver, Button, Circuit, CircuitAnchor, CircuitOutput, CircuitPort, Clock,
    Component, ComponentId, DFlipFlop, DLatch, Led, Nand, NetId, Nor, Not, Or, Pin, PinDirection,
    PortDrive, Probe, Rail, SrLatch, Transistor, TriStateBuffer, Xnor, Xor,
};

use crate::appearance::Appearance;
use crate::canvas;
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

/// The speeds the Simulation menu offers, slowest first.
///
/// Three rather than a slider: what you actually want is "slower so I can
/// watch", "normal", and "faster so I stop waiting", and a continuous
/// control makes you choose a number for a question that has no numeric
/// answer.
pub(crate) const SPEEDS: [f32; 3] = [0.25, 1.0, 4.0];

/// How many scheduled events a clock step will cross looking for an edge
/// before giving up. Generous — a busy circuit has plenty between two beats
/// — but finite, since a clock whose net is held by something stronger
/// never changes and would otherwise be searched for forever.
const MAX_EDGE_EVENTS: usize = 10_000;

/// How a speed is written, in the menu and in the status bar.
pub(crate) fn speed_label(speed: f32) -> &'static str {
    match speed {
        s if s < 0.5 => "¼×",
        s if s < 2.0 => "1×",
        _ => "4×",
    }
}

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
    /// Which base values are shown in, for everything that hasn't been told
    /// otherwise. `Auto` is the default, and is a choice not made rather
    /// than a fourth base.
    #[serde(default)]
    base: crate::properties::NumberBase,
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
    /// Whether the engine-state window is up. Same nature as the other two:
    /// looking at something changes nothing.
    show_inspector: bool,
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
    /// Which base values are shown in, unless a component names its own.
    /// Persisted with the settings.
    base: crate::properties::NumberBase,
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
    /// Which of [`SimLogixApp::clock_sources`] a clock step acts on, by
    /// position in `placed`. `None` means "the only one", which is the case
    /// that never has to be answered.
    clock_source_index: Option<usize>,
    /// Beat the chosen *port* on its own while the simulation runs, rather
    /// than only when stepped.
    ///
    /// Deliberately opt-in and off by default. `clock_source` falls back to
    /// the first source there is when nothing was picked, so driving
    /// automatically would set a lone `RESET` port oscillating in every
    /// circuit that has one — with nothing in the drawing to explain why.
    ///
    /// Not remembered between runs, like pause and speed: a way of working
    /// at a moment.
    free_running_source: bool,
    /// The tick the driven port was last flipped at.
    source_beat_at: u64,
    /// How fast logical time runs against real time, as a multiplier on the
    /// per-frame tick budget. A clock's *period* is unchanged — this moves
    /// the whole circuit through time faster or slower, which is what lets
    /// you watch something happen before deciding to freeze it.
    ///
    /// Not remembered between runs, for the same reason pause isn't: it is a
    /// way of working at a moment, not something you set once like a theme.
    speed: f32,
    /// The pins whose declared width disagrees with the net they sit on,
    /// recomputed with the nets.
    ///
    /// Per pin rather than per net: the net is fine, and what is wrong is
    /// one thing attached to it. It is a *standing* fault — it stays until
    /// the drawing is changed — so it is reported in the status bar and on
    /// the pin itself, never in the transient notice over the canvas.
    width_faults: Vec<(ComponentId, usize)>,
    /// How many bits each wire carries — from what the *wires* alone say,
    /// not from the net's width.
    ///
    /// Once a splitter joins branches to their bus they are one net, so a
    /// branch wire's net is as wide as the whole bus. Drawing it that way
    /// would say something false about it: thick, and reporting a value it
    /// does not hold.
    wire_slices: HashMap<u64, (usize, usize)>,
    /// Which wires are joined to which **by wire alone** — what a colour
    /// spreads over, and what stops a branch's colour repainting its bus.
    wire_colour_groups: HashMap<wiring::Node, Vec<u64>>,
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
    /// And whether it will be reflected, which `Shift+R` sets the same way
    /// `R` sets the rotation above.
    place_mirrored: bool,
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
    /// Where each component's pins were drawn last frame, in canvas
    /// coordinates.
    ///
    /// Test-only for the same reason as the two above: the application
    /// reads its pins from the frame it drew them in, and a symbol's
    /// geometry is otherwise not observable from outside — which is how a
    /// splitter came to draw its pins outside its own box.
    #[cfg(test)]
    pin_positions: std::collections::HashMap<ComponentId, Vec<egui::Pos2>>,
    /// The camera belonging to the view that *isn't* showing. Swapped on
    /// switch: the two views look at unrelated places, so carrying one
    /// camera between them would drop you somewhere arbitrary.
    idle_scene_rect: egui::Rect,
}

impl Default for SimLogixApp {
    fn default() -> Self {
        Self {
            show_about: false,
            show_inspector: false,
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
            base: crate::properties::NumberBase::default(),
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
            width_faults: Vec::new(),
            wire_slices: HashMap::new(),
            wire_colour_groups: HashMap::new(),
            running: true,
            clock_source_index: None,
            free_running_source: false,
            source_beat_at: 0,
            speed: 1.0,
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
            place_mirrored: false,
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
            pin_positions: std::collections::HashMap::new(),
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
        self.place_with(kind, center, &Properties::default())
    }

    /// The same, told the properties the component will carry.
    ///
    /// Only a splitter needs them, and it needs them badly: how many pins it
    /// has *is* one of its properties, and a built component's pins are
    /// fixed. Everything else is built the same whatever they say, and takes
    /// them afterwards through `set_properties`.
    fn place_with(
        &mut self,
        kind: ComponentKind,
        center: egui::Pos2,
        properties: &Properties,
    ) -> ComponentId {
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
            // A plain input port, in the engine: driving a value on N bits
            // is exactly what one does. What differs is that the value is a
            // setting rather than something you click through, and that
            // lives in the document and the symbol — not in the net.
            ComponentKind::Constant => {
                let net = self.circuit.add_net();
                let (port, handles) = CircuitPort::input();
                let id = self.circuit.add_component(
                    Box::new(port),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::constant(id, center, handles)
            }
            // Its pin count comes from its properties, so a freshly placed
            // one is what an untouched splitter is: a one-bit bus with a
            // single branch. Widening it adds pins, which a built component
            // cannot do — that edit goes through the document instead, the
            // same way changing a component's type does.
            ComponentKind::Splitter => {
                let branches = properties.branch_widths().len();
                let pins = (0..=branches)
                    .map(|_| Pin {
                        // `InOut` and inert: a splitter neither reads nor
                        // drives. Its pins exist so wires have something to
                        // attach to; what they *mean* is said by the net
                        // rebuild, which joins them at bit offsets.
                        direction: PinDirection::InOut,
                        net: self.circuit.add_net(),
                    })
                    .collect();
                // **Not a component that relays.** A splitter is wire — bit
                // 3 of this net *is* bit 0 of that one — so there is nothing
                // here to evaluate, and nothing that could cost a tick or
                // hear its own echo. `CircuitAnchor` is the same nothing an
                // instance already uses.
                let id = self.circuit.add_component(Box::new(CircuitAnchor), pins);
                PlacedComponent::splitter(id, center)
            }
            ComponentKind::InputPort
            | ComponentKind::OutputPort
            | ComponentKind::InOutPort
            | ComponentKind::TriStateSource => {
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
                    // A source only ever drives, so `Output` rather than
                    // `InOut`: what it shows is read off the net by the GUI,
                    // not through a pin of its own.
                    ComponentKind::TriStateSource => {
                        let (port, level) = CircuitPort::bidirectional();
                        (Box::new(port), PinDirection::Output, Some(level))
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
                PlacedComponent::hand_set(id, center, kind, level)
            }
            ComponentKind::Circuit(path) => {
                // Refusing (a missing circuit, or one that contains itself)
                // still places the box, empty: an instance you can see and
                // delete beats a click that silently does nothing, and
                // `flatten` has already said why in the status window.
                let wiring = self.flatten(&path).unwrap_or_default();
                let pins = wiring
                    .ports
                    .iter()
                    .map(|_| Pin {
                        // `InOut` whatever the port's own direction: the
                        // anchor drives nothing, and its pin has to be able
                        // to both carry a value in and read one back out.
                        direction: PinDirection::InOut,
                        net: self.circuit.add_net(),
                    })
                    .collect();
                let id = self.circuit.add_component(Box::new(CircuitAnchor), pins);
                self.circuit.schedule_now(id);
                let appearance = self.appearance_of(&path, &wiring.ports);
                PlacedComponent::instance(id, center, path, wiring, appearance)
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
            ComponentKind::DFlipFlop | ComponentKind::DFlipFlopFalling | ComponentKind::DLatch => {
                // How many pins it has is one of its properties, which is
                // why this needs `place_with`: a built component's pins are
                // fixed, so asking for the asynchronous inputs afterwards
                // goes through the document and rebuilds — the same route a
                // splitter's branch count takes.
                let async_inputs = properties.async_set_reset();
                let inputs = if async_inputs { 4 } else { 2 };
                let pins: Vec<Pin> = (0..inputs + 2)
                    .map(|index| Pin {
                        direction: if index < inputs {
                            PinDirection::Input
                        } else {
                            PinDirection::Output
                        },
                        net: self.circuit.add_net(),
                    })
                    .collect();
                let component: Box<dyn Component> = match kind {
                    ComponentKind::DFlipFlop => Box::new(DFlipFlop::rising()),
                    ComponentKind::DFlipFlopFalling => Box::new(DFlipFlop::falling()),
                    _ => Box::new(DLatch::new()),
                };
                let id = self.circuit.add_component(component, pins);
                PlacedComponent::storage(id, center, kind, async_inputs)
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
    /// Builds a component from its saved form: places it, gives it its
    /// rotation and properties, and **re-evaluates it**.
    ///
    /// That last step is the whole reason this exists. `place` schedules the
    /// component before its properties are applied, so a switch saved closed
    /// or a port with a resting level went into the engine holding whatever
    /// it starts with — and nothing put it right afterwards, since neither
    /// has an input for `rebuild_nets` to notice a change on. Three callers
    /// wrote out the same three steps and all three forgot the fourth.
    fn place_saved(&mut self, saved: &SavedComponent, offset: egui::Vec2) -> ComponentId {
        let id = self.place_with(
            saved.kind.clone(),
            egui::pos2(saved.x, saved.y) + offset,
            &saved.properties,
        );
        if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
            placed.set_rotation(saved.rotation);
            placed.set_mirrored(saved.mirrored);
            placed.set_properties(saved.properties.clone());
        }
        self.circuit.schedule_now(id);
        id
    }

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
                    mirrored: placed.is_mirrored(),
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
            .map(|saved| app.place_saved(saved, egui::Vec2::ZERO))
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
        self.base = crate::properties::NumberBase::default();
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
    /// There are two things to look at, not three: **the drawing** — which
    /// the schematic and the simulation both show, the second being the
    /// first with the editing taken away — and **the symbol**, which always
    /// sits on the origin. So there are two cameras, and one is put away
    /// only when crossing between the two.
    ///
    /// Swapping on every switch was wrong and showed: going from schematic
    /// to simulation and back moved the view, though what is on screen never
    /// changed. It also handed the symbol's camera to the drawing, which is
    /// what then tripped the refit below — a jump instead of a nudge.
    fn switch_view(&mut self, view: toolbar::View) {
        if view == self.view {
            return;
        }
        let was = self.view;
        self.view = view;
        self.swap_camera_for(was, view);
        // Never framed before: `ui()` reads a zero rect as "frame me".
        if self.scene_rect.width() <= 0.0 {
            self.refit_view = true;
        }
        // ...and whenever what it kept no longer looks at anything. A camera
        // belongs to a *view*, not to a circuit, so switching circuits leaves
        // the other view's camera where the previous circuit's drawing was —
        // and arriving there lands on blank canvas with nothing saying which
        // way the work is. Panning off and switching does the same.
        //
        // Only when the two have nothing in common, rather than framing on
        // every switch: a camera that still shows the drawing is one the user
        // chose, and re-framing would throw their zoom away every time they
        // flicked between the two.
        if self
            .content_rect()
            .is_some_and(|content| !self.scene_rect.intersects(content))
        {
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
            return Some(appearance.rect(egui::Pos2::ZERO, crate::symbol::Orientation::default()));
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

    /// The pins an instance of `path` would expose, and what it would look
    /// like — everything the placement ghost needs, worked out the same way
    /// the real instance works it out so the two cannot disagree.
    fn instance_preview(&self, path: &str) -> (Vec<InstancePort>, Appearance) {
        let ports: Vec<InstancePort> = self
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
        let appearance = self.appearance_of(path, &ports);
        (ports, appearance)
    }

    /// Where a component's *origin* goes when its symbol is dropped with the
    /// pointer at `at`.
    ///
    /// Everything built in is drawn about its own origin, so for those the
    /// two are the same point. A circuit's own symbol need not be: dragging a
    /// pin out in the appearance editor moves the drawing away from the
    /// origin, and the origin is what a position stores — so the pointer sat
    /// somewhere off the symbol, well above it for a drawing that had grown
    /// upwards, and you were placing by guesswork.
    ///
    /// The offset is rounded to a whole number of grid steps, and that is
    /// what keeps the pins on the grid: `at` is already snapped, so an offset
    /// that wasn't a multiple would take every pin off it. Rounded, the ghost
    /// can sit up to half a step off the pointer, which is invisible next to
    /// being a symbol's width away.
    pub(super) fn drop_origin(
        &self,
        kind: &ComponentKind,
        at: egui::Pos2,
        rotation: canvas::Rotation,
        mirrored: bool,
    ) -> egui::Pos2 {
        let Some(path) = kind.circuit_path() else {
            return at;
        };
        let (_, appearance) = self.instance_preview(path);
        let middle = appearance
            .rect(
                egui::Pos2::ZERO,
                crate::symbol::Orientation::new(rotation, mirrored),
            )
            .center();
        at - canvas::snap_to_grid(middle).to_vec2()
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
            // Applied before the entry is dropped: this is what puts a
            // switch's position or a port's resting level into the cell the
            // engine reads.
            ids.push(Some(self.place_saved(component, egui::Vec2::ZERO)));
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
        //
        // The declared widths go up with it, and for the same reason: the
        // only record of how wide an inner pin is lives in the entry about
        // to be dropped, and `rebuild_nets` has no other way to ask.
        let mut nested: Vec<Vec<(ComponentId, usize)>> = Vec::new();
        let mut inner_widths: Vec<((ComponentId, usize), Option<usize>)> = Vec::new();
        for placed in &self.placed[first_inner..] {
            for index in 0..self.circuit.try_pins(placed.id()).map_or(0, <[_]>::len) {
                inner_widths.push(((placed.id(), index), placed.pin_width(index)));
            }
            inner_widths.extend_from_slice(placed.inner_pin_widths());
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
        Some(InstanceWiring {
            ports: ports.into_iter().map(|(_, port)| port).collect(),
            inner_groups,
            inner_widths,
        })
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
                        // The port's own declared width: the boundary is
                        // what says how wide it is, and it is read here so
                        // the preview and the real instance cannot come to
                        // disagree about it either.
                        width: component.properties.width(),
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
                    mirrored: placed.is_mirrored(),
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
            .map(|saved| self.place_saved(saved, offset))
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
        self.edit_saved_component(id, |component| component.kind = kind);
    }

    /// Works out how much room each component's readout needs, so its box
    /// can be sized to hold it.
    ///
    /// Every frame rather than stored: the base is a setting that changes
    /// without the drawing changing, and a probe's width comes from a net
    /// that is reallocated on every edit. Derived, so it cannot go stale —
    /// the same bargain the wire routes make.
    ///
    /// Measured in the real font, since a box a little too narrow clips the
    /// value it exists to show.
    fn refresh_readouts(&mut self, ui: &egui::Ui) {
        let default_base = self.base;
        let per_char = crate::symbol::readout_char_width(ui);
        let room: Vec<f32> = self
            .placed
            .iter()
            .map(|placed| {
                let kind = placed.kind();
                if !Properties::has_base(&kind) {
                    return 0.0;
                }
                // A probe declares no width of its own — it reads whatever
                // its net carries, so that is what it has to have room for.
                let bits = if kind == ComponentKind::Probe {
                    // Its pin's slice, not the net's width: on a branch it
                    // reads two bits of an eight-bit conductor, and a box
                    // sized for eight would be five characters of nothing.
                    self.circuit
                        .try_pins(placed.id())
                        .and_then(|pins| pins.first())
                        .map_or(1, |pin| self.circuit.pin_slice((placed.id(), 0), pin.net).1)
                } else {
                    placed.properties().width()
                };
                let base = placed.properties().base.unwrap_or(default_base);
                base.digits(bits) as f32 * per_char
            })
            .collect();
        for (placed, room) in self.placed.iter_mut().zip(room) {
            placed.set_readout(room);
        }
    }

    /// Which bits of its conductor a wire carries: where its bit zero sits,
    /// and how many it takes.
    pub(crate) fn wire_slice(&self, wire: u64) -> (usize, usize) {
        self.wire_slices.get(&wire).copied().unwrap_or((0, 1))
    }

    /// How many bits a wire carries.
    pub(crate) fn wire_width(&self, wire: u64) -> usize {
        self.wire_slice(wire).1
    }

    /// What a wire carries — **its own bits**, not its whole conductor.
    pub(crate) fn wire_signal(&self, wire: u64, net: Option<NetId>) -> simlogix_core::Signal {
        let (offset, width) = self.wire_slice(wire);
        match net {
            Some(net) => self.circuit.signal_at(net).slice(offset, width),
            None => simlogix_core::Signal::splat(simlogix_core::Level::Unknown, width),
        }
    }

    /// What the inspector needs to name each component and say how wide its
    /// pins are.
    ///
    /// Its own method so the window and a test build it the same way — the
    /// tests used to assemble it by hand, which is how a test comes to check
    /// something the application never does.
    fn named_components(&self, strings: &Strings) -> Vec<crate::inspector::Named> {
        self.placed
            .iter()
            .map(|placed| crate::inspector::Named {
                id: placed.id(),
                pin_widths: (0..self.circuit.try_pins(placed.id()).map_or(0, <[_]>::len))
                    .map(|index| placed.pin_width(index))
                    .collect::<Vec<_>>(),
                label: placed
                    .properties()
                    .label()
                    .map(str::to_string)
                    .unwrap_or_else(|| strings.component_kind_label(&placed.kind()).to_string()),
            })
            .collect()
    }

    /// Applies edited properties to one component, and makes the engine
    /// notice.
    ///
    /// Its own method rather than a block inside `draw`, so what the panel
    /// does and what a test does are the same thing — the rebuild below is
    /// exactly the sort of wiring that stays green in a unit test while
    /// being wrong in the application.
    fn set_component_properties(&mut self, id: ComponentId, edited: Properties) {
        // A splitter's pin count comes from its properties, and a built
        // component's pins are fixed — so widening one is an edit that has
        // to go through the document, exactly as changing a component's
        // type does. Every id is handed out afresh by that rebuild, which
        // is why nothing below may assume `id` still names anything.
        let rebuild = self
            .placed
            .iter()
            .find(|placed| placed.id() == id)
            .is_some_and(|placed| {
                let kind = placed.kind();
                (kind == ComponentKind::Splitter
                    && placed.properties().branch_widths().len() != edited.branch_widths().len())
                    // Asking a flip-flop for its asynchronous inputs adds two
                    // pins, which a built component cannot grow. Same route,
                    // same reason.
                    || (matches!(
                        kind,
                        ComponentKind::DFlipFlop
                            | ComponentKind::DFlipFlopFalling
                            | ComponentKind::DLatch
                    ) && placed.properties().async_set_reset() != edited.async_set_reset())
            });
        if rebuild {
            let properties = edited.clone();
            self.edit_saved_component(id, move |component| component.properties = properties);
            return;
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

    /// Rewrites one component in the saved document and reopens the circuit.
    ///
    /// For the edits a *built* component cannot take: its type, and anything
    /// that changes how many pins it has — a splitter's branches. A
    /// component's identity in the engine is its `ComponentId`, which every
    /// wire endpoint refers to, so swapping it in place would mean remapping
    /// all of them; rewriting the saved form and reopening keeps routes,
    /// waypoints and colours exactly as they are, because wires are stored
    /// by index.
    fn edit_saved_component(&mut self, id: ComponentId, edit: impl FnOnce(&mut SavedComponent)) {
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
        edit(component);

        let open = self.active;
        self.reopen(&project, open);
        // Ids are handed out afresh by the rebuild, so the selection is
        // recovered by position — otherwise the edit would deselect the
        // thing you're editing.
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
                let preferences = (
                    self.language,
                    self.language_chosen,
                    self.left_drag_pans,
                    self.base,
                );
                let recent = std::mem::take(&mut self.recent);
                *self = Self::from_project(&project, 0);
                (
                    self.language,
                    self.language_chosen,
                    self.left_drag_pans,
                    self.base,
                ) = preferences;
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
    /// Whether something is queued for *now* and has not run yet.
    ///
    /// Only possible while paused: clicking a port or a switch schedules its
    /// component and asks the circuit to settle, and settling is what a
    /// pause refuses. So the value you set is in, and nothing on the wires
    /// has been told about it yet.
    ///
    /// It is the difference between "due now, not run" and "due later" that
    /// matters — a clock's next beat is pending in the plain sense and is
    /// not what anyone is waiting on, so the test is against *now* rather
    /// than merely being scheduled at all.
    fn change_pending(&self) -> bool {
        self.circuit
            .next_event_tick()
            .is_some_and(|tick| tick <= self.circuit.now())
    }

    /// Flips the chosen port when its beat is due, so a circuit whose clock
    /// arrives from outside can be watched running rather than only stepped.
    ///
    /// Only in the simulation view, and only while armed. The button that
    /// arms it lives in that view's tool row, and a behaviour you cannot see
    /// the control for is one you cannot turn off — you would be left
    /// looking at a port beating with nothing in the drawing to say why.
    ///
    /// The beat is counted in *logical* time, so it follows the speed
    /// multiplier and stops dead with the pause, exactly as a real `Clock`
    /// does.
    fn beat_free_running_source(&mut self, strings: &Strings) {
        if !self.free_running_source || self.view != toolbar::View::Simulation {
            return;
        }
        let now = self.circuit.now();
        // A reopen builds a new circuit with its clock back at zero, which
        // would otherwise leave the beat permanently in the future.
        if now < self.source_beat_at {
            self.source_beat_at = now;
        }
        if now.saturating_sub(self.source_beat_at) < CLOCK_PERIOD_TICKS {
            return;
        }

        let Some(index) = self.clock_source(strings) else {
            return;
        };
        let Some(placed) = self.placed.get(index) else {
            return;
        };
        let Some(level) = placed.hand_set_level() else {
            return;
        };
        // A beat is all-high then all-low: driving a *value* is what the
        // value field is for, and a clock has nothing to say about one.
        let all = simlogix_core::all_ones(placed.width());
        level.set(match level.get() {
            PortDrive::Driving(bits) if bits != 0 => PortDrive::Driving(0),
            _ => PortDrive::Driving(all),
        });
        let id = placed.id();
        self.source_beat_at = now;
        self.circuit.schedule_now(id);
    }

    fn advance_circuit(&mut self, ticks: u64) {
        if !self.running || self.unstable_net.is_some() {
            return;
        }
        if let Err(unstable) = self.circuit.advance(ticks) {
            self.unstable_net = Some(unstable.net);
            self.running = false;
        }
    }

    /// Advances by `ticks` and stops, whatever the simulation was doing.
    ///
    /// Not `advance_circuit`, which refuses while paused — that refusal is
    /// what keeps the frame loop from running a paused circuit, and stepping
    /// is the deliberate exception. It **pauses first**: stepping while it
    /// runs would add a step of your size on top of the frame's own, so what
    /// you looked at afterwards would not be the step you asked for.
    ///
    /// Allowed even after a net has been reported unstable, and the report is
    /// left standing. Walking an oscillation one tick at a time is exactly
    /// what you would want to do about it — and it cannot trip the guard
    /// again on the way, since `MAX_TOGGLES_PER_NET` counts within a single
    /// `advance` call.
    fn step(&mut self, ticks: u64) {
        self.running = false;
        if let Err(unstable) = self.circuit.advance(ticks) {
            self.unstable_net = Some(unstable.net);
        }
    }

    /// Advances straight to the next tick where something is scheduled.
    ///
    /// Between two beats of a clock there are dozens of ticks with nothing
    /// in them, and crossing those one at a time says nothing at all.
    ///
    /// Does nothing when nothing is pending — a circuit that has settled and
    /// holds no clock will never move again on its own, and pretending to
    /// advance would only run the tick counter up. The button is greyed for
    /// the same reason.
    fn step_to_next_event(&mut self) {
        let Some(tick) = self.circuit.next_event_tick() else {
            return;
        };
        // At least one, so this always moves: everything due at or before
        // now has already been evaluated, but a caller does not have to know
        // that to be safe here.
        self.step(tick.saturating_sub(self.circuit.now()).max(1));
    }

    /// What "one clock edge" could mean in this circuit, by position in
    /// `placed`, each with a label for the picker.
    ///
    /// Both the `Clock` components *and* the ports you set by hand: a
    /// circuit drawn to be used inside another has its clock arriving on a
    /// port, so a flip-flop tested on its own has no `Clock` in it at all.
    /// Refusing to step there would refuse the very circuit you drew the
    /// port for.
    ///
    /// A `Switch` is not offered even though it drives a level too — its
    /// position is part of the saved document, so stepping one would be
    /// making an edit on your behalf, undo step and all. A port's level is
    /// runtime state, like a button press, which is what makes this free.
    fn clock_sources(&self, strings: &Strings) -> Vec<(usize, String)> {
        let mut seen = 0;
        self.placed
            .iter()
            .enumerate()
            .filter(|(_, placed)| {
                placed.kind() == ComponentKind::Clock || placed.hand_set_level().is_some()
            })
            .map(|(index, placed)| {
                seen += 1;
                let label = match placed.properties().name.as_deref() {
                    Some(name) if !name.is_empty() => name.to_string(),
                    // Numbered in the order they were placed, so two
                    // unnamed clocks are still two different entries.
                    _ => format!("{} {seen}", strings.component_kind_label(&placed.kind())),
                };
                (index, label)
            })
            .collect()
    }

    /// The source a clock step acts on: whichever was picked, or the only
    /// one there is. `None` when the circuit offers none.
    ///
    /// Held by position rather than by `ComponentId`, since those are handed
    /// out afresh every time the circuit is rebuilt — which is nearly every
    /// edit. A stale position picks a different component; a stale id picks
    /// nothing at all, silently.
    fn clock_source(&self, strings: &Strings) -> Option<usize> {
        let sources = self.clock_sources(strings);
        match self.clock_source_index {
            Some(index) if sources.iter().any(|(at, _)| *at == index) => Some(index),
            _ => sources.first().map(|(at, _)| *at),
        }
    }

    /// Advances to the next edge of the chosen clock source, or drives one
    /// by hand if that source is a port.
    ///
    /// For a `Clock`, "the next edge" is read off the net rather than
    /// computed from the period: whatever ends up on the wire is what the
    /// rest of the circuit sees, and a clock feeding something else through
    /// a gate would make the period a lie. Jumping event to event rather
    /// than tick by tick, because the ticks in between hold nothing.
    ///
    /// Bounded, and gives up rather than hanging: a source that never
    /// changes — a clock whose net is held by something stronger — would
    /// otherwise search forever.
    fn step_clock_edge(&mut self, strings: &Strings) {
        let Some(index) = self.clock_source(strings) else {
            return;
        };
        let Some(placed) = self.placed.get(index) else {
            return;
        };
        let id = placed.id();

        // A port is not on a schedule of its own: *you* are its clock, so a
        // step is a flip. High and low only — undriven is a third position
        // of the switch, not part of a cycle.
        if let Some(level) = placed.hand_set_level() {
            let all = simlogix_core::all_ones(placed.width());
            level.set(match level.get() {
                PortDrive::Driving(bits) if bits != 0 => PortDrive::Driving(0),
                _ => PortDrive::Driving(all),
            });
            self.circuit.schedule_now(id);
            self.step(SETTLE_TICKS);
            return;
        }

        let Some(net) = self.circuit.try_pins(id).and_then(|pins| pins.first()) else {
            return;
        };
        let net = net.net;
        // The whole signal, not one level: a clock on a bus changes when
        // any bit of it does.
        let before = self.circuit.signal_at(net);
        for _ in 0..MAX_EDGE_EVENTS {
            let Some(tick) = self.circuit.next_event_tick() else {
                return;
            };
            self.step(tick.saturating_sub(self.circuit.now()).max(1));
            if self.circuit.signal_at(net) != before {
                return;
            }
        }
    }

    /// A small readout beside the pointer while a bus is hovered: how many
    /// bits, and what it carries.
    ///
    /// An `Area` at the pointer rather than a tooltip, because a wire is
    /// *painted* and has no widget for egui to hang one on — and in screen
    /// space rather than in the scene, so it neither scales nor blurs with
    /// the zoom.
    ///
    /// The value is left out while the signal state is hidden: that mode
    /// exists to stop levels changing under you, and a readout that went on
    /// reporting them would be arguing with it.
    fn show_bus_hint(
        &self,
        ui: &egui::Ui,
        strings: &Strings,
        wire: u64,
        width: usize,
        net: Option<NetId>,
    ) {
        let Some(at) = ui.ctx().pointer_latest_pos() else {
            return;
        };
        let mut text = strings.inspector_bits.replace("{}", &width.to_string());
        if self.show_signal_state {
            if let Some(net) = net {
                text.push_str(" · ");
                // The wire's own bits, not its whole conductor: a branch
                // shares a net with its bus and carries a slice of it.
                let _ = net;
                text.push_str(&crate::placed_component::signal_text(
                    &self.wire_signal(wire, Some(net)),
                    self.base,
                ));
            }
        }
        egui::Area::new(egui::Id::new("bus_hover"))
            .order(egui::Order::Tooltip)
            .fixed_pos(at + egui::vec2(16.0, 16.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(text);
                });
            });
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
        format!("SimLogix{BUILD_MARKER} — {name}{marker}")
    }
}

/// Says so when this was not built with `--release`.
///
/// In the title because that is what tells two windows apart at a glance,
/// and a debug build is *slow* — a circuit of any size runs visibly worse,
/// which reads as the simulator being at fault rather than the build.
///
/// `debug_assertions` rather than a feature of our own: it is the flag that
/// actually decides what this binary does, so it cannot come to disagree
/// with the thing it describes. A release build with assertions turned back
/// on will say debug, and that is right — it is one.
///
/// Not translated. It is a marker rather than a sentence, and the word is
/// the same one the profile is called.
pub(crate) const BUILD_MARKER: &str = if cfg!(debug_assertions) {
    " (debug)"
} else {
    ""
};

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
        app.base = settings.base;
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
                base: self.base,
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
        // Before anything asks a component how big it is: a readout's room
        // decides the box, and the box is read by the hover test, the
        // marquee and the view framing alike.
        self.refresh_readouts(ui);
        self.tick_budget += ui.ctx().input(|i| i.stable_dt) * TICKS_PER_SECOND * self.speed;
        let ticks_due = self.tick_budget.floor();
        if ticks_due > 0.0 {
            self.tick_budget -= ticks_due;
            self.advance_circuit(ticks_due as u64);
        }
        ui.ctx().request_repaint();

        let strings = Strings::for_language(self.language);
        // After the advance, so its own event is picked up by the next one
        // rather than a tick late.
        self.beat_free_running_source(strings);

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

        let keys = menu::Shortcuts::new();
        // Consumed here, before any widget sees the keystroke, so a shortcut
        // never doubles as canvas input.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.new)) {
            self.request_action(PendingAction::New, ui.ctx());
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.open)) {
            self.request_action(PendingAction::Open, ui.ctx());
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.save_as)) {
            self.save_project_as();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.save)) {
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

        // Stepping is allowed from every view, not only the simulation one:
        // it advances time, which is a thing the schematic is doing too.
        if !ui.ctx().text_edit_focused()
            && ui
                .ctx()
                .input_mut(|input| input.consume_shortcut(&keys.inspector))
        {
            self.show_inspector = !self.show_inspector;
        }

        if !ui.ctx().text_edit_focused() {
            // The longer step is tried first. egui matches modifiers
            // exactly, so the two chords cannot both fire — but the order
            // says which one is meant to win if that ever stops being true.
            if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.step_event)) {
                self.step_to_next_event();
            } else if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.step_edge)) {
                self.step_clock_edge(strings);
            } else if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.step)) {
                self.step(1);
            }
        }

        // Renames the circuit you are in — the one shown in bold. The tree
        // has no notion of a selected row, so there is nothing else `F2`
        // could unambiguously mean; folders and the project keep the context
        // menu, which is where they were reachable from anyway.
        //
        // It exists because the double-click that used to rename now opens.
        if !ui.ctx().text_edit_focused()
            && self.renaming.is_none()
            && ui.ctx().input(|i| i.key_pressed(egui::Key::F2))
        {
            let name = self.circuits[self.active].name.clone();
            self.renaming = Some((RenameTarget::Circuit(self.active), name));
        }

        // Redo is tested first: Ctrl+Shift+Z would otherwise also match the
        // plain Ctrl+Z pattern and undo instead.
        if ui
            .ctx()
            .input_mut(|i| i.consume_shortcut(&keys.redo) || i.consume_shortcut(&keys.redo_alt))
        {
            self.redo();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&keys.undo)) {
            self.undo();
        }

        self.menu_bar(ui, strings, &keys);

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            let hint = if !self.width_faults.is_empty() {
                // Above the instability report: a width that disagrees is a
                // fault in the *drawing*, and a drawing that cannot be read
                // consistently is worth fixing before anything it does.
                Some(
                    strings
                        .status_width_fault
                        .replace("{}", &self.width_faults.len().to_string()),
                )
            } else if let Some(net) = self.unstable_net {
                Some(strings.status_unstable.replace("{}", &net.0.to_string()))
            } else if !self.show_signal_state {
                Some(strings.status_signals_hidden.to_string())
            } else if !self.running {
                // A switch draws its own lever, so a click on one looks like
                // it worked whatever the engine is doing. A port draws what
                // its *net* resolves to, which cannot change until time
                // moves — so without this, clicking one while paused looks
                // like nothing happened at all.
                Some(if self.change_pending() {
                    strings.status_paused_pending.to_string()
                } else {
                    strings.status_paused.to_string()
                })
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
            ui.horizontal(|ui| {
                ui.label(hint.unwrap_or_default());
                // The logical clock, at the far right and always shown. It is
                // what makes stepping legible: a tick where nothing happens
                // to look at is otherwise indistinguishable from a button
                // that did nothing, and most ticks are that.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            strings
                                .status_tick
                                .replace("{}", &self.circuit.now().to_string()),
                        )
                        .weak(),
                    );
                    // Only when it is not 1x. A mode you can leave on and
                    // forget has to say so — but saying "normal speed" all
                    // day is noise.
                    if self.speed != 1.0 {
                        ui.label(egui::RichText::new(speed_label(self.speed)).weak());
                    }
                });
            });
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
                            circuit_tree::Tree {
                                strings,
                                project_name: &project_name,
                                folders: &self.folders,
                                circuits: &self.circuits,
                                active: self.active,
                                reveal_active: std::mem::take(&mut self.reveal_active),
                            },
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
                                self.place_mirrored = false;
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
            // Both open the name for editing straight away, with the
            // generated one filled in: naming a thing is part of making it.
            // Escape leaves the generated name, which is what it was before.
            //
            // After the create, never before — `create_circuit` opens the new
            // circuit, and opening one rebuilds the application, which would
            // throw a rename in progress away.
            Some(TreeAction::Create { folder }) => {
                self.create_circuit(folder);
                let name = self.circuits[self.active].name.clone();
                self.renaming = Some((RenameTarget::Circuit(self.active), name));
            }
            Some(TreeAction::CreateFolder { parent }) => {
                let path = self.create_folder(&parent);
                let leaf = path.rsplit('/').next().unwrap_or_default().to_string();
                self.renaming = Some((RenameTarget::Folder(path), leaf));
            }
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
        let mut pending_mirror: Option<(ComponentId, bool)> = None;
        let mut pending_wire_color: Option<(u64, Option<[u8; 3]>)> = None;
        // A port whose live value was just changed, so it can be told to
        // re-evaluate. Runtime state, so nothing else about it is recorded.
        let mut pending_drive: Option<ComponentId> = None;
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
                        // Enabled whatever the mode, because what is
                        // disabled is chosen section by section below. The
                        // **value** stays live while a circuit is being
                        // watched: it is runtime state, which is the whole
                        // reason it was split out of the properties, and
                        // greying it with them undid that distinction.
                        ui.add_enabled_ui(true, |ui| {
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
                                let width = self.wire_width(wire.id);
                                if let Some(color) = ui
                                    .add_enabled_ui(editable, |ui| {
                                        properties::show_wire(ui, strings, wire.color, width)
                                    })
                                    .inner
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
                                    let outcome = ui
                                        .add_enabled_ui(editable, |ui| {
                                            properties::show(
                                                ui,
                                                strings,
                                                &placed.kind(),
                                                &mut edited,
                                                placed.is_mirrored(),
                                                self.base,
                                            )
                                        })
                                        .inner;
                                    if let Some(kind) = outcome.change_kind {
                                        pending_kind = Some((placed.id(), kind));
                                    }
                                    if let Some(mirrored) = outcome.mirrored {
                                        pending_mirror = Some((placed.id(), mirrored));
                                    }
                                    if outcome.edit_started || edited != *placed.properties() {
                                        pending_properties =
                                            Some((placed.id(), edited, outcome.edit_started));
                                    }
                                    // Below the properties and separated
                                    // from them: this is runtime state, and
                                    // applied straight to the cell the
                                    // engine reads rather than through the
                                    // document — no snapshot, no dirty flag.
                                    // A switch's position is the same
                                    // nature as a port's drive, so it sits
                                    // in the same place: runtime, straight
                                    // into the cell, no undo step.
                                    if let Some(on) = placed.switch_position() {
                                        if let Some(now) =
                                            properties::show_switch_value(ui, strings, on.get())
                                        {
                                            on.set(now);
                                            pending_drive = Some(placed.id());
                                        }
                                    }
                                    if let Some(drive) = placed.hand_set_level() {
                                        if let Some(next) = properties::show_value(
                                            ui,
                                            strings,
                                            drive.get(),
                                            placed.width(),
                                            placed.properties().base.unwrap_or(self.base),
                                        ) {
                                            drive.set(next);
                                            pending_drive = Some(placed.id());
                                        }
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

        // Runtime state: the cell is already set, and what remains is to
        // let the change reach the wires. No snapshot and no dirty flag —
        // driving a port is no more a document change than pressing a
        // button is.
        if let Some(id) = pending_drive {
            self.circuit.schedule_now(id);
            self.advance_circuit(SETTLE_TICKS);
        }

        if let Some((id, mirrored)) = pending_mirror {
            // Placement, not a property, so it is applied straight to the
            // component — but it is an edit to the drawing all the same.
            self.record_edit();
            if let Some(placed) = self.placed.iter_mut().find(|placed| placed.id() == id) {
                placed.set_mirrored(mirrored);
            }
            self.dirty = true;
        }

        if let Some((id, edited, started)) = pending_properties {
            // Only the frame an editing session begins, so a typed-in name
            // is one undo step rather than one per keystroke.
            if started {
                self.record_edit();
            }
            self.set_component_properties(id, edited);
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
                    let has_event = self.circuit.next_event_tick().is_some();
                    let clocks = self.clock_sources(strings);
                    let chosen = self.clock_source(strings);
                    let drivable = chosen
                        .and_then(|at| self.placed.get(at))
                        .is_some_and(|placed| placed.hand_set_level().is_some());
                    match toolbar::show_sim_tools(
                        ui,
                        strings,
                        toolbar::SimRow {
                            tool: self.sim_tool,
                            has_event,
                            clocks: &clocks,
                            chosen,
                            drivable,
                            free_running: self.free_running_source,
                            running: self.running,
                        },
                    ) {
                        Some(toolbar::SimAction::Tool(tool)) => self.sim_tool = tool,
                        Some(toolbar::SimAction::ToggleRunning) => self.toggle_running(),
                        Some(toolbar::SimAction::StepTick) => self.step(1),
                        Some(toolbar::SimAction::StepEvent) => self.step_to_next_event(),
                        Some(toolbar::SimAction::StepEdge) => self.step_clock_edge(strings),
                        Some(toolbar::SimAction::PickClock(at)) => {
                            self.clock_source_index = Some(at)
                        }
                        Some(toolbar::SimAction::ToggleFreeRun) => {
                            self.free_running_source = !self.free_running_source;
                            // From now, not from whenever it was last on.
                            self.source_beat_at = self.circuit.now();
                        }
                        None => {}
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

        self.canvas_ui(ui);

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
        if self.show_inspector {
            // Built here rather than inside the window, which borrows
            // `self.circuit` and `self.show_inspector` at once otherwise.
            let named = self.named_components(strings);
            let focus: Vec<ComponentId> = self.selection.components.iter().copied().collect();
            let mut open = true;
            crate::inspector::show(
                ui.ctx(),
                strings,
                &self.circuit,
                &named,
                &self.width_faults,
                &focus,
                &mut open,
            );
            self.show_inspector = open;
        }
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
                            egui::RichText::new(strings.about_version.replace(
                                "{}",
                                &format!("{}{BUILD_MARKER}", env!("CARGO_PKG_VERSION")),
                            ))
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
mod ui_tests;

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests;
