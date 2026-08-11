//! A component instance placed on the canvas: where it is, plus whatever each
//! kind needs beyond its `Circuit` registration.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Align2, Color32, Id, Painter, Pos2, Rect, Sense, Ui, Vec2};
use simlogix_core::{Circuit, ComponentId, Level, NetId, PortDrive, PortHandles, Signal};

use crate::appearance::Appearance;
use crate::canvas::{self, Rotation, BOX_SIZE, GRID_SPACING};

/// The room a readout is given beyond the characters themselves — the body
/// it sits in, and whatever the symbol draws beside it.
const READOUT_MARGIN: f32 = GRID_SPACING * 3.0;
use crate::palette::ComponentKind;
use crate::properties::{NumberBase, Properties, DEFAULT_LED_COLOR};
use crate::symbol::{self, SymbolState};

/// A flattened sub-circuit's ports, its own internal connections and how
/// wide its innards' pins are — what `SimLogixApp::flatten` hands back and
/// `Shape::Instance` keeps.
///
/// A struct rather than a tuple of three, and it earned that on the third
/// member: two `Vec`s of pin pairs side by side is a call site where
/// transposing them still compiles.
#[derive(Default)]
pub struct InstanceWiring {
    pub ports: Vec<InstancePort>,
    pub inner_groups: Vec<Vec<(ComponentId, usize)>>,
    pub inner_widths: Vec<((ComponentId, usize), Option<usize>)>,
}

/// The pair the net rebuild reads back off an instance.
pub type InstanceWiringRef<'a> = (&'a [InstancePort], &'a [Vec<(ComponentId, usize)>]);

/// One pin an instance exposes, and where it lands inside the flattened
/// sub-circuit.
#[derive(Debug, Clone)]
pub struct InstancePort {
    pub name: String,
    /// How many bits this port's pin carries, from the port's own
    /// properties inside the sub-circuit. Read there rather than declared
    /// out here: the sub-circuit's boundary is what says how wide it is,
    /// and an instance repeating the number is one that can disagree.
    pub width: usize,
    /// `InputPort`, `OutputPort` or `InOutPort` — which side of the box it
    /// goes on, and which way the arrow points.
    pub kind: ComponentKind,
    /// Which of the sub-circuit's internal nets this port sits on, as an
    /// index into the instance's `inner_groups`.
    ///
    /// The port component itself is never instantiated: its pin was only
    /// ever a *member* of that net, so declaring the instance's own pin a
    /// member of it too is the whole connection. Membership rather than a
    /// list of pins to link to, because a net joining two ports and nothing
    /// else has no pins to offer — and that is exactly a pass-through.
    pub group: Option<usize>,
}

/// The box a component occupies, before rotation.
///
/// Wide enough for whatever readout it has to show, grown in whole grid
/// steps so its pins keep landing on the dots, and about the centre so the
/// symbol stays where it was put. A component with no readout — or a
/// one-character one — is exactly `BOX_SIZE`, so nothing already drawn
/// moves.
///
/// A free function because the hit area and *every draw arm* need it, and
/// the arms have `self` destructured. One definition rather than two: the
/// first version of this grew only `rect()`, so the box you could click
/// grew while the symbol drawn inside it stayed put — the same fault as the
/// rotated component that could not be clicked, made a second time.
fn box_rect(center: Pos2, readout: f32) -> Rect {
    let width = BOX_SIZE.x.max(canvas::snap_up(readout + READOUT_MARGIN));
    Rect::from_center_size(center, egui::vec2(width, BOX_SIZE.y))
}

/// A splitter's box: one grid row per branch, never shorter than an ordinary
/// component.
///
/// A free function because both the hit area and the drawing need it, and
/// the draw arm has `self` destructured. One definition rather than two —
/// which is the whole lesson of the rotated component that could not be
/// clicked: the box and the drawing agreed until they didn't.
fn splitter_rect(center: Pos2, rotation: Rotation, properties: &Properties) -> Rect {
    let rows = properties.branch_widths().len().max(1) as f32;
    let height = (GRID_SPACING * (rows + 1.0)).max(BOX_SIZE.y);
    symbol::rotate_rect(
        Rect::from_center_size(center, egui::vec2(BOX_SIZE.x, height)),
        center,
        rotation,
    )
}

/// A pin's on-canvas hit target this frame: which component/pin it is, where
/// it is, which net it's on, and whether it was clicked this frame (starts
/// or finishes a click-by-click wire — see `SimLogixApp::wiring_from`).
pub struct PinHandle {
    pub component: ComponentId,
    pub pin_index: usize,
    pub position: Pos2,
    pub net: NetId,
    pub clicked: bool,
}

/// What happened while drawing and interacting with a placed component this frame.
#[derive(Default)]
pub struct FrameResult {
    /// This component's id, if it was clicked (not dragged) this frame.
    pub clicked: Option<ComponentId>,
    /// Whether a drag on this component just began. The caller snapshots
    /// for undo here rather than when it ends, since by then the pre-drag
    /// position is many frames gone.
    pub grab_started: bool,
    /// Whether a drag on it just ended, so it has landed somewhere new —
    /// the caller checks whether a pin came to rest on a loose wire end.
    pub settled: bool,
    /// Whether this component's own input changed (a `Button` pressed or
    /// released), so the caller should let the circuit settle. Reported
    /// rather than settled here: whether the simulation is even running is
    /// the app's decision, not an individual component's.
    pub input_changed: bool,
    /// Whether a latching switch was clicked and wants flipping.
    ///
    /// *Reported* rather than done, because a switch's position is document
    /// data: the caller has to snapshot for undo before it changes, and by
    /// the time this returns it would already be too late.
    pub toggled: bool,
    /// How far this component moved under the pointer this frame, zero when
    /// it isn't being dragged. The caller uses it to carry the rest of a
    /// multi-selection along by the same amount.
    pub dragged_by: Vec2,
    pub pins: Vec<PinHandle>,
}

