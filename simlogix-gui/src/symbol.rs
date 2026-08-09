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
/// How far a `Button`'s cap sinks towards its pin while held.
const CAP_TRAVEL: f32 = 2.5;

/// Where a drawn component's pins ended up — a wire attaches at these exact
/// points — in the same order `Circuit::pins` reports them.
pub struct PinPositions {
    pub inputs: Vec<Pos2>,
    pub outputs: Vec<Pos2>,
}

/// Whatever a symbol needs to draw itself beyond its geometry — the bits of
/// live state that differ frame to frame. Most kinds use none of it and pass
/// [`SymbolState::default`].
///
/// A struct rather than a growing list of parameters: `label` was already
/// here for `Probe` alone, and `pressed` would have been the second such
/// escape hatch. This way the next one costs a field rather than a signature
/// change at every call site.
#[derive(Debug, Default, Clone, Copy)]
pub struct SymbolState<'a> {
    /// `Probe`: the net state it's reading, e.g. `"1"`/`"0"`/`"?"`.
    pub label: &'a str,
    /// `Button`: whether its cap is currently down.
    pub pressed: bool,
}

/// Draws `kind`'s icon within `rect`, oriented by `rotation`, in `color`, and
/// returns where its pins ended up.
pub fn draw(
    painter: &Painter,
    kind: ComponentKind,
    rect: Rect,
    rotation: Rotation,
    color: Color32,
    state: SymbolState<'_>,
) -> PinPositions {
    let stroke = Stroke::new(1.6, color);
    let label = state.label;
    match kind {
        ComponentKind::Button => draw_button(painter, rect, rotation, stroke, color, state.pressed),
        ComponentKind::Led => draw_led(painter, rect, rotation, stroke, color),
        ComponentKind::NTransistor => draw_transistor(painter, rect, rotation, stroke, true),
        ComponentKind::PTransistor => draw_transistor(painter, rect, rotation, stroke, false),
        ComponentKind::Ground => draw_ground(painter, rect, rotation, stroke),
        ComponentKind::Power => draw_power(painter, rect, rotation, stroke, color),
        ComponentKind::Probe => draw_probe(painter, rect, rotation, stroke, color, label),
        ComponentKind::Clock => draw_clock(painter, rect, rotation, stroke),
        ComponentKind::And => draw_and_gate(painter, rect, rotation, stroke, false),
        ComponentKind::Nand => draw_and_gate(painter, rect, rotation, stroke, true),
        ComponentKind::Or => draw_or_gate(painter, rect, rotation, stroke, false, false),
        ComponentKind::Nor => draw_or_gate(painter, rect, rotation, stroke, false, true),
        ComponentKind::Xor => draw_or_gate(painter, rect, rotation, stroke, true, false),
        ComponentKind::Xnor => draw_or_gate(painter, rect, rotation, stroke, true, true),
        ComponentKind::Buffer => draw_triangle_gate(painter, rect, rotation, stroke, false),
        ComponentKind::Not => draw_triangle_gate(painter, rect, rotation, stroke, true),
        ComponentKind::SrLatch => draw_sr_latch(painter, rect, rotation, stroke),
        ComponentKind::TriStateBuffer => draw_tri_state_buffer(painter, rect, rotation, stroke),
        ComponentKind::BusTransceiver => {
            draw_bus_transceiver(painter, rect, rotation, stroke, false)
        }
        ComponentKind::BusTransceiverOe => {
            draw_bus_transceiver(painter, rect, rotation, stroke, true)
        }
    }
}

