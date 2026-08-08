//! A component instance placed on the canvas: where it is, plus whatever each
//! kind needs beyond its `Circuit` registration.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Color32, Id, Painter, Pos2, Rect, Sense, Ui};
use simlogix_core::{Circuit, ComponentId, NetId, Signal};

use crate::canvas::{self, Rotation, BOX_SIZE};
use crate::palette::ComponentKind;
use crate::symbol;

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
    pub pins: Vec<PinHandle>,
}

/// The symbol's stroke/accent color for components with no on/off state of
/// their own (button, transistor, rail).
const SYMBOL_COLOR: Color32 = Color32::from_gray(220);
/// Symbol color for a High signal.
const ON_COLOR: Color32 = Color32::from_rgb(220, 30, 30);
/// Symbol color for a Low signal — bright enough to stay visible against the
/// dark canvas now that there's no box behind it to contrast against.
const OFF_COLOR: Color32 = Color32::from_gray(180);
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
pub enum PlacedComponent {
    Button {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
        pressed: Rc<Cell<bool>>,
    },
    Led {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
    },
    Transistor {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
        kind: ComponentKind,
    },
    Rail {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
        kind: ComponentKind,
    },
    Probe {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
    },
    Clock {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
    },
    /// Any 2-input, 1-output, stateless combinational gate: two inputs at
    /// pin indices 0/1, one output at pin index 0. Adding a new gate of this
    /// shape needs no new variant here: add the `ComponentKind`, a core
    /// `Component` impl, a `draw_xxx` in `symbol.rs`, and a `place()` arm in
    /// `app.rs` that calls [`PlacedComponent::two_input_gate`] — this type's
    /// `draw_and_interact` arm already handles the rest generically.
    TwoInputGate {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
        kind: ComponentKind,
    },
    /// The 1-input mirror of `TwoInputGate`: a single input at pin index 0,
    /// a single output at pin index 1 (`Not`/`Buffer`). Same extension
    /// recipe, via [`PlacedComponent::one_input_gate`].
    OneInputGate {
        id: ComponentId,
        center: Pos2,
        rotation: Rotation,
        kind: ComponentKind,
    },
}

impl PlacedComponent {
    pub fn button(id: ComponentId, center: Pos2, pressed: Rc<Cell<bool>>) -> Self {
        Self::Button {
            id,
            center,
            rotation: Rotation::default(),
            pressed,
        }
    }

    pub fn led(id: ComponentId, center: Pos2) -> Self {
        Self::Led {
            id,
            center,
            rotation: Rotation::default(),
        }
    }

    pub fn transistor(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::Transistor {
            id,
            center,
            rotation: Rotation::default(),
            kind,
        }
    }

    pub fn rail(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::Rail {
            id,
            center,
            rotation: Rotation::default(),
            kind,
        }
    }

    pub fn probe(id: ComponentId, center: Pos2) -> Self {
        Self::Probe {
            id,
            center,
            rotation: Rotation::default(),
        }
    }

    pub fn clock(id: ComponentId, center: Pos2) -> Self {
        Self::Clock {
            id,
            center,
            rotation: Rotation::default(),
        }
    }

    pub fn two_input_gate(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::TwoInputGate {
            id,
            center,
            rotation: Rotation::default(),
            kind,
        }
    }

    pub fn one_input_gate(id: ComponentId, center: Pos2, kind: ComponentKind) -> Self {
        Self::OneInputGate {
            id,
            center,
            rotation: Rotation::default(),
            kind,
        }
    }

    pub fn id(&self) -> ComponentId {
        match self {
            PlacedComponent::Button { id, .. }
            | PlacedComponent::Led { id, .. }
            | PlacedComponent::Transistor { id, .. }
            | PlacedComponent::Rail { id, .. }
            | PlacedComponent::Probe { id, .. }
            | PlacedComponent::Clock { id, .. }
            | PlacedComponent::TwoInputGate { id, .. }
            | PlacedComponent::OneInputGate { id, .. } => *id,
        }
    }

    pub fn center(&self) -> Pos2 {
        match self {
            PlacedComponent::Button { center, .. }
            | PlacedComponent::Led { center, .. }
            | PlacedComponent::Transistor { center, .. }
            | PlacedComponent::Rail { center, .. }
            | PlacedComponent::Probe { center, .. }
            | PlacedComponent::Clock { center, .. }
            | PlacedComponent::TwoInputGate { center, .. }
            | PlacedComponent::OneInputGate { center, .. } => *center,
        }
    }

    pub fn rotation(&self) -> Rotation {
        match self {
            PlacedComponent::Button { rotation, .. }
            | PlacedComponent::Led { rotation, .. }
            | PlacedComponent::Transistor { rotation, .. }
            | PlacedComponent::Rail { rotation, .. }
            | PlacedComponent::Probe { rotation, .. }
            | PlacedComponent::Clock { rotation, .. }
            | PlacedComponent::TwoInputGate { rotation, .. }
            | PlacedComponent::OneInputGate { rotation, .. } => *rotation,
        }
    }

    /// Which palette entry would place an identical component — what a
    /// project file needs to reconstruct this on load.
    pub fn kind(&self) -> ComponentKind {
        match self {
            PlacedComponent::Button { .. } => ComponentKind::Button,
            PlacedComponent::Led { .. } => ComponentKind::Led,
            PlacedComponent::Transistor { kind, .. }
            | PlacedComponent::Rail { kind, .. }
            | PlacedComponent::TwoInputGate { kind, .. }
            | PlacedComponent::OneInputGate { kind, .. } => *kind,
            PlacedComponent::Probe { .. } => ComponentKind::Probe,
            PlacedComponent::Clock { .. } => ComponentKind::Clock,
        }
    }

