//! Generic canvas rendering: the grid background, selection highlight, and
//! wire-path helpers. The auto-generated appearance itself (a component's
//! symbol and its exact pins) lives in `symbol.rs` — no separate generic
//! box/pin layer here.

use egui::{pos2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};

pub const GRID_SPACING: f32 = 20.0;
/// Default box size for the auto-generated component appearance — each half
/// extent is a whole multiple of `GRID_SPACING`, so a pin sitting at a rect
/// edge (as every symbol's does — see `symbol.rs`) lands exactly on a grid
/// dot, since `center` itself is always grid-snapped.
pub const BOX_SIZE: Vec2 = egui::vec2(GRID_SPACING * 4.0, GRID_SPACING * 2.0);

/// A component's orientation, applied to its whole symbol (shape and pins
/// together — see `symbol::rotate`) as a clockwise quarter-turn count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Rotation {
    #[default]
    Deg0,
    Deg90,
    Deg180,
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
}

/// Snaps a single coordinate to the nearest grid line.
pub fn snap_coord_to_grid(value: f32) -> f32 {
    (value / GRID_SPACING).round() * GRID_SPACING
}

/// Snaps a canvas position to the nearest grid intersection.
pub fn snap_to_grid(pos: Pos2) -> Pos2 {
    pos2(snap_coord_to_grid(pos.x), snap_coord_to_grid(pos.y))
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

/// Draws a polyline through every point of `path` in order.
pub fn draw_path(painter: &Painter, path: &[Pos2], stroke: Stroke) {
    for pair in path.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}

/// The shortest distance from `point` to any segment of `path` — used to
/// hit-test a click against a (possibly multi-segment) drawn wire.
pub fn distance_to_path(point: Pos2, path: &[Pos2]) -> f32 {
    path.windows(2)
        .map(|pair| distance_to_segment(point, pair[0], pair[1]))
        .fold(f32::INFINITY, f32::min)
}

/// The index `i` of the segment `path[i]`-`path[i+1]` closest to `point`,
/// and that distance — used to figure out where a new waypoint should be
/// inserted when double-clicking along an already-drawn wire (the new point
/// goes at that same index in the wire's waypoint list, since `path[0]` is
/// the wire's anchor and isn't itself a waypoint).
pub fn closest_segment(path: &[Pos2], point: Pos2) -> Option<(usize, f32)> {
    path.windows(2)
        .enumerate()
        .map(|(i, pair)| (i, distance_to_segment(point, pair[0], pair[1])))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Draws a dot grid filling `rect`.
pub fn draw_grid(painter: &Painter, rect: Rect) {
    let dot_color = Color32::from_gray(70);
    // Align dots to the same absolute grid `snap_to_grid` rounds to, not to
    // this panel's own top-left corner — otherwise the visible grid drifts
    // out of sync with where things actually snap whenever the canvas panel
    // doesn't start at a multiple of `GRID_SPACING` (it usually doesn't).
    let mut y = (rect.top() / GRID_SPACING).ceil() * GRID_SPACING;
    while y < rect.bottom() {
        let mut x = (rect.left() / GRID_SPACING).ceil() * GRID_SPACING;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.0, dot_color);
            x += GRID_SPACING;
        }
        y += GRID_SPACING;
    }
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