/// A bus transceiver: a body with a double-headed arrow across it, the two
/// bus sides on the left and right, and the control pins stacked on the left.
///
/// Labelled, by the same rule as the SR latch: `DIR` and `OE` do entirely
/// different things and nothing about their position says which is which.
/// The arrow carries the rest — `A` is the left side, `B` the right, and
/// `DIR` high sends A to B.
///
/// `active_low` draws the enable as `OE` with an inversion bubble; without
/// it, as a plain `EN`. That bubble is the whole difference on screen, and
/// it has to be there: the two variants are otherwise identical, and so is
/// the tri-state buffer's own (active-high, unbubbled) enable.
fn draw_bus_transceiver(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    active_low: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let inset = rect.width() * 0.2;
    let body = Rect::from_min_max(
        pos2(rect.left() + inset, rect.top()),
        pos2(rect.right() - inset, rect.bottom()),
    );

    let dir = pos2(rect.left(), rect.top());
    let a = pos2(rect.left(), c.y);
    let enable = pos2(rect.left(), rect.bottom());
    let b = pos2(rect.right(), c.y);

    painter.line_segment([r(dir), r(pos2(body.left(), rect.top()))], stroke);
    painter.line_segment([r(a), r(pos2(body.left(), c.y))], stroke);
    if active_low {
        // The bubble sits against the body and the lead stops short of it —
        // `bubble_end` draws it and hands back where the lead resumes.
        let bubble_at = pos2(body.left() - BUBBLE_RADIUS * 2.0, rect.bottom());
        painter.line_segment([r(enable), r(bubble_at)], stroke);
        bubble_end(painter, bubble_at, r, stroke);
    } else {
        painter.line_segment([r(enable), r(pos2(body.left(), rect.bottom()))], stroke);
    }
    painter.line_segment([r(b), r(pos2(body.right(), c.y))], stroke);

    let corners = [
        pos2(body.left(), body.top()),
        pos2(body.right(), body.top()),
        pos2(body.right(), body.bottom()),
        pos2(body.left(), body.bottom()),
        pos2(body.left(), body.top()),
    ];
    painter.line(corners.into_iter().map(r).collect(), stroke);

    // A double-headed arrow across the middle: the one thing that has to
    // read at a glance is that this passes both ways.
    let (tail, head) = (pos2(body.left() + 8.0, c.y), pos2(body.right() - 8.0, c.y));
    painter.line_segment([r(tail), r(head)], stroke);
    for (point, sign) in [(tail, 1.0), (head, -1.0)] {
        painter.line_segment(
            [r(point), r(pos2(point.x + 4.0 * sign, point.y - 3.0))],
            stroke,
        );
        painter.line_segment(
            [r(point), r(pos2(point.x + 4.0 * sign, point.y + 3.0))],
            stroke,
        );
    }

    // Upright at rotated positions, like every other label here.
    let font = FontId::proportional(9.0);
    painter.text(
        r(pos2(body.left() + 3.0, rect.top() + 7.0)),
        Align2::LEFT_CENTER,
        "DIR",
        font.clone(),
        color,
    );
    painter.text(
        r(pos2(body.left() + 3.0, rect.bottom() - 7.0)),
        Align2::LEFT_CENTER,
        if active_low { "OE" } else { "EN" },
        font,
        color,
    );

    for pin in [dir, a, enable, b] {
        draw_pin(painter, r(pin), color);
    }

    PinPositions {
        // Control pins first, then the two bus sides — the draw arm in
        // `placed_component.rs` maps these back onto the pin order `place()`
        // registers (A, B, Dir, Enable).
        inputs: vec![r(dir), r(enable)],
        outputs: vec![r(a), r(b)],
    }
}

