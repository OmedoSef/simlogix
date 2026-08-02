//! Generic canvas rendering: a grid background and the auto-generated
//! box-with-pins appearance used for every component (no custom symbol editor
//! yet — out of scope for v1).

use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

pub const GRID_SPACING: f32 = 20.0;
/// Default box size for the auto-generated component appearance.
pub const BOX_SIZE: Vec2 = egui::vec2(90.0, 50.0);
const PIN_STUB: f32 = 16.0;
const PIN_RADIUS: f32 = 3.0;

/// A component's orientation: which box edge its inputs/outputs are drawn on.
/// The box itself stays axis-aligned (so its label stays horizontal and
/// readable) — only which side is "input" vs "output" rotates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    /// Inputs left, outputs right.
    #[default]
    Deg0,
    /// Inputs top, outputs bottom.
    Deg90,
    /// Inputs right, outputs left.
    Deg180,
    /// Inputs bottom, outputs top.
    Deg270,
}

impl Rotation {
    /// The next quarter-turn clockwise.
    pub fn next_clockwise(self) -> Self {
        match self {
            Rotation::Deg0 => Rotation::Deg90,
            Rotation::Deg90 => Rotation::Deg180,
            Rotation::Deg180 => Rotation::Deg270,
            Rotation::Deg270 => Rotation::Deg0,
        }
    }

    fn input_output_edges(self) -> (Edge, Edge) {
        match self {
            Rotation::Deg0 => (Edge::Left, Edge::Right),
            Rotation::Deg90 => (Edge::Top, Edge::Bottom),
            Rotation::Deg180 => (Edge::Right, Edge::Left),
            Rotation::Deg270 => (Edge::Bottom, Edge::Top),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Snaps a canvas position to the nearest grid intersection.
pub fn snap_to_grid(pos: Pos2) -> Pos2 {
    pos2(
        (pos.x / GRID_SPACING).round() * GRID_SPACING,
        (pos.y / GRID_SPACING).round() * GRID_SPACING,
    )
}

/// The shortest distance from `point` to the segment `a`-`b` — used to hit-test
/// a click against a drawn wire.
pub fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq == 0.0 {
        return (point - a).length();
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    (point - projection).length()
}

/// Draws a dot grid filling `rect`.
pub fn draw_grid(painter: &Painter, rect: Rect) {
    let dot_color = Color32::from_gray(70);
    let mut y = rect.top();
    while y < rect.bottom() {
        let mut x = rect.left();
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.0, dot_color);
            x += GRID_SPACING;
        }
        y += GRID_SPACING;
    }
}

/// Where a drawn component's pins ended up (the tip of each pin's stub, where
/// a wire should attach), in the same order as the `inputs`/`outputs` names
/// passed to [`draw_component`].
pub struct PinPositions {
    pub inputs: Vec<Pos2>,
    pub outputs: Vec<Pos2>,
}

/// Draws a component as a labeled box with named pins — the auto-generated
/// appearance planned for v1. `rotation` picks which edge inputs/outputs land
/// on; the box and its label stay axis-aligned regardless.
pub fn draw_component(
    painter: &Painter,
    rect: Rect,
    label: &str,
    fill_color: Color32,
    rotation: Rotation,
    inputs: &[&str],
    outputs: &[&str],
) -> PinPositions {
    painter.rect_filled(rect, 4.0, fill_color);
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.5, Color32::from_gray(200)),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(14.0),
        Color32::WHITE,
    );

    let (input_edge, output_edge) = rotation.input_output_edges();

    PinPositions {
        inputs: draw_pins_on_edge(painter, rect, input_edge, inputs),
        outputs: draw_pins_on_edge(painter, rect, output_edge, outputs),
    }
}

/// Draws `names.len()` pins evenly spaced along `rect`'s given `edge`,
/// returning each pin's tip position (where a wire attaches) in order.
fn draw_pins_on_edge(painter: &Painter, rect: Rect, edge: Edge, names: &[&str]) -> Vec<Pos2> {
    let pin_stroke = Stroke::new(2.0, Color32::from_gray(200));
    let pin_color = Color32::from_gray(200);
    let label_color = Color32::from_gray(180);
    let font = FontId::proportional(11.0);

    let positions_along = match edge {
        Edge::Left | Edge::Right => evenly_spaced(rect.top(), rect.bottom(), names.len()),
        Edge::Top | Edge::Bottom => evenly_spaced(rect.left(), rect.right(), names.len()),
    };

    positions_along
        .into_iter()
        .zip(names)
        .map(|(coord, name)| {
            let (attach, tip, label_pos, anchor) = match edge {
                Edge::Left => (
                    pos2(rect.left(), coord),
                    pos2(rect.left() - PIN_STUB, coord),
                    pos2(rect.left() - PIN_STUB - 4.0, coord),
                    Align2::RIGHT_CENTER,
                ),
                Edge::Right => (
                    pos2(rect.right(), coord),
                    pos2(rect.right() + PIN_STUB, coord),
                    pos2(rect.right() + PIN_STUB + 4.0, coord),
                    Align2::LEFT_CENTER,
                ),
                Edge::Top => (
                    pos2(coord, rect.top()),
                    pos2(coord, rect.top() - PIN_STUB),
                    pos2(coord, rect.top() - PIN_STUB - 4.0),
                    Align2::CENTER_BOTTOM,
                ),
                Edge::Bottom => (
                    pos2(coord, rect.bottom()),
                    pos2(coord, rect.bottom() + PIN_STUB),
                    pos2(coord, rect.bottom() + PIN_STUB + 4.0),
                    Align2::CENTER_TOP,
                ),
            };
            painter.line_segment([attach, tip], pin_stroke);
            painter.circle_filled(tip, PIN_RADIUS, pin_color);
            painter.text(label_pos, anchor, *name, font.clone(), label_color);
            tip
        })
        .collect()
}

/// Draws a highlight outline around a selected component's box.
pub fn draw_selection_outline(painter: &Painter, rect: Rect) {
    painter.rect_stroke(
        rect.expand(3.0),
        6.0,
        Stroke::new(2.0, Color32::from_rgb(90, 160, 255)),
        StrokeKind::Outside,
    );
}

/// `count` values evenly spaced between `min` and `max` (never right on
/// either edge, so a single pin lands dead center).
fn evenly_spaced(min: f32, max: f32, count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let fraction = (i as f32 + 1.0) / (count as f32 + 1.0);
            min + fraction * (max - min)
        })
        .collect()
}
