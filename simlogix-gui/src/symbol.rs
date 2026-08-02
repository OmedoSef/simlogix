//! Small schematic-style icons per component kind — simplified, not
//! IEEE/IEC-precise, but recognizable. Used both for the palette (icon next
//! to the name, always drawn at `Rotation::Deg0`) and the canvas (icon alone,
//! no text — see `placed_component.rs`).
//!
//! Unlike a generic "box with pins on an edge" layout, each symbol here draws
//! its own leads and reports exactly where they end — the pin sits wherever
//! the symbol's own line stops, not at an independently-computed box-edge
//! offset. Every symbol is defined once in its canonical (`Deg0`) layout, then
//! [`rotate`] spins every point of it (geometry and pins alike) together, so
//! the icon and its pins always agree at every `Rotation`.

use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use crate::canvas::Rotation;
use crate::palette::ComponentKind;

const PIN_RADIUS: f32 = 3.0;

/// Where a drawn component's pins ended up — a wire attaches at these exact
/// points — in the same order `Circuit::pins` reports them.
pub struct PinPositions {
    pub inputs: Vec<Pos2>,
    pub outputs: Vec<Pos2>,
}

/// Draws `kind`'s icon within `rect`, oriented by `rotation`, in `color`, and
/// returns where its pins ended up. `label` is only used by `Probe` (the net
/// state it's reading, e.g. `"1"`/`"0"`/`"?"`) — pass `""` for every other
/// kind.
pub fn draw(
    painter: &Painter,
    kind: ComponentKind,
    rect: Rect,
    rotation: Rotation,
    color: Color32,
    label: &str,
) -> PinPositions {
    let stroke = Stroke::new(1.6, color);
    match kind {
        ComponentKind::Button => draw_button(painter, rect, rotation, stroke, color),
        ComponentKind::Led => draw_led(painter, rect, rotation, stroke, color),
        ComponentKind::NTransistor => draw_transistor(painter, rect, rotation, stroke, true),
        ComponentKind::PTransistor => draw_transistor(painter, rect, rotation, stroke, false),
        ComponentKind::Ground => draw_ground(painter, rect, rotation, stroke),
        ComponentKind::Power => draw_power(painter, rect, rotation, stroke, color),
        ComponentKind::Probe => draw_probe(painter, rect, rotation, stroke, color, label),
        ComponentKind::Clock => draw_clock(painter, rect, rotation, stroke),
    }
}

/// Rotates `point` clockwise around `center` by `rotation`'s quarter-turns —
/// the same clockwise convention the old edge-based layout used (a point on
/// the left ends up on top after one quarter-turn, and so on), so a symbol's
/// canonical `Deg0` geometry keeps its intended orientation under rotation.
fn rotate(point: Pos2, center: Pos2, rotation: Rotation) -> Pos2 {
    let quarters = match rotation {
        Rotation::Deg0 => 0,
        Rotation::Deg90 => 1,
        Rotation::Deg180 => 2,
        Rotation::Deg270 => 3,
    };
    let mut d = point - center;
    for _ in 0..quarters {
        d = egui::vec2(-d.y, d.x);
    }
    center + d
}

fn draw_pin(painter: &Painter, point: Pos2, color: Color32) {
    painter.circle_filled(point, PIN_RADIUS, color);
}

/// A pushbutton: a round cap with a single lead reaching its output pin.
fn draw_button(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    color: Color32,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);

    let pin = pos2(rect.right(), c.y);
    let cap_radius = rect.height() * 0.22;
    let cap_center = pos2(c.x - cap_radius, c.y);

    painter.line_segment([r(pos2(cap_center.x + cap_radius, c.y)), r(pin)], stroke);
    painter.circle_stroke(r(cap_center), cap_radius, stroke);
    draw_pin(painter, r(pin), color);

    PinPositions {
        inputs: vec![],
        outputs: vec![r(pin)],
    }
}

/// A bulb that lights up or goes dark, Logisim-style — `color` already
/// carries the on/off state (see `placed_component.rs`), so the circle itself
/// is the indicator. Its one input pin sits where the lead reaching it ends.
fn draw_led(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    color: Color32,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let radius = rect.height() * 0.34;
    let bulb = pos2(c.x + radius * 0.2, c.y);

    let pin = pos2(rect.left(), c.y);
    painter.line_segment([r(pin), r(pos2(bulb.x - radius, c.y))], stroke);
    painter.circle_filled(r(bulb), radius, color);
    painter.circle_stroke(r(bulb), radius, stroke);

    draw_pin(painter, r(pin), color);
    PinPositions {
        inputs: vec![r(pin)],
        outputs: vec![],
    }
}