/// A tri-state buffer: the buffer triangle, with the enable coming down
/// onto the middle of its upper edge — the conventional symbol.
///
/// Its own geometry rather than [`draw_triangle_gate`]'s, because the
/// enable dictates it: the triangle is sized so the midpoint of its upper
/// edge sits directly below the box's top-centre, which is the only grid
/// point on that edge. That keeps the enable lead vertical and all three
/// pins on the grid.
fn draw_tri_state_buffer(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let back_x = rect.left() + rect.width() * 0.1;
    let tip = pos2(rect.right() - rect.width() * 0.1, c.y);
    let half_h = rect.height() * 0.45;

    let pin_in = pos2(rect.left(), c.y);
    let pin_out = pos2(rect.right(), c.y);
    let pin_enable = pos2(c.x, rect.top());

    painter.line_segment([r(pin_in), r(pos2(back_x, c.y))], stroke);
    painter.line_segment([r(tip), r(pin_out)], stroke);

    let triangle = vec![
        pos2(back_x, c.y - half_h),
        pos2(back_x, c.y + half_h),
        tip,
        pos2(back_x, c.y - half_h),
    ];
    painter.line(triangle.into_iter().map(r).collect(), stroke);

    // Halfway along the upper edge, which the geometry above puts exactly
    // at the box's horizontal centre.
    let on_edge = pos2(c.x, c.y - half_h / 2.0);
    painter.line_segment([r(pin_enable), r(on_edge)], stroke);

    for pin in [pin_in, pin_enable, pin_out] {
        draw_pin(painter, r(pin), color);
    }

    PinPositions {
        // Data first, enable second — the order `place()` registers the
        // pins in, and therefore the order a saved wire refers to them by.
        inputs: vec![r(pin_in), r(pin_enable)],
        outputs: vec![r(pin_out)],
    }
}

/// An SR latch: a plain body with its four pins labelled.
///
/// **The one symbol that carries text**, against the standing convention.
/// A gate can go unlabelled because its shape says which side is which and
/// its two inputs are interchangeable — swap an AND's inputs and nothing
/// changes. A latch's are not: `S` and `R` do opposite things, and so do
/// `Q` and `Q̄`. Four unlabelled pins at four corners would be a guess, and
/// the box-with-labels *is* this component's recognisable form.
fn draw_sr_latch(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    // Inset horizontally only: the pins have to land on the rect's corners
    // to stay on the grid, so the body's height is the rect's.
    let inset = rect.width() * 0.2;
    let body = Rect::from_min_max(
        pos2(rect.left() + inset, rect.top()),
        pos2(rect.right() - inset, rect.bottom()),
    );

    let set = pos2(rect.left(), rect.top());
    let reset = pos2(rect.left(), rect.bottom());
    let q = pos2(rect.right(), rect.top());
    let q_bar = pos2(rect.right(), rect.bottom());

    painter.line_segment([r(set), r(pos2(body.left(), body.top()))], stroke);
    painter.line_segment([r(reset), r(pos2(body.left(), body.bottom()))], stroke);
    painter.line_segment([r(q), r(pos2(body.right(), body.top()))], stroke);
    painter.line_segment([r(q_bar), r(pos2(body.right(), body.bottom()))], stroke);

    let corners = [
        pos2(body.left(), body.top()),
        pos2(body.right(), body.top()),
        pos2(body.right(), body.bottom()),
        pos2(body.left(), body.bottom()),
        pos2(body.left(), body.top()),
    ];
    painter.line(corners.into_iter().map(r).collect(), stroke);

    // Drawn upright at rotated positions, the same way a component's own
    // name label is: turning the glyphs would only make them unreadable.
    let font = FontId::proportional(9.0);
    let pad = 4.0;
    let label = |at: Pos2, align: Align2, text: &str| {
        painter.text(r(at), align, text, font.clone(), color);
    };
    label(
        pos2(body.left() + pad, body.top() + pad + 3.0),
        Align2::LEFT_CENTER,
        "S",
    );
    label(
        pos2(body.left() + pad, body.bottom() - pad - 3.0),
        Align2::LEFT_CENTER,
        "R",
    );
    label(
        pos2(body.right() - pad, body.top() + pad + 3.0),
        Align2::RIGHT_CENTER,
        "Q",
    );
    label(
        pos2(body.right() - pad, body.bottom() - pad - 3.0),
        Align2::RIGHT_CENTER,
        "Q\u{0305}",
    );

    for pin in [set, reset, q, q_bar] {
        draw_pin(painter, r(pin), color);
    }

    PinPositions {
        inputs: vec![r(set), r(reset)],
        outputs: vec![r(q), r(q_bar)],
    }
}

