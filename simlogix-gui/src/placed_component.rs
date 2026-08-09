//! A component instance placed on the canvas: where it is, plus whatever each
//! kind needs beyond its `Circuit` registration.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Align2, Color32, FontId, Id, Painter, Pos2, Rect, Sense, Ui, Vec2};
use simlogix_core::{Circuit, ComponentId, NetId, PortLevel, Signal};

use crate::canvas::{self, Rotation, BOX_SIZE};
use crate::palette::ComponentKind;
use crate::properties::{Properties, DEFAULT_LED_COLOR};
use crate::symbol::{self, SymbolState};

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
    /// A circuit boundary port. `level` is present only on an input, the
    /// one port whose value is set by hand — clicking it *latches*, unlike
    /// a `Button`, because a port stands for what a parent will drive and
    /// that doesn't spring back.
    Port {
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

    pub fn port(
        id: ComponentId,
        center: Pos2,
        kind: ComponentKind,
        level: Option<Rc<Cell<PortLevel>>>,
    ) -> Self {
        Self::new(id, center, Shape::Port { kind, level })
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

    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    pub fn set_properties(&mut self, properties: Properties) {
        // A port's resting level is a property, so it has to reach the cell
        // the engine reads — on load and whenever it's edited. Same idea as
        // a button's `pressed`, except a latching port has no "held" state
        // to settle itself from, so it's pushed explicitly.
        if let Shape::Port {
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
            Shape::Port { kind, .. } => kind.clone(),
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

        // A name the user set is drawn under the symbol — once here rather
        // than once per arm, since every kind can carry one. This is the
        // deliberate exception to "symbols carry no text": an annotation you
        // wrote is not a label the editor generated for you.
        if let Some(label) = properties.label() {
            painter.text(
                Rect::from_center_size(*center, BOX_SIZE).center_bottom() + egui::vec2(0.0, 2.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(11.0),
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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);
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
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);
                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

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
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        1,
                        pin_positions.outputs[1],
                        circuit_pins[1].net,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        2,
                        pin_positions.inputs[0],
                        circuit_pins[2].net,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        3,
                        pin_positions.inputs[1],
                        circuit_pins[3].net,
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
            Shape::Port { level, .. } => {
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
                        ..Default::default()
                    },
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);
                // Latching: a click advances it and it stays there. Only on
                // a click, never on a drag, so moving a port across the
                // canvas can't also change what it carries.
                if let Some(level) = level {
                    if response.clicked() {
                        level.set(level.get().next(properties.is_tri_state()));
                        circuit.schedule_now(id);
                        input_changed = true;
                    }
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let circuit_pins = circuit.pins(id);
                let pins = vec![
                    pin_handle(
                        ui,
                        painter,
                        id,
                        0,
                        pin_positions.inputs[0],
                        circuit_pins[0].net,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        1,
                        pin_positions.inputs[1],
                        circuit_pins[1].net,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        2,
                        pin_positions.outputs[0],
                        circuit_pins[2].net,
                    ),
                    pin_handle(
                        ui,
                        painter,
                        id,
                        3,
                        pin_positions.outputs[1],
                        circuit_pins[3].net,
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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let circuit_pins = circuit.pins(id);
                let gate = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                );
                let source = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.inputs[1],
                    circuit_pins[1].net,
                );
                let drain = pin_handle(
                    ui,
                    painter,
                    id,
                    2,
                    pin_positions.outputs[0],
                    circuit_pins[2].net,
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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.inputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, painter, id, 0, pin_positions.outputs[0], net);

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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let circuit_pins = circuit.pins(id);
                let a = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                );
                let b = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.inputs[1],
                    circuit_pins[1].net,
                );
                let out = pin_handle(
                    ui,
                    painter,
                    id,
                    2,
                    pin_positions.outputs[0],
                    circuit_pins[2].net,
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
                );
                if is_selected {
                    canvas::draw_selection_outline(painter, rect, dark_mode);
                }

                let response = interact_box(ui, painter, rect, rect_id, center);

                let circuit_pins = circuit.pins(id);
                let input = pin_handle(
                    ui,
                    painter,
                    id,
                    0,
                    pin_positions.inputs[0],
                    circuit_pins[0].net,
                );
                let output = pin_handle(
                    ui,
                    painter,
                    id,
                    1,
                    pin_positions.outputs[0],
                    circuit_pins[1].net,
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

fn interact_box(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    rect_id: Id,
    center: &mut Pos2,
) -> egui::Response {
    let response = ui.interact(
        rect.shrink(PIN_HIT_MARGIN),
        rect_id,
        Sense::click_and_drag(),
    );

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
) -> PinHandle {
    let hit_rect = Rect::from_center_size(position, egui::vec2(14.0, 14.0));
    let response = ui.interact(
        hit_rect,
        Id::new(("pin", component, pin_index)),
        Sense::click(),
    );

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