/// How far the component box's own drag/click area is inset from its full
/// symbol rect — half a pin's hit-rect size, so the two never overlap. Pins
/// sit exactly on the box's edge, so without this a click meant for a pin
/// could just as easily be claimed by the box underneath it and move the
/// component instead of starting a wire.
const PIN_HIT_MARGIN: f32 = 7.0;

/// A `Button` needs its pressed handle to react to clicks; a `Led` and a
/// `Probe` need nothing extra — their state is read straight from the net
/// their pin is on. `Transistor`/`Rail` carry which specific `ComponentKind`
/// they are (N/P-type, Ground/Power) since `Circuit` only stores the opaque
/// `Component` trait object and can't tell them apart from the outside.
/// `TwoInputGate` does the same for every 2-input combinational gate
/// (`And`/`Or`/`Nand`/`Nor`/`Xor`/`Xnor`); `OneInputGate` for every 1-input
/// one (`Not`/`Buffer`).
pub struct PlacedComponent {
    id: ComponentId,
    center: Pos2,
    rotation: Rotation,
    /// Reflected left-to-right, before being turned.
    ///
    /// Beside `rotation` rather than in `properties`, because it is the
    /// same kind of fact: where the symbol is put, not what it is set to.
    /// A mirror is not a rotation and no amount of turning stands in for
    /// one — a half turn also reverses the order of a symbol's pins, which
    /// is what a splitter used as a merger must not do.
    mirrored: bool,
    /// What the user has set on this one; absent values mean the behaviour
    /// this component has always had. See `properties.rs`.
    properties: Properties,
    /// What kind of thing it is, and whatever *that* needs to carry.
    shape: Shape,
    /// How much room its readout needs, in points — see `set_readout`.
    /// Zero for everything that shows no value.
    readout: f32,
}

/// The part that actually varies between components.
///
/// `id`, `center`, `rotation` and `properties` were once repeated in every
/// variant, which meant adding a property touched all of them. Only what
/// genuinely differs lives here now.
enum Shape {
    Button {
        pressed: Rc<Cell<bool>>,
    },
    /// The latching counterpart of `Button`: a click flips it and it stays.
    /// Same engine component — the difference lives here, in the gesture.
    Switch {
        on: Rc<Cell<bool>>,
    },
    Led,
    Transistor(ComponentKind),
    Rail(ComponentKind),
    Probe,
    Clock,
    /// Any 2-input, 1-output, stateless combinational gate: two inputs at
    /// pin indices 0/1, one output at pin index 2. Adding a new gate of this
    /// shape needs no new variant here: add the `ComponentKind`, a core
    /// `Component` impl, a `draw_xxx` in `symbol.rs`, and a `place()` arm in
    /// `app.rs` that calls [`PlacedComponent::two_input_gate`].
    TwoInputGate(ComponentKind),
    /// The 1-input mirror of `TwoInputGate` (`Not`/`Buffer`), via
    /// [`PlacedComponent::one_input_gate`].
    OneInputGate(ComponentKind),
    /// An instance of another circuit: an anchor pin per port, with the
    /// referenced circuit's own components already built into the engine
    /// but living outside `placed`.
    Instance {
        path: String,
        ports: Vec<InstancePort>,
        /// The referenced circuit's own connectivity, which the net rebuild
        /// has to re-apply — it derives everything else from the *open*
        /// drawing, and these wires aren't in it.
        inner_groups: Vec<Vec<(ComponentId, usize)>>,
        /// What each of the innards' pins declares, since they are not in
        /// the drawing for `rebuild_nets` to ask.
        inner_widths: Vec<((ComponentId, usize), Option<usize>)>,
        /// What it looks like: the circuit's own symbol if it has one, and
        /// otherwise the generated box. Resolved once, when the instance is
        /// built, rather than worked out again on every frame.
        appearance: Appearance,
    },
    /// One pin carrying a level you set by hand, in three positions: the
    /// circuit boundary ports, and the plain tri-state source. `level` is
    /// absent on an output port, the one that only ever reads.
    ///
    /// Clicking *latches*, unlike a `Button`: a port stands for what a
    /// parent will drive, and a source for a switch that stays where it is
    /// put — neither springs back.
    HandSet {
        kind: ComponentKind,
        /// Absent on an output port, the one that only ever reads.
        handles: Option<PortHandles>,
    },
    /// A fixed value on one output pin.
    ///
    /// Its own variant rather than a `HandSet`: that one is *defined* by
    /// its click cycle, and a constant has none — its value is a property,
    /// typed rather than clicked through. The handles are how that property
    /// reaches the engine, exactly as a port's do.
    Constant {
        handles: PortHandles,
    },
    /// A bus at pin 0 and one branch per pin after it. How many there are
    /// comes from the properties, so the box grows with them — which is
    /// also why changing that has to go through the document: a built
    /// component's pins are fixed.
    Splitter,
    /// Two `InOut` bus sides at pin indices 0/1 (`A`, `B`) and two control
    /// inputs at 2/3 (`Dir`, `Enable`) — the only component whose pins both
    /// read and drive.
    BusTransceiver(ComponentKind),
    /// Two inputs at pin indices 0/1 (`S`, `R`) and *two* outputs at 2/3
    /// (`Q`, `Q̄`) — the first component with more than one output, which is
    /// why it doesn't fit `TwoInputGate`.
    SrLatch,
}