/// The wire tool's palette icon: a routed run with its corner points, the
/// same shape the tool actually draws. Not tied to a `ComponentKind` —
/// a wire isn't a component.
pub fn draw_wire_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let w = rect.width() * 0.36;
    let h = rect.height() * 0.22;
    let corner = pos2(c.x, c.y - h);
    let points = [
        pos2(c.x - w, c.y - h),
        corner,
        pos2(c.x, c.y + h),
        pos2(c.x + w, c.y + h),
    ];
    painter.line(points.to_vec(), stroke);
    for point in [points[0], corner, points[3]] {
        painter.circle_filled(point, 2.0, color);
    }
}

/// The select tool's palette icon: the familiar pointer arrow.
pub fn draw_select_tool(painter: &Painter, rect: Rect, color: Color32) {
    let tip = pos2(
        rect.left() + rect.width() * 0.3,
        rect.top() + rect.height() * 0.1,
    );
    let w = rect.width() * 0.34;
    let h = rect.height() * 0.62;
    // Head, then the short tail that makes it read as a cursor rather than
    // just a triangle.
    let head = vec![
        tip,
        pos2(tip.x, tip.y + h),
        pos2(tip.x + w * 0.55, tip.y + h * 0.72),
        pos2(tip.x + w, tip.y + h * 0.5),
    ];
    painter.add(Shape::convex_polygon(head, color, Stroke::NONE));
    painter.line_segment(
        [
            pos2(tip.x + w * 0.42, tip.y + h * 0.72),
            pos2(tip.x + w * 0.72, tip.y + h * 1.25),
        ],
        Stroke::new(2.4, color),
    );
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

/// How many segments a sampled curve (Bézier or arc) is broken into — plenty
/// for a smooth look at this icon size without costing anything real.
const CURVE_STEPS: usize = 12;

/// Samples `CURVE_STEPS + 1` points along the quadratic Bézier curve from
/// `p0` through control point `ctrl` to `p1` — used by every curved gate
/// outline (Or/Nor/Xor/Xnor) instead of a straight-line approximation, which
/// reads as jagged rather than rounded at this size.
fn quadratic_bezier(p0: Pos2, ctrl: Pos2, p1: Pos2) -> Vec<Pos2> {
    (0..=CURVE_STEPS)
        .map(|i| {
            let t = i as f32 / CURVE_STEPS as f32;
            let mt = 1.0 - t;
            pos2(
                mt * mt * p0.x + 2.0 * mt * t * ctrl.x + t * t * p1.x,
                mt * mt * p0.y + 2.0 * mt * t * ctrl.y + t * t * p1.y,
            )
        })
        .collect()
}

/// Samples `CURVE_STEPS + 1` points along the circular arc of `radius`
/// around `center`, sweeping from `start_angle` to `end_angle` (radians).
fn arc_around(center: Pos2, radius: f32, start_angle: f32, end_angle: f32) -> Vec<Pos2> {
    (0..=CURVE_STEPS)
        .map(|i| {
            let t = i as f32 / CURVE_STEPS as f32;
            let angle = start_angle + t * (end_angle - start_angle);
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

/// Inversion bubble radius, used by every inverted gate (NAND/NOR/XNOR/NOT).
const BUBBLE_RADIUS: f32 = 3.5;

/// Draws a small inversion "bubble" just past `tip` along +x — canonical,
/// pre-rotation space, same as every other point in a `draw_xxx` function;
/// `rotate_fn` is that function's own `r` closure, used here only to place
/// the bubble draw call in screen space. Returns where the lead past the
/// bubble continues from, still in canonical space, for the caller to route
/// the rest of its (still-unrotated) lead through `r` as usual.
fn bubble_end(
    painter: &Painter,
    tip: Pos2,
    rotate_fn: impl Fn(Pos2) -> Pos2,
    stroke: Stroke,
) -> Pos2 {
    let bubble_center = pos2(tip.x + BUBBLE_RADIUS, tip.y);
    painter.circle_stroke(rotate_fn(bubble_center), BUBBLE_RADIUS, stroke);
    pos2(tip.x + BUBBLE_RADIUS * 2.0, tip.y)
}

/// A pushbutton: a round cap with a single lead reaching its output pin.
/// The cap fills in and sinks towards its pin while held.
///
/// Two cues rather than one, because neither carries on its own: the travel
/// is a couple of pixels, invisible at a glance, and a filled circle alone
/// reads like a lit LED. Together they say "pushed in". The fill takes the
/// symbol's own colour rather than a signal colour — the cap being down is a
/// position, not a level.
fn draw_button(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    color: Color32,
    pressed: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);

    let pin = pos2(rect.right(), c.y);
    let cap_radius = rect.height() * 0.22;
    let travel = if pressed { CAP_TRAVEL } else { 0.0 };
    let cap_center = pos2(c.x - cap_radius + travel, c.y);

    // The lead starts at the cap's edge, so it follows the travel and no gap
    // opens up behind it.
    painter.line_segment([r(pos2(cap_center.x + cap_radius, c.y)), r(pin)], stroke);
    if pressed {
        painter.circle_filled(r(cap_center), cap_radius, color);
    } else {
        painter.circle_stroke(r(cap_center), cap_radius, stroke);
    }
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

/// The classic AND-gate "D" shape: a flat back (where the two inputs enter)
/// and a semicircular front (where the output leaves). Both inputs sit at the
/// box's own top-left/bottom-left corners — same trick used for the
/// transistor's source/drain: two grid-aligned pins on one edge without an
/// arbitrary fractional offset. `invert` (NAND) adds the small inversion
/// bubble at the tip.
fn draw_and_gate(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let half_h = rect.height() / 2.0;
    let radius = half_h;
    let back_x = rect.left() + rect.width() * 0.1;
    let arc_center_x = back_x + rect.width() * 0.35;
    let tip = pos2(arc_center_x + radius, c.y);

    let pin_a = pos2(rect.left(), rect.top());
    let pin_b = pos2(rect.left(), rect.bottom());
    let pin_out = pos2(rect.right(), c.y);

    painter.line_segment([r(pin_a), r(pos2(back_x, c.y - half_h))], stroke);
    painter.line_segment([r(pin_b), r(pos2(back_x, c.y + half_h))], stroke);

    let lead_start = if invert {
        bubble_end(painter, tip, r, stroke)
    } else {
        tip
    };
    painter.line_segment([r(lead_start), r(pin_out)], stroke);

    let mut outline = vec![pos2(back_x, c.y - half_h)];
    outline.extend(arc_around(
        pos2(arc_center_x, c.y),
        radius,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    ));
    outline.push(pos2(back_x, c.y + half_h));
    outline.push(pos2(back_x, c.y - half_h));
    painter.line(outline.into_iter().map(r).collect(), stroke);

    draw_pin(painter, r(pin_a), color);
    draw_pin(painter, r(pin_b), color);
    draw_pin(painter, r(pin_out), color);

    PinPositions {
        inputs: vec![r(pin_a), r(pin_b)],
        outputs: vec![r(pin_out)],
    }
}

/// The classic OR-gate "shield" shape: a concave back (bulging toward the
/// output) tapering to a point at the front. `xor` adds the extra curved
/// line just behind the back edge that marks XOR/XNOR (the input leads cross
/// it, same as a real XOR symbol); `invert` (NOR/XNOR) adds the small
/// inversion bubble at the tip. Inputs sit at the box's own
/// top-left/bottom-left corners, same trick as `draw_and_gate`.
fn draw_or_gate(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    xor: bool,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let half_h = rect.height() / 2.0;
    let back_x = rect.left() + rect.width() * 0.12;
    let bow = rect.width() * 0.14;
    let span = rect.width() * 0.45;
    let tip = pos2(back_x + span, c.y);

    let pin_a = pos2(rect.left(), rect.top());
    let pin_b = pos2(rect.left(), rect.bottom());
    let pin_out = pos2(rect.right(), c.y);
    let back_top = pos2(back_x, c.y - half_h);
    let back_bottom = pos2(back_x, c.y + half_h);

    painter.line_segment([r(pin_a), r(back_top)], stroke);
    painter.line_segment([r(pin_b), r(back_bottom)], stroke);

    let lead_start = if invert {
        bubble_end(painter, tip, r, stroke)
    } else {
        tip
    };
    painter.line_segment([r(lead_start), r(pin_out)], stroke);

    // Top/bottom tapers: a real curve (not a straight line) from each back
    // corner out to a genuine point at the tip — an OR gate's tip is a
    // point, not a rounded cap, so unlike `draw_and_gate`'s semicircle this
    // deliberately doesn't try to meet tangent-smooth there.
    let top_ctrl = pos2(
        back_top.x + (tip.x - back_top.x) * 0.55,
        back_top.y + (tip.y - back_top.y) * 0.1,
    );
    let bottom_ctrl = pos2(
        back_bottom.x + (tip.x - back_bottom.x) * 0.55,
        back_bottom.y + (tip.y - back_bottom.y) * 0.1,
    );
    let mut outline = quadratic_bezier(back_top, top_ctrl, tip);
    outline.extend(quadratic_bezier(tip, bottom_ctrl, back_bottom));
    outline.extend(quadratic_bezier(
        back_bottom,
        pos2(back_x + bow, c.y),
        back_top,
    ));
    painter.line(outline.into_iter().map(r).collect(), stroke);

    if xor {
        let extra_x = back_x - rect.width() * 0.1;
        let extra = quadratic_bezier(
            pos2(extra_x, c.y - half_h),
            pos2(extra_x + bow, c.y),
            pos2(extra_x, c.y + half_h),
        );
        painter.line(extra.into_iter().map(r).collect(), stroke);
    }

    draw_pin(painter, r(pin_a), color);
    draw_pin(painter, r(pin_b), color);
    draw_pin(painter, r(pin_out), color);

    PinPositions {
        inputs: vec![r(pin_a), r(pin_b)],
        outputs: vec![r(pin_out)],
    }
}

/// A triangle gate: `Buffer`'s plain pass-through, or `Not`'s inverter with a
/// bubble at the tip. Single input at the box's own left-center (matching
/// `Led`/`Probe`'s single-pin convention), single output at the tip (or past
/// the bubble, for `Not`).
fn draw_triangle_gate(
    painter: &Painter,
    rect: Rect,
    rotation: Rotation,
    stroke: Stroke,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| rotate(p, c, rotation);
    let color = stroke.color;

    let half_h = rect.height() * 0.4;
    let back_x = rect.left() + rect.width() * 0.12;
    let tip = pos2(back_x + rect.width() * 0.5, c.y);

    let pin_in = pos2(rect.left(), c.y);
    let pin_out = pos2(rect.right(), c.y);

    painter.line_segment([r(pin_in), r(pos2(back_x, c.y))], stroke);

    let lead_start = if invert {
        bubble_end(painter, tip, r, stroke)
    } else {
        tip
    };
    painter.line_segment([r(lead_start), r(pin_out)], stroke);

    let triangle = vec![
        pos2(back_x, c.y - half_h),
        pos2(back_x, c.y + half_h),
        tip,
        pos2(back_x, c.y - half_h),
    ];
    painter.line(triangle.into_iter().map(r).collect(), stroke);

    draw_pin(painter, r(pin_in), color);
    draw_pin(painter, r(pin_out), color);

    PinPositions {
        inputs: vec![r(pin_in)],
        outputs: vec![r(pin_out)],
    }
}
