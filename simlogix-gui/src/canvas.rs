//! Generic canvas rendering: the grid background, selection highlight, and
//! wire-path helpers. The auto-generated appearance itself (a component's
//! symbol and its exact pins) lives in `symbol.rs` — no separate generic
//! box/pin layer here.

use egui::{pos2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use simlogix_core::{Level, Signal};

/// What colour a net at `signal` is drawn in — the single place the signal
/// colour code is defined, so wires and anything else reading out a net
/// can't drift apart.
///
/// Two sets, picked per theme rather than one compromise. A single palette
/// can't serve both backgrounds: measured against them, an amber that reads
/// at 8:1 on the dark canvas manages only 2:1 on the light one, and a true
/// midnight blue is the reverse. Every value below clears 4.5:1 against the
/// background it is used on.
pub fn signal_color(signal: Level, dark_mode: bool) -> Color32 {
    // A net never resolves to a weak level -- `Circuit` normalises those
    // away, so only a *contribution* is ever weak. Folding them back here
    // keeps the match honest without inventing two more colours nothing
    // would ever draw.
    let signal = signal.strengthened();
    if dark_mode {
        match signal {
            Level::High => Color32::from_rgb(72, 200, 96),
            Level::Low => Color32::from_rgb(235, 193, 60),
            Level::Unknown => Color32::from_rgb(116, 138, 240),
            Level::Error => Color32::from_rgb(240, 78, 78),
            // Not one of the four states with a colour of its own: `HighZ`
            // is a driver deliberately not driving, so it reads as "nothing
            // here" rather than as a value.
            Level::HighZ => Color32::from_gray(150),
            _ => Color32::from_gray(150),
        }
    } else {
        match signal {
            Level::High => Color32::from_rgb(22, 120, 45),
            Level::Low => Color32::from_rgb(146, 104, 6),
            Level::Unknown => Color32::from_rgb(40, 52, 150),
            Level::Error => Color32::from_rgb(186, 24, 24),
            Level::HighZ => Color32::from_gray(105),
            _ => Color32::from_gray(105),
        }
    }
}

/// What colour a whole signal is drawn in.
///
/// A plain wire keeps its level's colour. On a **bus** the exception
/// dominates, in the order a schematic is read: a fault anywhere is the
/// error colour, then anything unknown is the unknown one, then a bus
/// nobody drives reads as nothing there.
///
/// A bus that carries a real value gets a colour of its own rather than
/// High's or Low's, because a mix of ones and zeroes is neither and
/// borrowing either would claim the bus was all one thing. The readout
/// beside it is what says *which* value.
pub fn bus_color(signal: &Signal, dark_mode: bool) -> Color32 {
    match bus_level(signal) {
        Some(level) => signal_color(level, dark_mode),
        None if dark_mode => Color32::from_rgb(96, 190, 200),
        None => Color32::from_rgb(18, 106, 116),
    }
}

/// The one state a wire is in, or `None` when it has none to report.
///
/// A plain wire always has one — that is what a level *is*. A **bus** only
/// does when something is wrong or missing: a fault anywhere, a bit nobody
/// knows, or every bit let go of. A bus whose bits are all definite carries
/// a *value*, not a level, and there is no colour that says `0x35`.
///
/// That `None` is what the drawing reads to decide whether the core is
/// worth keeping: with no level to report, a user's colour takes the whole
/// wire instead of ringing it — the same rule that already applies when the
/// signal state is switched off entirely.
pub fn bus_level(signal: &Signal) -> Option<Level> {
    if signal.width() == 1 {
        return Some(signal.only_level());
    }
    let levels: Vec<Level> = signal
        .levels()
        .iter()
        .map(|level| level.strengthened())
        .collect();
    if levels.contains(&Level::Error) {
        Some(Level::Error)
    } else if levels
        .iter()
        .any(|&level| level != Level::High && level != Level::Low && level != Level::HighZ)
    {
        Some(Level::Unknown)
    } else if levels.iter().all(|&level| level == Level::HighZ) {
        Some(Level::HighZ)
    } else if levels.contains(&Level::HighZ) {
        Some(Level::Unknown)
    } else {
        None
    }
}

/// How much a weakly driven net's colour is faded.
///
/// A level delivered only through a pass transistor is a real level — the
/// gate downstream reads it — so it keeps its colour rather than becoming a
/// different state. Fading is the honest cue: the value is there, the margin
/// isn't.
pub const WEAK_FADE: f32 = 0.5;

/// The highlight blue: selection outlines, and the rings marking a pin or
/// waypoint you can drop a wire onto. One definition rather than a literal
/// repeated at each site, and per theme — the bright blue that carries the
/// dark canvas manages only 2.5:1 on the light one.
pub fn accent_color(dark_mode: bool) -> Color32 {
    if dark_mode {
        Color32::from_rgb(90, 160, 255)
    } else {
        Color32::from_rgb(16, 90, 200)
    }
}

pub const GRID_SPACING: f32 = 20.0;
/// Default box size for the auto-generated component appearance — each half
/// extent is a whole multiple of `GRID_SPACING`, so a pin sitting at a rect
/// edge (as every symbol's does — see `symbol.rs`) lands exactly on a grid
/// dot, since `center` itself is always grid-snapped.
pub const BOX_SIZE: Vec2 = egui::vec2(GRID_SPACING * 4.0, GRID_SPACING * 2.0);

/// The next whole number of grid spaces at or above `size`.
///
/// A box that has to grow grows by whole steps, or its pins stop landing on
/// the dots — the rule `BOX_SIZE` itself was built around, applied to a
/// box whose width varies.
pub fn snap_up(size: f32) -> f32 {
    (size / GRID_SPACING).ceil().max(0.0) * GRID_SPACING
}

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
    /// How many quarter-turns clockwise this is from upright.
    pub fn quarter_turns(self) -> usize {
        match self {
            Rotation::Deg0 => 0,
            Rotation::Deg90 => 1,
            Rotation::Deg180 => 2,
            Rotation::Deg270 => 3,
        }
    }

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
/// A short notice across the top of the canvas.
///
/// Painted rather than laid out, and with no widget of its own, so it can
/// never take a click: the canvas underneath is where wires get drawn, and
/// the one thing a notice must not do is get in the way of the work it is
/// commenting on.
///
/// It says what the status bar says. Both, because the bar is at the far
/// edge of the window and the eye is on the circuit — and a notice over the
/// drawing is only tolerable while it is *transient*, which is why the bar
/// keeps the standing version.
pub fn draw_notice(ui: &egui::Ui, within: Rect, text: &str, color: Color32) {
    let painter = ui.painter_at(within);
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), color);
    let padding = egui::vec2(10.0, 5.0);
    let box_size = galley.size() + padding * 2.0;
    let at = pos2(within.center().x - box_size.x / 2.0, within.top() + 10.0);
    let rect = Rect::from_min_size(at, box_size);

    // The panel's own fill, so the drawing behind never shows through the
    // letters, with the accent only on the border and the text.
    painter.rect(
        rect,
        6.0,
        ui.visuals().panel_fill,
        egui::Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.galley(at + padding, galley, color);
}

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

