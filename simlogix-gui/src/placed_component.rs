//! A component instance placed on the canvas: where it is, plus whatever each
//! kind needs beyond its `Circuit` registration.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Align2, Color32, Id, Painter, Pos2, Rect, Sense, Ui, Vec2};
use simlogix_core::{Circuit, ComponentId, NetId, PortLevel, Signal};

use crate::appearance::Appearance;
use crate::canvas::{self, Rotation, BOX_SIZE};
use crate::palette::ComponentKind;
use crate::properties::{Properties, DEFAULT_LED_COLOR};
use crate::symbol::{self, SymbolState};

/// A flattened sub-circuit's ports and its own internal connections — what
/// `SimLogixApp::flatten` hands back and `Shape::Instance` keeps.
pub type InstanceWiring = (Vec<InstancePort>, Vec<Vec<(ComponentId, usize)>>);

/// The same pair, borrowed — what the net rebuild reads back off an instance.
pub type InstanceWiringRef<'a> = (&'a [InstancePort], &'a [Vec<(ComponentId, usize)>]);

/// One pin an instance exposes, and where it lands inside the flattened
/// sub-circuit.
#[derive(Debug, Clone)]
pub struct InstancePort {
    pub name: String,
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
    /// What the user has set on this one; absent values mean the behaviour
    /// this component has always had. See `properties.rs`.
    properties: Properties,
    /// What kind of thing it is, and whatever *that* needs to carry.
    shape: Shape,
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
        level: Option<Rc<Cell<PortLevel>>>,
    },
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
            properties: Properties::default(),
            shape,
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
        level: Option<Rc<Cell<PortLevel>>>,
    ) -> Self {
        Self::new(id, center, Shape::HandSet { kind, level })
    }

    pub fn instance(
        id: ComponentId,
        center: Pos2,
        path: String,
        ports: Vec<InstancePort>,
        inner_groups: Vec<Vec<(ComponentId, usize)>>,
        appearance: Appearance,
    ) -> Self {
        Self::new(
            id,
            center,
            Shape::Instance {
                path,
                ports,
                inner_groups,
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

    /// The box this component occupies. Everything is one grid box except an
    /// instance, which grows downward with the number of pins it must show.
    pub fn rect(&self) -> Rect {
        if let Shape::Instance { appearance, .. } = &self.shape {
            // A symbol you drew decides its own extent; the generated box
            // reports exactly what it always did.
            return appearance.rect(self.center, self.rotation);
        }
        // Turned too, and for the same reason: the box is not square, so a
        // component on its side occupies the other way round. This one still
        // covered its own centre, so it was a mis-shaped hit area rather than
        // an absent one — visible in what the selection outline drew and in
        // what a rubber band caught.
        symbol::rotate_rect(
            Rect::from_center_size(self.center, egui::vec2(BOX_SIZE.x, BOX_SIZE.y)),
            self.center,
            self.rotation,
        )
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
    pub fn hand_set_level(&self) -> Option<&Rc<Cell<PortLevel>>> {
        match &self.shape {
            Shape::HandSet { level, .. } => level.as_ref(),
            _ => None,
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
            level: Some(level), ..
        } = &self.shape
        {
            if properties.initial_level() != self.properties.initial_level() {
                level.set(properties.initial_level());
            }
        }
        // A switch needs the same push for the same reason: it latches, so
        // there is no "held" state for it to settle itself from the way a
        // button does.
        if let Shape::Switch { on } = &self.shape {
            let starts_on = properties.pressed.unwrap_or(false);
            if starts_on != self.properties.pressed.unwrap_or(false) {
                on.set(starts_on);
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
    ) -> FrameResult {
        let id = self.id();
        let kind = self.kind();
        // Destructured up front so the borrow of `center`/`rotation` and the
        // borrow of `shape` are disjoint -- matching on `self` while also
        // handing `&mut self.center` to `interact_box` wouldn't compile.
        let PlacedComponent {
            center,
            rotation,
            properties,
            shape,
            ..
        } = self;

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
                Rect::from_center_size(*center, BOX_SIZE).center_bottom() + egui::vec2(0.0, 2.0),
                Align2::CENTER_TOP,
                label,
                11.0,
                symbol_color,
            );
        }

        match shape {
            Shape::Button { pressed } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                    .unwrap_or(Signal::Unknown);
                let color = if signal == Signal::High {
                    let [r, g, b] = properties.color.unwrap_or(DEFAULT_LED_COLOR);
                    Color32::from_rgb(r, g, b)
                } else {
                    off_color
                };
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                    *rotation,
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
            Shape::HandSet { level, .. } => {
                // Every port shows what its net resolves to, driving or not:
                // on an output that's the whole point, and on the other two
                // it's what tells you a value you set is being fought over.
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or(Signal::Unknown);
                let readout = signal_letter(signal);
                // The readout follows the signal, the body doesn't: which way
                // the value crosses the boundary is structure and shouldn't
                // change colour as the circuit runs.
                let mut readout_color = canvas::signal_color(signal, dark_mode);
                if circuit
                    .pins(id)
                    .first()
                    .is_some_and(|pin| circuit.is_weakly_driven(pin.net))
                {
                    readout_color = readout_color.gamma_multiply(canvas::WEAK_FADE);
                }

                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
                    symbol_color,
                    SymbolState {
                        label: readout,
                        label_color: Some(readout_color),
                        level: level.as_ref().map(|level| level.get()),
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
                if let Some(level) = level {
                    if response.clicked() {
                        level.set(level.get().next(properties.cycles_undriven(&kind)));
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
            Shape::Probe => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or(Signal::Unknown);
                // A probe reads out the net it's attached to, so it uses the
                // very colour code that net is drawn in — its own duplicate
                // of the five states was the one place they could disagree.
                let mut color = canvas::signal_color(signal, dark_mode);
                if circuit
                    .pins(id)
                    .first()
                    .is_some_and(|pin| circuit.is_weakly_driven(pin.net))
                {
                    color = color.gamma_multiply(canvas::WEAK_FADE);
                }
                let label = signal_letter(signal);
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
                    color,
                    SymbolState {
                        label,
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
                    .unwrap_or(Signal::Unknown);
                // A clock is a signal source, so its symbol follows the same
                // colour code as the wire it drives (`canvas::signal_color`)
                // rather than a lit/unlit one of its own — green while high,
                // not red.
                let color = canvas::signal_color(signal, dark_mode);
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(
                    painter,
                    &kind,
                    rect,
                    *rotation,
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
fn signal_letter(signal: Signal) -> &'static str {
    // A net never resolves to a weak level, so those arms are unreachable;
    // reading them as their full-strength selves is the only answer that
    // could ever be right.
    match signal.strengthened() {
        Signal::High => "1",
        Signal::Low => "0",
        Signal::Error => "E",
        Signal::HighZ => "Z",
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

    fn port(kind: ComponentKind) -> InstancePort {
        InstancePort {
            name: String::new(),
            kind,
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