impl PlacedComponent {
    fn new(id: ComponentId, center: Pos2, shape: Shape) -> Self {
        Self {
            id,
            center,
            rotation: Rotation::default(),
            mirrored: false,
            properties: Properties::default(),
            shape,
            readout: 0.0,
        }
    }

    pub fn button(id: ComponentId, center: Pos2, pressed: Rc<Cell<bool>>) -> Self {
        Self::new(id, center, Shape::Button { pressed })
    }

    pub fn switch(id: ComponentId, center: Pos2, on: Rc<Cell<bool>>) -> Self {
        Self::new(id, center, Shape::Switch { on })
    }

    pub fn led(id: ComponentId, center: Pos2) -> Self {
        Self::new(id, center, Shape::Led)
    }

    pub fn transistor(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::new(id, center, Shape::Transistor(kind))
    }

    pub fn rail(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::new(id, center, Shape::Rail(kind))
    }

    pub fn probe(id: ComponentId, center: Pos2) -> Self {
        Self::new(id, center, Shape::Probe)
    }

    pub fn clock(id: ComponentId, center: Pos2) -> Self {
        Self::new(id, center, Shape::Clock)
    }

    pub fn two_input_gate(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::new(id, center, Shape::TwoInputGate(kind))
    }

    pub fn one_input_gate(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::new(id, center, Shape::OneInputGate(kind))
    }

    pub fn sr_latch(id: ComponentId, center: Pos2) -> Self {
        Self::new(id, center, Shape::SrLatch)
    }

    pub fn hand_set(
        id: ComponentId,
        center: Pos2,
        kind: ComponentKind,
        handles: Option<PortHandles>,
    ) -> Self {
        Self::new(id, center, Shape::HandSet { kind, handles })
    }

    /// How much room this component's readout needs, in points.
    ///
    /// Refreshed every frame from the net's width and the base in force,
    /// rather than stored and kept in step: a base is a setting that can
    /// change without the drawing changing, and a probe's width comes from
    /// a net that is reallocated on every edit. The same bargain the wire
    /// routes make — derived each frame, so it cannot go stale.
    pub fn set_readout(&mut self, width: f32) {
        self.readout = width;
    }

    pub fn constant(id: ComponentId, center: Pos2, handles: PortHandles) -> Self {
        // A constant always drives — that is the whole of what it is — and
        // the cell it shares with the engine starts undriven, since it is a
        // port's cell. Set here rather than in each caller: placing one from
        // the palette, loading one, pasting one and flattening one would
        // each have to remember, and the one that forgot would put a
        // constant on the canvas driving nothing at all.
        handles.drive.set(PortDrive::Driving(0));
        Self::new(id, center, Shape::Constant { handles })
    }

    pub fn instance(
        id: ComponentId,
        center: Pos2,
        path: String,
        wiring: InstanceWiring,
        appearance: Appearance,
    ) -> Self {
        Self::new(
            id,
            center,
            Shape::Instance {
                path,
                ports: wiring.ports,
                inner_groups: wiring.inner_groups,
                inner_widths: wiring.inner_widths,
                appearance,
            },
        )
    }

    /// Points an instance at a circuit that has moved or been renamed.
    ///
    /// Nothing is rebuilt: the circuit's *contents* haven't changed, only
    /// what it's called, so the flattened innards stay exactly as they are.
    pub fn repoint_instance(&mut self, from: &str, to: &str) {
        if let Shape::Instance { path, .. } = &mut self.shape {
            if path == from {
                *path = to.to_string();
            }
        }
    }