/// A simplified MOSFET: gate on one side, source and drain both on the other
/// (a real MOSFET's two channel terminals sit on the same side, opposite the
/// gate) — an arrow at the channel shows carrier direction, into the channel
/// for N-type, out of it for P-type.
fn draw_transistor(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    is_n_type: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let gate_x = c.x - rect.width() * 0.12;
    let chan_x = c.x + rect.width() * 0.08;
    // Land exactly on the box's top/bottom-right corners — both grid-aligned,
    // unlike an arbitrary fraction of the box height.
    let drain_y = rect.top();
    let source_y = rect.bottom();

    let gate_pin = pos2(rect.left(), c.y);
    let drain_pin = pos2(rect.right(), drain_y);
    let source_pin = pos2(rect.right(), source_y);

    painter.line_segment([r(gate_pin), r(pos2(gate_x, c.y))], stroke);
    painter.line_segment(
        [r(pos2(gate_x, drain_y)), r(pos2(gate_x, source_y))],
        stroke,
    );

    painter.line_segment(
        [r(pos2(chan_x, drain_y)), r(pos2(chan_x, source_y))],
        stroke,
    );
    painter.line_segment([r(pos2(chan_x, drain_y)), r(drain_pin)], stroke);
    painter.line_segment([r(pos2(chan_x, source_y)), r(source_pin)], stroke);

    let (arrow_from, arrow_to) = if is_n_type {
        (pos2(chan_x + 8.0, c.y), pos2(chan_x, c.y))
    } else {
        (pos2(chan_x, c.y), pos2(chan_x + 8.0, c.y))
    };
    painter.line_segment([r(arrow_from), r(arrow_to)], stroke);
    let back = if is_n_type { 4.0 } else { -4.0 };
    painter.line_segment(
        [r(arrow_to), r(pos2(arrow_to.x + back, arrow_to.y - 3.0))],
        stroke,
    );
    painter.line_segment(
        [r(arrow_to), r(pos2(arrow_to.x + back, arrow_to.y + 3.0))],
        stroke,
    );

    draw_pin(painter, r(gate_pin), color);
    draw_pin(painter, r(drain_pin), color);
    draw_pin(painter, r(source_pin), color);

    PinPositions {
        inputs: vec![r(gate_pin), r(source_pin)],
        outputs: vec![r(drain_pin)],
    }
}

/// The classic ground symbol: a lead down into a stack of shrinking bars. Its
/// one output pin sits at the lead's far end.
fn draw_ground(painter: &Painter, rect: Rect, rotation: Rotation, stroke: Stroke) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let pin = pos2(c.x, rect.top());
    painter.line_segment([r(pin), r(c)], stroke);

    for (i, width_fraction) in [0.5_f32, 0.32, 0.16].into_iter().enumerate() {
        let y = c.y + i as f32 * 5.0;
        let half = rect.width() * width_fraction * 0.5;
        painter.line_segment([r(pos2(c.x - half, y)), r(pos2(c.x + half, y))], stroke);
    }

    draw_pin(painter, r(pin), color);
    PinPositions {
        inputs: vec![],
        outputs: vec![r(pin)],
    }
}

/// A power rail: a bold upward arrow (Logisim-style — a large, prominent
/// triangle, not a thin arrowhead) whose tail is the output pin.
fn draw_power(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    color: Color32,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);

    let pin = pos2(c.x, rect.bottom());
    let base_y = c.y + rect.height() * 0.12;
    let tip = pos2(c.x, rect.top());
    painter.line_segment([r(pin), r(pos2(c.x, base_y))], stroke);

    let half_w = rect.width() * 0.24;
    let arrowhead = vec![
        r(pos2(c.x - half_w, base_y)),
        r(pos2(c.x + half_w, base_y)),
        r(tip),
    ];
    painter.add(Shape::convex_polygon(arrowhead, color, Stroke::NONE));

    draw_pin(painter, r(pin), color);
    PinPositions {
        inputs: vec![],
        outputs: vec![r(pin)],
    }
}

/// A measurement probe: a circle around the net's state, spelled out as text
/// (`"1"`/`"0"`/`"?"`/`"E"`/`"Z"`) — unlike every other symbol, the probe's
/// whole point is to read out a state a human can name, so it's the one
/// deliberate exception to "no text on the canvas". Fed by a lead reaching
/// its one input pin. The text itself is drawn upright regardless of
/// `rotation`, so it stays readable even on a rotated probe.
fn draw_probe(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    color: Color32,
    label: &str,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let radius = rect.height() * 0.4;
    let bulb = pos2(c.x + radius * 0.2, c.y);

    let pin = pos2(rect.left(), c.y);
    painter.line_segment([r(pin), r(pos2(bulb.x - radius, c.y))], stroke);
    painter.circle_stroke(r(bulb), radius, stroke);
    painter.text(
        r(bulb),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        color,
    );

    draw_pin(painter, r(pin), color);
    PinPositions {
        inputs: vec![r(pin)],
        outputs: vec![],
    }
}

/// A clock/oscillator, Logisim-style: a small chip-like box framing the
/// square-wave icon, with a lead reaching its one output pin.
fn draw_clock(painter: &Painter, rect: Rect, rotation: Rotation, stroke: Stroke) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let box_half_w = rect.width() * 0.32;
    let box_half_h = rect.height() * 0.4;
    let corners = [
        pos2(c.x - box_half_w, c.y - box_half_h),
        pos2(c.x + box_half_w, c.y - box_half_h),
        pos2(c.x + box_half_w, c.y + box_half_h),
        pos2(c.x - box_half_w, c.y + box_half_h),
    ];
    for i in 0..corners.len() {
        painter.line_segment([r(corners[i]), r(corners[(i + 1) % corners.len()])], stroke);
    }

    let w = box_half_w * 0.55;
    let h = box_half_h * 0.5;
    let points = vec![
        pos2(c.x - w, c.y + h),
        pos2(c.x - w, c.y - h),
        pos2(c.x - w * 0.33, c.y - h),
        pos2(c.x - w * 0.33, c.y + h),
        pos2(c.x + w * 0.33, c.y + h),
        pos2(c.x + w * 0.33, c.y - h),
        pos2(c.x + w, c.y - h),
    ]
    .into_iter()
    .map(r)
    .collect();
    painter.line(points, stroke);

    let pin = pos2(rect.right(), c.y);
    painter.line_segment([r(pos2(c.x + box_half_w, c.y)), r(pin)], stroke);

    draw_pin(painter, r(pin), color);
    PinPositions {
        inputs: vec![],
        outputs: vec![r(pin)],
    }
}