    /// Sets this component's rotation directly (used when loading a project).
    pub fn set_rotation(&mut self, new_rotation: Rotation) {
        let rotation = match self {
            PlacedComponent::Button { rotation, .. }
            | PlacedComponent::Led { rotation, .. }
            | PlacedComponent::Transistor { rotation, .. }
            | PlacedComponent::Rail { rotation, .. }
            | PlacedComponent::Probe { rotation, .. }
            | PlacedComponent::Clock { rotation, .. }
            | PlacedComponent::TwoInputGate { rotation, .. }
            | PlacedComponent::OneInputGate { rotation, .. } => rotation,
        };
        *rotation = new_rotation;
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
        selected: Option<ComponentId>,
    ) -> FrameResult {
        let id = self.id();
        let kind = self.kind();
        let is_selected = selected == Some(id);
        let rect_id = Id::new(("placed", id));

        match self {
            PlacedComponent::Button {
                center,
                rotation,
                pressed,
                ..
            } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, SYMBOL_COLOR, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                } else {
                    let is_pressed = response.is_pointer_button_down_on();
                    if is_pressed != pressed.get() {
                        pressed.set(is_pressed);
                        circuit.schedule_now(id);
                        let _ = circuit.advance(crate::app::SETTLE_TICKS);
                    }
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, id, 0, pin_positions.outputs[0], net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![pin],
                }
            }
            PlacedComponent::Led {
                center, rotation, ..
            } => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or(Signal::Unknown);
                let color = if signal == Signal::High {
                    ON_COLOR
                } else {
                    OFF_COLOR
                };
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, color, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, id, 0, pin_positions.inputs[0], net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![pin],
                }
            }
            PlacedComponent::Transistor {
                center, rotation, ..
            } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, SYMBOL_COLOR, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let circuit_pins = circuit.pins(id);
                let gate = pin_handle(ui, id, 0, pin_positions.inputs[0], circuit_pins[0].net);
                let source = pin_handle(ui, id, 1, pin_positions.inputs[1], circuit_pins[1].net);
                let drain = pin_handle(ui, id, 2, pin_positions.outputs[0], circuit_pins[2].net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![gate, source, drain],
                }
            }
            PlacedComponent::Rail {
                center, rotation, ..
            } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, SYMBOL_COLOR, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, id, 0, pin_positions.outputs[0], net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![pin],
                }
            }
            PlacedComponent::Probe {
                center, rotation, ..
            } => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or(Signal::Unknown);
                let color = match signal {
                    Signal::High => ON_COLOR,
                    Signal::Low => OFF_COLOR,
                    Signal::Unknown => Color32::from_gray(140),
                    Signal::Error => Color32::from_rgb(200, 60, 60),
                    Signal::HighZ => Color32::from_gray(160),
                };
                let label = match signal {
                    Signal::High => "1",
                    Signal::Low => "0",
                    Signal::Unknown => "?",
                    Signal::Error => "E",
                    Signal::HighZ => "Z",
                };
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, color, label);
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, id, 0, pin_positions.inputs[0], net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![pin],
                }
            }
            PlacedComponent::Clock {
                center, rotation, ..
            } => {
                let signal = circuit
                    .pins(id)
                    .first()
                    .map(|pin| circuit.signal_at(pin.net))
                    .unwrap_or(Signal::Unknown);
                let color = if signal == Signal::High {
                    ON_COLOR
                } else {
                    OFF_COLOR
                };
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, color, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let net = circuit.pins(id)[0].net;
                let pin = pin_handle(ui, id, 0, pin_positions.outputs[0], net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![pin],
                }
            }
            PlacedComponent::TwoInputGate {
                center, rotation, ..
            } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, SYMBOL_COLOR, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let circuit_pins = circuit.pins(id);
                let a = pin_handle(ui, id, 0, pin_positions.inputs[0], circuit_pins[0].net);
                let b = pin_handle(ui, id, 1, pin_positions.inputs[1], circuit_pins[1].net);
                let out = pin_handle(ui, id, 2, pin_positions.outputs[0], circuit_pins[2].net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![a, b, out],
                }
            }
            PlacedComponent::OneInputGate {
                center, rotation, ..
            } => {
                let rect = Rect::from_center_size(*center, BOX_SIZE);
                let pin_positions = symbol::draw(painter, kind, rect, *rotation, SYMBOL_COLOR, "");
                if is_selected {
                    canvas::draw_selection_outline(painter, rect);
                }

                let response = ui.interact(
                    rect.shrink(PIN_HIT_MARGIN),
                    rect_id,
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    *center += response.drag_delta();
                }
                if response.drag_stopped() {
                    *center = canvas::snap_to_grid(*center);
                }

                let circuit_pins = circuit.pins(id);
                let input = pin_handle(ui, id, 0, pin_positions.inputs[0], circuit_pins[0].net);
                let output = pin_handle(ui, id, 1, pin_positions.outputs[0], circuit_pins[1].net);

                FrameResult {
                    clicked: response.clicked().then_some(id),
                    pins: vec![input, output],
                }
            }
        }
    }
}

/// A small, separate interactive hit area right at a pin's tip, distinct from
/// the component box's own drag area (pins are drawn just outside the box).
fn pin_handle(
    ui: &mut Ui,
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
    PinHandle {
        component,
        pin_index,
        position,
        net,
        clicked: response.clicked(),
    }
}