    /// What this instance is made of, when it is one — the net rebuild needs
    /// both halves.
    pub fn instance_wiring(&self) -> Option<InstanceWiringRef<'_>> {
        match &self.shape {
            Shape::Instance {
                ports,
                inner_groups,
                ..
            } => Some((ports, inner_groups)),
            _ => None,
        }
    }

    /// How wide each pin of this instance's *innards* is.
    ///
    /// Those components are not in the drawing — they were built into the
    /// engine and their `PlacedComponent`s dropped — so nothing else can be
    /// asked. Carried up with the wiring, and for the same reason: the
    /// record would otherwise go with the entry that held it.
    pub fn inner_pin_widths(&self) -> &[((ComponentId, usize), Option<usize>)] {
        match &self.shape {
            Shape::Instance { inner_widths, .. } => inner_widths,
            _ => &[],
        }
    }

    /// The box this component occupies. Everything is one grid box except an
    /// instance, which grows downward with the number of pins it must show.
    pub fn rect(&self) -> Rect {
        if let Shape::Instance { appearance, .. } = &self.shape {
            // A symbol you drew decides its own extent; the generated box
            // reports exactly what it always did.
            return appearance.rect(self.center, self.rotation);
        }
        if matches!(self.shape, Shape::Splitter) {
            // One grid row per branch, and never shorter than a box: the
            // pins are laid out on that step, so the extent has to follow
            // however many there are.
            let rows = self.properties.branch_widths().len().max(1) as f32;
            let height = (GRID_SPACING * (rows + 1.0)).max(BOX_SIZE.y);
            return symbol::rotate_rect(
                Rect::from_center_size(self.center, egui::vec2(BOX_SIZE.x, height)),
                self.center,
                self.rotation,
            );
        }
        let box_rect = box_rect(self.center, self.readout);
        if symbol::keeps_upright(&self.kind()) {
            // A symbol that is mostly a readout keeps its body upright and
            // moves only its pin, so its box does not turn either — turning
            // it would leave a tall narrow hit area around a wide symbol.
            return box_rect;
        }
        // Turned too, and for the same reason: the box is not square, so a
        // component on its side occupies the other way round. This one still
        // covered its own centre, so it was a mis-shaped hit area rather than
        // an absent one — visible in what the selection outline drew and in
        // what a rubber band caught.
        symbol::rotate_rect(box_rect, self.center, self.rotation)
    }

    pub fn splitter(id: ComponentId, center: Pos2) -> Self {
        Self::new(id, center, Shape::Splitter)
    }

    pub fn bus_transceiver(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::new(id, center, Shape::BusTransceiver(kind))
    }

    pub fn id(&self) -> ComponentId {
        self.id
    }

    pub fn center(&self) -> Pos2 {
        self.center
    }

    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// The level cell of a component whose value is set by hand — the
    /// driving ports and the tri-state source.
    ///
    /// `None` for everything else, including an output port, which only
    /// reads. A `Switch` is deliberately not one of these: its position is
    /// part of the saved document, so anything driving it from outside
    /// would be making an edit.
    pub fn hand_set_level(&self) -> Option<&Rc<Cell<PortDrive>>> {
        match &self.shape {
            Shape::HandSet { handles, .. } => handles.as_ref().map(|handles| &handles.drive),
            _ => None,
        }
    }

    /// Where a `Switch` is **now**, as the engine reads it.
    ///
    /// Runtime state, like a port's drive: its property says where it
    /// *rests* when the project opens, and flipping it by hand is not an
    /// edit to the drawing. `None` for everything else.
    pub fn switch_position(&self) -> Option<&Rc<Cell<bool>>> {
        match &self.shape {
            Shape::Switch { on } => Some(on),
            _ => None,
        }
    }

    /// The width its pins carry, as its properties say.
    pub fn width(&self) -> usize {
        self.properties.width()
    }

    /// How many bits pin `index` **declares**, and `None` when it declares
    /// nothing and simply takes whatever its net carries.
    ///
    /// Per *pin*, because a component's pins are not always alike: a
    /// tri-state buffer's enable and a transceiver's direction are one bit
    /// whatever the data beside them is, and an instance's pins are as wide
    /// as the ports they stand for, one by one.
    ///
    /// `None` is not "one bit" — it is a pin with no opinion, so it never
    /// widens a net and can never disagree with one. A `Probe` is the case
    /// it exists for: it is an instrument reading a net, not a part of the
    /// circuit, and telling it a width is how it would come to show
    /// something the net does not say.
    ///
    /// A kind that is never offered the setting answers one whatever its
    /// properties happen to hold — a width pasted onto a LED is not a claim
    /// it ever made.
    pub fn pin_width(&self, index: usize) -> Option<usize> {
        let declared = self.properties.width();
        match &self.shape {
            Shape::Probe => None,
            Shape::Instance { ports, .. } => Some(ports.get(index).map_or(1, |port| port.width)),
            // It shares `TwoInputGate`'s shape but not its pins: the enable
            // at index 1 is one bit whatever passes through.
            Shape::TwoInputGate(kind) if *kind == ComponentKind::TriStateBuffer => {
                Some(if index == 1 { 1 } else { declared })
            }
            // `A` and `B` carry the data; `Dir` and the enable do not.
            Shape::BusTransceiver(_) => Some(if index < 2 { declared } else { 1 }),
            // Pin 0 is the bus, as wide as it says; each branch after it
            // carries its own share.
            Shape::Splitter => Some(match index.checked_sub(1) {
                None => declared,
                Some(branch) => self
                    .properties
                    .branch_widths()
                    .get(branch)
                    .copied()
                    .unwrap_or(1),
            }),
            _ if Properties::has_width(&self.kind()) => Some(declared),
            _ => Some(1),
        }
    }

    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    pub fn set_properties(&mut self, properties: Properties) {
        // A port's resting level is a property, so it has to reach the cell
        // the engine reads — on load and whenever it's edited. Same idea as
        // a button's `pressed`, except a latching port has no "held" state
        // to settle itself from, so it's pushed explicitly.
        if let Shape::HandSet {
            handles: Some(handles),
            ..
        } = &self.shape
        {
            if properties.initial_level() != self.properties.initial_level() {
                handles
                    .drive
                    .set(properties.initial_level().to_drive(properties.width()));
            }
            // Unconditional: the component has to drive the width the net
            // was given, and a port claiming a width it does not supply
            // faults every bit of that net.
            handles.width.set(properties.width());
        }
        // A constant's value is a property and nothing else writes it, so
        // it is pushed the same way — and unconditionally, since the width
        // it drives has to be the one the net was given.
        if let Shape::Constant { handles } = &self.shape {
            handles.width.set(properties.width());
            handles
                .drive
                .set(PortDrive::Driving(properties.constant_value()));
        }
        // A switch needs the same push for the same reason: it latches, so
        // there is no "held" state for it to settle itself from the way a
        // button does. Its property is where it *rests*, so setting one
        // puts the switch there — the same as a port's resting level.
        if let Shape::Switch { on } = &self.shape {
            let at_rest = properties.pressed.unwrap_or(false);
            if at_rest != self.properties.pressed.unwrap_or(false) {
                on.set(at_rest);
            }
        }
        self.properties = properties;
    }

    /// Which palette entry would place an identical component — what a
    /// project file needs to reconstruct this on load.
    pub fn kind(&self) -> ComponentKind {
        match &self.shape {
            Shape::Button { .. } => ComponentKind::Button,
            Shape::Switch { .. } => ComponentKind::Switch,
            Shape::Led => ComponentKind::Led,
            Shape::HandSet { kind, .. } => kind.clone(),
            Shape::Constant { .. } => ComponentKind::Constant,
            Shape::Splitter => ComponentKind::Splitter,
            Shape::Instance { path, .. } => ComponentKind::Circuit(path.clone()),
            Shape::Transistor(kind)
            | Shape::BusTransceiver(kind)
            | Shape::Rail(kind)
            | Shape::TwoInputGate(kind)
            | Shape::OneInputGate(kind) => kind.clone(),
            Shape::Probe => ComponentKind::Probe,
            Shape::Clock => ComponentKind::Clock,
            Shape::SrLatch => ComponentKind::SrLatch,
        }
    }

    /// Shifts this component by `delta` — how the rest of a multi-selection
    /// follows the one actually under the pointer.
    pub fn move_by(&mut self, delta: Vec2) {
        self.center += delta;
    }

    /// Puts this component back on the grid. Called when a drag ends rather
    /// than every frame, for the same reason `interact_box` does it: snapping
    /// mid-drag feels jerky.
    pub fn snap(&mut self) {
        self.center = canvas::snap_to_grid(self.center);
    }

    /// Whether it is reflected left-to-right.
    pub fn is_mirrored(&self) -> bool {
        self.mirrored
    }

    /// Reflects it, or stops. Nothing else moves: a mirror about the
    /// centre maps the box onto itself, so the hit area is untouched.
    pub fn set_mirrored(&mut self, mirrored: bool) {
        self.mirrored = mirrored;
    }

    /// Sets this component's rotation directly (used when loading a project).
    pub fn set_rotation(&mut self, new_rotation: Rotation) {
        self.rotation = new_rotation;
    }

    /// Rotates this component a quarter-turn clockwise.
    pub fn rotate(&mut self) {
        self.set_rotation(self.rotation().next_clockwise());
    }

    /// Draws this component at its current position, highlighted if
    /// `selected` names it, and reacts to the mouse: dragging the box moves it
    /// (snapped to the grid), while a plain press/release on a `Button`
    /// re-schedules and re-runs `circuit` so the change is visible the same
    /// frame. Each pin also gets its own small hit target, returned so the
    /// caller can turn clicks on two pins into a wire. The canvas shows
    /// only the component's symbol (`symbol::draw`) — no text label except
    /// `Probe`'s own state readout, which is its whole purpose. The palette
    /// is where every other component's name shows up.
    pub fn draw_and_interact(
        &mut self,
        ui: &mut Ui,
        painter: &Painter,
        circuit: &mut Circuit,
        is_selected: bool,
        // False while the circuit is being watched rather than built:
        // nothing may be moved, and no pin may start a wire.
        movable: bool,
        // The base to show a value in when this component doesn't name one
        // of its own. Handed in rather than read here: it is a setting, and
        // a component has no way to reach one.
        default_base: NumberBase,
    ) -> FrameResult {
        let id = self.id();
        let kind = self.kind();
        let base = self.properties.base.unwrap_or(default_base);
        // Destructured up front so the borrow of `center`/`rotation` and the
        // borrow of `shape` are disjoint -- matching on `self` while also
        // handing `&mut self.center` to `interact_box` wouldn't compile.
        let PlacedComponent {
            center,
            rotation,
            mirrored,
            properties,
            shape,
            readout,
            ..
        } = self;
        let orientation = symbol::Orientation::new(*rotation, *mirrored);
        // Named apart from the `readout` *string* one arm builds: this is
        // the room it needs, not what it says.
        let readout_room = *readout;

        let rect_id = Id::new(("placed", id));
        let mut input_changed = false;

        // Taken from the theme rather than fixed: symbols are drawn straight
        // onto the canvas with nothing behind them, so a light grey that
        // reads well on the dark background all but vanishes on the light one.
        let symbol_color = ui.visuals().strong_text_color();
        let dark_mode = ui.visuals().dark_mode;
        let off_color = ui.visuals().weak_text_color();
        // Labels are painted outside the canvas's transform so they stay
        // sharp at any zoom — see `symbol::TextLayer`.
        let text_layer = symbol::TextLayer::for_ui(ui);

        // A name the user set is drawn under the symbol — once here rather
        // than once per arm, since every kind can carry one. This is the
        // deliberate exception to "symbols carry no text": an annotation you
        // wrote is not a label the editor generated for you.
        if let Some(label) = properties.label() {
            text_layer.text(
                box_rect(*center, readout_room).center_bottom() + egui::vec2(0.0, 2.0),
                Align2::CENTER_TOP,
                label,
                11.0,
                symbol_color,
            );
        }

        match shape {
            Shape::Button { pressed } => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState {
                        pressed: pressed.get(),
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);
                // Press/release only counts while not dragging, so moving a
                // button across the canvas never also toggles it.
                if !response.dragged() {
                    // The property is the state the button *rests* in, and a
                    // press inverts it — which is exactly what makes a
                    // "pressed at rest" button one that clicking releases.
                    // It settles itself too: on the first frame after a load
                    // nothing is held, so this drives the cell to the resting
                    // state without a separate initialisation.
                    let at_rest = properties.pressed.unwrap_or(false);
                    let held = response.is_pointer_button_down_on();
                    let wanted = held != at_rest;
                    if wanted != pressed.get() {
                        pressed.set(wanted);
                        circuit.schedule_now(id);
                        input_changed = true;
                    }
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::Switch { on } => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState {
                        pressed: on.get(),
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);
                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    // Only on a click: dragging one across the canvas must
                    // not also flip it.
                    toggled: response.clicked(),
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::Led => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or_default();
                // A LED lights on a definite high — on a bus, only when
                // every bit of it is high, since a LED has one lamp and
                // cannot report eight different bits.
                let color = if signal
                    .levels()
                    .iter()
                    .all(|&l| l.strengthened() == Level::High)
                {
                    let [r, g, b] = properties.color.unwrap_or(DEFAULT_LED_COLOR);
                    Color32::from_rgb(r, g, b)
                } else {
                    off_color
                };
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::BusTransceiver(_) => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                // Pin order is A, B, Dir, Enable; the symbol reports the two
                // bus sides as its outputs and the two controls as inputs.
                let circuit_pins = circuit.pins(id);
                let pins = vec![
                    pin_handle(
                        ui,
                        painter,
                        id,
                        0,
                        pin_positions.outputs[0],
                        circuit_pins[0].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        1,
                        pin_positions.outputs[1],
                        circuit_pins[1].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        2,
                        pin_positions.inputs[0],
                        circuit_pins[2].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        3,
                        pin_positions.inputs[1],
                        circuit_pins[3].net,
                        movable,
                    ),
                ];

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins,
                }
            }
            Shape::Instance {
                path,
                ports,
                appearance,
                ..
            } => {
                let rect = appearance.rect(*center, *rotation);
                let pin_positions = symbol::draw_instance(
                    painter,
                    *center,
                    orientation,
                    symbol_color,
                    path,
                    ports,
                    appearance,
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                // One anchor pin per port, in the order the box lays them
                // out, which is the order `flatten` sorted them into.
                let circuit_pins = circuit.pins(id);
                let pins = pin_positions
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(index, at)| {
                        Some(pin_handle(
                            ui,
                            painter,
                            id,
                            index,
                            *at,
                            circuit_pins.get(index)?.net,
                            movable,
                        ))
                    })
                    .collect();

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins,
                }
            }
            Shape::HandSet { handles, .. } => {
                // Every port shows what its net resolves to, driving or not:
                // on an output that's the whole point, and on the other two
                // it's what tells you a value you set is being fought over.
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or_default();
                let readout = signal_text(&signal, base);
                // The readout follows the signal, the body doesn't: which way
                // the value crosses the boundary is structure and shouldn't
                // change colour as the circuit runs.
                let mut readout_color = canvas::bus_color(&signal, dark_mode);
                if circuit
                    .pins(id)
                    .first()
                    .is_some_and(|pin| circuit.is_weakly_driven(pin.net))
                {
                    readout_color = readout_color.gamma_multiply(canvas::WEAK_FADE);
                }

                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState {
                        label: &readout,
                        label_color: Some(readout_color),
                        level: handles.as_ref().map(|handles| handles.drive.get()),
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);
                // Latching: a click advances it and it stays there. Only on
                // a click, never on a drag, so moving a port across the
                // canvas can't also change what it carries.
                //
                // **And only while the circuit is being watched.** A click
                // in the schematic *selects* — to set a width, a name, a
                // base — and driving on the same gesture meant every such
                // click also poked the circuit. What it carries is runtime
                // state, so changing it belongs to the mode that exists for
                // running the thing. A `Switch` is deliberately not like
                // this: its position is part of the document, so flipping
                // one is an edit and belongs where edits are made.
                if let Some(drive) = handles.as_ref().map(|handles| &handles.drive) {
                    if response.clicked() && !movable {
                        drive.set(
                            drive
                                .get()
                                .next(properties.cycles_undriven(&kind), properties.width()),
                        );
                        circuit.schedule_now(id);
                        input_changed = true;
                    }
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::SrLatch => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let circuit_pins = circuit.pins(id);
                let pins = vec![
                    pin_handle(
                        ui,
                        painter,
                        id,
                        0,
                        pin_positions.inputs[0],
                        circuit_pins[0].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        1,
                        pin_positions.inputs[1],
                        circuit_pins[1].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        2,
                        pin_positions.outputs[0],
                        circuit_pins[2].net,
                        movable,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        3,
                        pin_positions.outputs[1],
                        circuit_pins[3].net,
                        movable,
                    ),
                ];

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins,
                }
            }
            Shape::Transistor(_) => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let circuit_pins = circuit.pins(id);
                let gate = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                    movable,
                );
                let source = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.inputs[1],
                    circuit_pins[1].net,
                    movable,
                );
                let drain = pin_handle(
                    ui,
                    painter,
                    id,
                    2,
                    pin_positions.outputs[0],
                    circuit_pins[2].net,
                    movable,
                );

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![gate, source, drain],
                }
            }
            Shape::Rail(_) => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::Constant { .. } => {
                // Its own value, not the net's: a constant is not reporting
                // what it sees, it is saying what it puts there.
                let label = crate::properties::format_value(
                    u128::from(properties.constant_value()),
                    properties.width(),
                    base,
                    false,
                );
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState {
                        label: &label,
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);
                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::Splitter => {
                let rect = splitter_rect(*center, *rotation, properties);
                let branches = properties.branch_widths();
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState {
                        branches: &branches,
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);
                let nets: Vec<NetId> = circuit.pins(id).iter().map(|pin| pin.net).collect();
                let mut pins = Vec::with_capacity(nets.len());
                let positions = pin_positions
                    .inputs
                    .iter()
                    .chain(pin_positions.outputs.iter());
                for (index, (position, net)) in positions.zip(nets).enumerate() {
                    pins.push(pin_handle(ui, painter, id, index, *position, net, movable));
                }

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins,
                }
            }
            Shape::Probe => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or_default();
                // A probe reads out the net it's attached to, so it uses the
                // very colour code that net is drawn in — its own duplicate
                // of the five states was the one place they could disagree.
                let mut color = canvas::bus_color(&signal, dark_mode);
                if circuit
                    .pins(id)
                    .first()
                    .is_some_and(|pin| circuit.is_weakly_driven(pin.net))
                {
                    color = color.gamma_multiply(canvas::WEAK_FADE);
                }
                let label = signal_text(&signal, base);
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    color,
                    SymbolState {
                        label: &label,
                        ..Default::default()
                    },
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::Clock => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or_default();
                // A clock is a signal source, so its symbol follows the same
                // colour code as the wire it drives (`canvas::signal_color`)
                // rather than a lit/unlit one of its own — green while high,
                // not red.
                let color = canvas::bus_color(&signal, dark_mode);
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net, movable);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![pin],
                }
            }
            Shape::TwoInputGate(_) => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let circuit_pins = circuit.pins(id);
                let a = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                    movable,
                );
                let b = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.inputs[1],
                    circuit_pins[1].net,
                    movable,
                );
                let out = pin_handle(
                    ui,
                    painter,
                    id,
                    2,
                    pin_positions.outputs[0],
                    circuit_pins[2].net,
                    movable,
                );

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![a, b, out],
                }
            }
            Shape::OneInputGate(_) => {
                let rect = box_rect(*center, readout_room);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    orientation,
                    symbol_color,
                    SymbolState::default(),
                    &text_layer,
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center, movable);

                let circuit_pins = circuit.pins(id);
                let input = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                    movable,
                );
                let output = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.outputs[0],
                    circuit_pins[1].net,
                    movable,
                );

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    grab_started: response.drag_started(),
                    settled: response.drag_stopped(),
                    input_changed,
                    toggled: false,
                    dragged_by: applied_drag(&response),
                    pins: vec![input, output],
                }
            }
        }
    }
}