/// Draws a dot grid filling `rect` in `dot_color`.
///
/// The colour comes from the caller rather than being fixed here: a grey
/// picked to sit quietly on the dark canvas is either invisible or far too
/// heavy on the light one, so it has to follow the active theme.
pub fn draw_grid(painter: &Painter, rect: Rect, dot_color: Color32) {
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

/// Draws a faint outline around a component's box while the pointer is over
/// it — the "you're about to grab this" cue, deliberately dimmer than
/// [`draw_selection_outline`] so a hovered component never reads as selected.
pub fn draw_hover_outline(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_stroke(
        rect.expand(3.0),
        6.0,
        Stroke::new(1.5, color),
        StrokeKind::Outside,
    );
}

/// Draws a highlight outline around a selected component's box.
pub fn draw_selection_outline(painter: &Painter, rect: Rect, dark_mode: bool) {
    painter.rect_stroke(
        rect.expand(3.0),
        6.0,
        Stroke::new(2.0, accent_color(dark_mode)),
        StrokeKind::Outside,
    );
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_wire_always_has_a_level_to_report() {
        for level in [Level::High, Level::Low, Level::Unknown, Level::HighZ] {
            assert_eq!(bus_level(&Signal::bit(level)), Some(level));
        }
    }

    #[test]
    fn a_healthy_bus_has_a_value_rather_than_a_level() {
        // Which is the whole reason the casing comes off it: a bus of
        // definite bits carries `0x35`, and there is no colour that says
        // that. Ringing a core reporting nothing is just a thicker wire.
        let value = Signal::from_levels(vec![Level::High, Level::Low, Level::High, Level::Low]);
        assert_eq!(bus_level(&value), None);
        // Even all-high: it is a value, not a level.
        assert_eq!(bus_level(&Signal::splat(Level::High, 4)), None);
    }

    #[test]
    fn a_bus_gets_a_level_back_the_moment_something_is_wrong() {
        // And that is what makes the rule worth having rather than just
        // "no casing on a bus": the core — and the casing around it —
        // return exactly when they have something to say.
        let mut bits = vec![Level::High; 4];
        bits[2] = Level::Error;
        assert_eq!(bus_level(&Signal::from_levels(bits)), Some(Level::Error));

        let mut bits = vec![Level::High; 4];
        bits[1] = Level::Unknown;
        assert_eq!(bus_level(&Signal::from_levels(bits)), Some(Level::Unknown));

        // Everything let go of is a bus that is floating, which is a state.
        assert_eq!(
            bus_level(&Signal::splat(Level::HighZ, 4)),
            Some(Level::HighZ)
        );
        // And one bit let go of while the rest are driven is not knowable.
        let mut bits = vec![Level::Low; 4];
        bits[3] = Level::HighZ;
        assert_eq!(bus_level(&Signal::from_levels(bits)), Some(Level::Unknown));
    }
}