/// The drag-to-move handle every placed component shares, plus its hover
/// feedback. Each `draw_and_interact` arm still gets the `Response` back so
/// it can layer on its own behavior (a `Button`'s press, for instance).
///
/// The interactive area is inset by [`PIN_HIT_MARGIN`] so it never overlaps
/// a pin's own hit-rect — see that constant.
/// The one-character readout a `Probe` or a port shows for a signal.
/// What a whole signal reads as, in as few characters as possible.
///
/// A plain wire keeps its single letter. A bus cannot: `only_level` answers
/// `Error` for anything wider, which is right for a *component* that has no
/// meaning on a bus and quite wrong for a readout — it made a perfectly
/// healthy two-bit port show `E`.
///
/// The exception dominates, in the order a schematic is read: a fault
/// anywhere is `E`, then anything unknown is `?`, then a bus nobody drives
/// is `Z`. Only once every bit is a definite level is there a *value* to
/// show, and then it is shown in hex, least significant bit first being
/// what `Signal` already promises.
pub fn signal_text(signal: &Signal, base: NumberBase) -> String {
    if signal.width() == 1 {
        return signal_letter(signal.only_level()).to_string();
    }
    let levels: Vec<Level> = signal
        .levels()
        .iter()
        .map(|level| level.strengthened())
        .collect();
    if levels.contains(&Level::Error) {
        return "E".to_string();
    }
    if levels
        .iter()
        .any(|&level| level != Level::High && level != Level::Low && level != Level::HighZ)
    {
        return "?".to_string();
    }
    if levels.iter().all(|&level| level == Level::HighZ) {
        return "Z".to_string();
    }
    if levels.contains(&Level::HighZ) {
        return "?".to_string();
    }
    let value: u128 = levels
        .iter()
        .enumerate()
        .filter(|(_, &level)| level == Level::High)
        .map(|(bit, _)| 1u128 << bit.min(127))
        .sum();
    // The same rule a typed value is written by, minus the prefix: a
    // readout on a schematic is read, not retyped.
    crate::properties::format_value(value, signal.width(), base, false)
}

fn signal_letter(signal: Level) -> &'static str {
    // A net never resolves to a weak level, so those arms are unreachable;
    // reading them as their full-strength selves is the only answer that
    // could ever be right.
    match signal.strengthened() {
        Level::High => "1",
        Level::Low => "0",
        Level::Error => "E",
        Level::HighZ => "Z",
        _ => "?",
    }
}

/// How tall an instance's box has to be to show its pins, always a whole
/// number of grid steps so every pin lands on a dot.
pub fn instance_height(ports: &[InstancePort]) -> f32 {
    let outputs = ports
        .iter()
        .filter(|port| port.kind == ComponentKind::OutputPort)
        .count();
    let per_side = outputs.max(ports.len() - outputs);

    // Pins sit whole grid steps below the box's top edge, and that edge is
    // `centre - height / 2`. So it is *half* the height that has to be a
    // whole number of steps: an odd count would put the top edge — and every
    // pin with it — between two dots.
    let steps = (per_side + 1).max(2);
    let steps = steps + steps % 2;
    BOX_SIZE.y.max(steps as f32 * canvas::GRID_SPACING)
}

/// How far `interact_box` actually moved a component this frame.
///
/// Zero on the frame a drag *starts*, because `interact_box` deliberately
/// doesn't move then — reporting the delta anyway would shift the rest of a
/// multi-selection by an amount the grabbed component never took, and the
/// group would come apart by exactly that much.
fn applied_drag(response: &egui::Response) -> Vec2 {
    if response.dragged() && !response.drag_started() {
        response.drag_delta()
    } else {
        Vec2::ZERO
    }
}

/// `movable` is false while a circuit is being watched rather than built.
/// The box then senses clicks but not drags, so a button or a switch still
/// answers one and nothing can be nudged out of place by a click that
/// travelled a pixel — which is the whole point of the simulation mode.
fn interact_box(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    rect_id: Id,
    center: &mut Pos2,
    movable: bool,
) -> egui::Response {
    let sense = if movable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let response = ui.interact(rect.shrink(PIN_HIT_MARGIN), rect_id, sense);

    if !movable {
        if response.hovered() {
            // Outlined, but with no grab cursor: nothing here can be picked
            // up, and saying otherwise would be a promise the mode breaks.
            canvas::draw_hover_outline(painter, rect, ui.visuals().weak_text_color());
        }
        return response;
    }

    if response.drag_started() {
        // Deliberately no movement on this one frame: the caller snapshots
        // for undo when it sees `grab_started`, and leaving the position
        // alone is what makes that snapshot the true pre-drag state. The
        // delta skipped here is at most egui's drag threshold, and the
        // position snaps to the grid on release anyway.
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.dragged() {
        *center += response.drag_delta();
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.hovered() {
        canvas::draw_hover_outline(painter, rect, ui.visuals().weak_text_color());
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    // Snap only on release: snapping every frame mid-drag feels jerky.
    if response.drag_stopped() {
        *center = canvas::snap_to_grid(*center);
    }

    response
}

/// A small, separate interactive hit area right at a pin's tip, distinct from
/// the component box's own drag area (pins are drawn just outside the box).
/// Hovering one draws a ring around it — without that cue there's no way to
/// tell a pin is a wiring target until you've already clicked it.
fn pin_handle(
    ui: &mut Ui,
    painter: &Painter,
    component: ComponentId,
    pin_index: usize,
    position: Pos2,
    net: NetId,
    wiring: bool,
) -> PinHandle {
    let hit_rect = Rect::from_center_size(position, egui::vec2(14.0, 14.0));
    // `hover()` reports nothing clickable, so a pin cannot start a wire
    // while the circuit is only being watched.
    let sense = if wiring {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(hit_rect, Id::new(("pin", component, pin_index)), sense);

    if response.hovered() {
        painter.circle_stroke(
            position,
            6.0,
            egui::Stroke::new(1.5, canvas::accent_color(ui.visuals().dark_mode)),
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }

    PinHandle {
        component,
        pin_index,
        position,
        net,
        clicked: response.clicked(),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_bus_does_not_read_as_an_error() {
        // The bug this exists for: `only_level` answers `Error` for
        // anything wider than a bit — right for a component with no meaning
        // on a bus, and quite wrong for a readout. A two-bit port sitting
        // there undriven showed a red `E`.
        let undriven = Signal::splat(Level::Unknown, 2);
        assert_eq!(signal_text(&undriven, NumberBase::Auto), "?");

        // Every bit definite is a *value*, in hex, bit 0 least significant.
        let five = Signal::from_levels(vec![Level::High, Level::Low, Level::High]);
        assert_eq!(signal_text(&five, NumberBase::Auto), "5");

        // And a fault anywhere still dominates, which is the order a
        // schematic is read in.
        let faulted = Signal::from_levels(vec![Level::High, Level::Error]);
        assert_eq!(signal_text(&faulted, NumberBase::Auto), "E");

        // A plain wire is untouched.
        assert_eq!(
            signal_text(&Signal::bit(Level::High), NumberBase::Auto),
            "1"
        );
    }

    fn port(kind: ComponentKind) -> InstancePort {
        InstancePort {
            name: String::new(),
            kind,
            width: 1,
            group: None,
        }
    }

    #[test]
    fn an_instance_box_always_puts_its_pins_on_the_grid() {
        // Pins are placed whole steps below the top edge, which is
        // `centre - height / 2` -- so half the height has to be a whole
        // number of steps, or every pin lands between two dots.
        for count in 1..8 {
            let ports: Vec<InstancePort> =
                (0..count).map(|_| port(ComponentKind::InputPort)).collect();
            let height = instance_height(&ports);
            let half_steps = height / 2.0 / canvas::GRID_SPACING;
            assert_eq!(
                half_steps,
                half_steps.round(),
                "{count} ports gave a height of {height}"
            );
            // And it has to be tall enough for the last pin to fit inside.
            assert!(height >= (count as f32 + 1.0) * canvas::GRID_SPACING);
        }
    }

    #[test]
    fn the_two_sides_are_counted_separately() {
        // Three in and one out needs room for three, not for four.
        let ports = vec![
            port(ComponentKind::InputPort),
            port(ComponentKind::InputPort),
            port(ComponentKind::InputPort),
            port(ComponentKind::OutputPort),
        ];
        assert_eq!(instance_height(&ports), instance_height(&ports[..3]));
    }
}
