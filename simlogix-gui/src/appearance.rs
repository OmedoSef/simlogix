//! What a circuit looks like when it is used as a component.
//!
//! A circuit is instantiated as a box with a pin per port. That box is
//! generated, and generated is the wrong word for something a schematic
//! reader has to recognise at a glance — so a circuit can carry a symbol of
//! its own instead.
//!
//! # The generated box is not a separate drawing routine
//!
//! [`Appearance::generated`] *builds* the automatic box as an `Appearance`,
//! and [`Appearance::draw`] is the only thing that draws either kind. So
//! "start editing from the generated box" stores exactly what was already on
//! screen, and the automatic and hand-drawn forms cannot drift apart —
//! there is one renderer, not two that have to agree.
//!
//! # Pins are placed, never drawn
//!
//! The circuit's ports are the truth: a symbol says *where* each pin comes
//! out, and can neither invent one nor drop one. That's what stops a symbol
//! from lying about the circuit behind it — see [`Appearance::pins`].
//!
//! # Coordinates and colour
//!
//! Everything is relative to the symbol's centre, in canvas points, in the
//! canonical [`Rotation::Deg0`] layout — the other three rotations turn every
//! point of it, geometry and pins together, exactly as the hand-coded
//! symbols in [`crate::symbol`] do.
//!
//! Colour is *not* stored. Symbols derive theirs from the theme, so a symbol
//! saved in a light grey would vanish on a white background.

use egui::{pos2, Align2, Color32, Painter, Pos2, Stroke};
use serde::{Deserialize, Serialize};

use crate::canvas::{self, Rotation};
use crate::palette::ComponentKind;
use crate::placed_component::{instance_height, InstancePort};
use crate::symbol::{draw_pin, rotate, rotate_rect, TextLayer};

/// How wide the generated box's body is, as a fraction of the box.
///
/// What's left on each side is the pin's lead, which is why this is also
/// where a generated pin's `lead` comes from.
const GENERATED_LEAD: f32 = 0.16;

/// Gap between the body and a port's name.
const LABEL_INSET: f32 = 4.0;

const STROKE_WIDTH: f32 = 1.6;

/// Gap between the top of a symbol and the circuit's name above it.
const NAME_GAP: f32 = 2.0;

/// How many segments a circle is broken into for aiming and highlighting.
const CIRCLE_SAMPLES: usize = 48;

/// The polyline an arc through three points is drawn as.
///
/// Falls back to the three points themselves when they are collinear (or
/// two of them coincide): there is no circle through them, and a straight
/// run between them is what the user drew.
fn arc_points(start: (f32, f32), mid: (f32, f32), end: (f32, f32)) -> Vec<(f32, f32)> {
    let (ax, ay) = start;
    let (bx, by) = mid;
    let (cx, cy) = end;
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-4 {
        return vec![start, mid, end];
    }
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let radius = (ax - ux).hypot(ay - uy);

    let angle = |(x, y): (f32, f32)| (y - uy).atan2(x - ux);
    let (from, through, to) = (angle(start), angle(mid), angle(end));
    // The sweep is the one that actually passes through the middle point —
    // which is the whole reason the middle point is stored.
    let normalise = |a: f32| a.rem_euclid(std::f32::consts::TAU);
    let forward = normalise(to - from);
    let mid_forward = normalise(through - from);
    let sweep = if mid_forward <= forward {
        forward
    } else {
        forward - std::f32::consts::TAU
    };

    (0..=CIRCLE_SAMPLES)
        .map(|step| {
            let a = from + sweep * (step as f32 / CIRCLE_SAMPLES as f32);
            (ux + radius * a.cos(), uy + radius * a.sin())
        })
        .collect()
}

/// What a drawn shape's points snap to.
///
/// A quarter of the grid, not the whole step: pins have to land on grid dots
/// and do, but 20 points across a component box leaves four cells to draw a
/// symbol in, which is not enough to draw anything. Fine enough to shape a
/// gate, coarse enough that two lines meant to meet actually do.
pub const SHAPE_SNAP: f32 = canvas::GRID_SPACING / 4.0;
const PORT_NAME_SIZE: f32 = 9.0;

/// A circuit's own symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    /// The line art, drawn in order.
    pub shapes: Vec<Shape>,
    /// One entry per port, in the order [`crate::app::SimLogixApp::port_slots`]
    /// lays them out — so entry `i` is instance pin `i`.
    ///
    /// A symbol places the pins the circuit already has. Adding a port adds a
    /// slot; removing one removes it. Neither is the symbol's decision.
    pub pins: Vec<PinSlot>,
    /// Whether the circuit's name is written above the symbol.
    ///
    /// True on the generated box, where the box says nothing on its own and
    /// the name is the only thing identifying it. A symbol you drew often
    /// says what it is by its shape, or carries its own label in a place you
    /// chose — and then a name floating above it is in the way.
    ///
    /// Defaulted to true when absent so a symbol saved before this existed
    /// keeps drawing what it drew.
    #[serde(default = "shows_name_by_default")]
    pub show_name: bool,
}

fn shows_name_by_default() -> bool {
    true
}

/// One piece of line art. Deliberately a small set: these are schematic
/// symbols, not drawings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Polyline {
        points: Vec<(f32, f32)>,
        /// Whether the last point joins back to the first.
        closed: bool,
    },
    Circle {
        center: (f32, f32),
        radius: f32,
    },
    /// An arc, stored as the three points it passes through rather than as
    /// a centre, a radius and two angles.
    ///
    /// Three points because that is what the gesture gives and what editing
    /// wants back: each one snaps, moves and reads like any other point,
    /// whereas an angle pair has no handle you can grab. Three points on a
    /// line describe no circle, and that case degenerates to the polyline
    /// through them rather than being rejected.
    Arc {
        start: (f32, f32),
        mid: (f32, f32),
        end: (f32, f32),
    },
    /// Text the user wrote. The standing convention that symbols carry no
    /// text applies to the ones *this application* draws; a symbol you drew
    /// is yours, the same exception the `name` property is.
    Text {
        at: (f32, f32),
        align: TextAlign,
        size: f32,
        text: String,
    },
}

/// Where a piece of text sits relative to its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    fn align2(self) -> Align2 {
        match self {
            Self::Left => Align2::LEFT_CENTER,
            Self::Center => Align2::CENTER_CENTER,
            Self::Right => Align2::RIGHT_CENTER,
        }
    }
}

/// Where one port's pin comes out of the symbol.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PinSlot {
    /// The connection point itself — what a wire attaches to. Lands on a
    /// grid dot, which is the whole reason the box's height is rounded.
    pub at: (f32, f32),
    /// Which way the pin points *outward*; the lead runs the other way.
    pub facing: Facing,
    /// Length of the lead line drawn inward from `at`. Zero draws none.
    pub lead: f32,
    /// Whether to write the port's name at the lead's inner end.
    pub show_name: bool,
    /// Nudges that name away from where it would otherwise land, in the
    /// symbol's own coordinates.
    ///
    /// The automatic place is a fixed step in from the lead, which is right
    /// until the line art is somewhere else: a name against a sloped edge —
    /// the side of a multiplexer, say — is unreadable, and nothing about the
    /// pin can work out where the drawing left room. So this is set by hand,
    /// in the same coordinates every other field is typed in rather than
    /// along the pin's facing, which would flip meaning as the pin is moved
    /// from one edge to another.
    ///
    /// Absent means no nudge, so a symbol that never asks for one is written
    /// exactly as it was before this existed.
    #[serde(default, skip_serializing_if = "is_no_offset")]
    pub name_offset: (f32, f32),
}

fn is_no_offset(offset: &(f32, f32)) -> bool {
    *offset == (0.0, 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Facing {
    Left,
    Right,
    Up,
    Down,
}

impl Facing {
    /// The unit step from the pin towards the body.
    fn inward(self) -> (f32, f32) {
        match self {
            Self::Left => (1.0, 0.0),
            Self::Right => (-1.0, 0.0),
            Self::Up => (0.0, 1.0),
            Self::Down => (0.0, -1.0),
        }
    }

    /// How a name sitting inward of this pin should be aligned so it reads
    /// away from the edge rather than across it.
    fn label_align(self) -> Align2 {
        match self {
            Self::Left => Align2::LEFT_CENTER,
            Self::Right => Align2::RIGHT_CENTER,
            Self::Up | Self::Down => Align2::CENTER_CENTER,
        }
    }
}

impl Appearance {
    /// The automatic box: a rectangle, inputs down the left, outputs down
    /// the right, each port's name written inside.
    ///
    /// This is what a circuit gets until it is given a symbol, and it is
    /// also the starting point when you decide to draw one — see the module
    /// docs for why that has to be the same code.
    pub fn generated(ports: &[InstancePort]) -> Self {
        let half_w = canvas::BOX_SIZE.x / 2.0;
        let half_h = instance_height(ports) / 2.0;
        let lead = canvas::BOX_SIZE.x * GENERATED_LEAD;
        let body = half_w - lead;

        let shapes = vec![Shape::Polyline {
            points: vec![
                (-body, -half_h),
                (body, -half_h),
                (body, half_h),
                (-body, half_h),
            ],
            closed: true,
        }];

        // Whole grid steps down from the top edge, so every pin lands on a
        // dot whatever the height works out to be.
        let (mut left, mut right) = (0usize, 0usize);
        let pins = ports
            .iter()
            .map(|port| {
                let is_output = port.kind == ComponentKind::OutputPort;
                let slot = if is_output { &mut right } else { &mut left };
                let y = -half_h + (*slot as f32 + 1.0) * canvas::GRID_SPACING;
                *slot += 1;
                PinSlot {
                    at: (if is_output { half_w } else { -half_w }, y),
                    facing: if is_output {
                        Facing::Right
                    } else {
                        Facing::Left
                    },
                    lead,
                    show_name: true,
                    name_offset: (0.0, 0.0),
                }
            })
            .collect();

        Self {
            shapes,
            pins,
            show_name: true,
        }
    }

    /// Draws the symbol centred on `center`, and reports where each pin
    /// ended up — in `pins` order, which is port order.
    ///
    /// `port_names` is read alongside `pins`; a slot with no matching name
    /// simply goes unlabelled rather than the two being forced to agree.
    pub fn draw(
        &self,
        painter: &Painter,
        center: Pos2,
        rotation: Rotation,
        color: Color32,
        port_names: &[&str],
        text_layer: &TextLayer,
    ) -> Vec<Pos2> {
        let stroke = Stroke::new(STROKE_WIDTH, color);
        // Symbol coordinates are centre-relative, so placing and rotating
        // are the same step.
        let at = |(x, y): (f32, f32)| rotate(pos2(center.x + x, center.y + y), center, rotation);

        for shape in &self.shapes {
            match shape {
                Shape::Polyline { points, closed } => {
                    let mut path: Vec<Pos2> = points.iter().map(|&p| at(p)).collect();
                    if *closed {
                        if let Some(&first) = path.first() {
                            path.push(first);
                        }
                    }
                    painter.line(path, stroke);
                }
                Shape::Circle { center: c, radius } => {
                    painter.circle_stroke(at(*c), *radius, stroke);
                }
                Shape::Arc { start, mid, end } => {
                    painter.line(
                        arc_points(*start, *mid, *end).into_iter().map(at).collect(),
                        stroke,
                    );
                }
                Shape::Text {
                    at: p,
                    align,
                    size,
                    text,
                } => {
                    text_layer.text(at(*p), align.align2(), text, *size, color);
                }
            }
        }

        self.pins
            .iter()
            .enumerate()
            .map(|(index, pin)| {
                let (dx, dy) = pin.facing.inward();
                let inner = (pin.at.0 + dx * pin.lead, pin.at.1 + dy * pin.lead);
                if pin.lead > 0.0 {
                    painter.line_segment([at(pin.at), at(inner)], stroke);
                }
                let point = at(pin.at);
                draw_pin(painter, point, color);

                if pin.show_name {
                    if let Some(name) = port_names.get(index).filter(|name| !name.is_empty()) {
                        let label = (
                            inner.0 + dx * LABEL_INSET + pin.name_offset.0,
                            inner.1 + dy * LABEL_INSET + pin.name_offset.1,
                        );
                        text_layer.text(
                            at(label),
                            pin.facing.label_align(),
                            name,
                            PORT_NAME_SIZE,
                            color,
                        );
                    }
                }
                point
            })
            .collect()
    }

    /// One shape's outline in canvas coordinates — what the painter would
    /// stroke, so aiming at a shape and highlighting it read the same
    /// geometry rather than each computing its own.
    ///
    /// A circle is sampled; text reduces to its anchor point.
    pub fn shape_path(&self, index: usize, center: Pos2) -> Vec<Pos2> {
        let at = |(x, y): (f32, f32)| pos2(center.x + x, center.y + y);
        match self.shapes.get(index) {
            Some(Shape::Polyline { points, closed }) => {
                let mut path: Vec<Pos2> = points.iter().map(|&p| at(p)).collect();
                if *closed {
                    if let Some(&first) = path.first() {
                        path.push(first);
                    }
                }
                path
            }
            Some(Shape::Circle { center: c, radius }) => (0..=CIRCLE_SAMPLES)
                .map(|step| {
                    let angle = step as f32 / CIRCLE_SAMPLES as f32 * std::f32::consts::TAU;
                    at((c.0 + radius * angle.cos(), c.1 + radius * angle.sin()))
                })
                .collect(),
            Some(Shape::Arc { start, mid, end }) => {
                arc_points(*start, *mid, *end).into_iter().map(at).collect()
            }
            Some(Shape::Text { at: p, .. }) => vec![at(*p)],
            None => Vec::new(),
        }
    }

    /// How far `point` is from shape `index`. Used to pick what a click
    /// landed on.
    pub fn distance_to_shape(&self, index: usize, point: Pos2, center: Pos2) -> f32 {
        let path = self.shape_path(index, center);
        match path.as_slice() {
            [] => f32::INFINITY,
            // A single point (text) has no segments to measure against.
            [only] => only.distance(point),
            path => canvas::distance_to_path(point, path),
        }
    }

    /// Moves one shape bodily. Points are the only thing a symbol is made
    /// of, so this is the same operation whatever the shape.
    pub fn translate_shape(&mut self, index: usize, by: egui::Vec2) {
        let shift = |p: &mut (f32, f32)| {
            p.0 += by.x;
            p.1 += by.y;
        };
        match self.shapes.get_mut(index) {
            Some(Shape::Polyline { points, .. }) => points.iter_mut().for_each(shift),
            Some(Shape::Circle { center, .. }) => shift(center),
            Some(Shape::Arc { start, mid, end }) => {
                shift(start);
                shift(mid);
                shift(end);
            }
            Some(Shape::Text { at, .. }) => shift(at),
            None => {}
        }
    }

    /// Puts every one of a shape's points back on the drawing step, after a
    /// drag has left them wherever the pointer was.
    pub fn snap_shape(&mut self, index: usize) {
        let snap = |value: &mut f32| *value = (*value / SHAPE_SNAP).round() * SHAPE_SNAP;
        let snap_point = |p: &mut (f32, f32)| {
            snap(&mut p.0);
            snap(&mut p.1);
        };
        match self.shapes.get_mut(index) {
            Some(Shape::Polyline { points, .. }) => points.iter_mut().for_each(snap_point),
            Some(Shape::Circle { center, radius }) => {
                snap_point(center);
                snap(radius);
            }
            Some(Shape::Arc { start, mid, end }) => {
                snap_point(start);
                snap_point(mid);
                snap_point(end);
            }
            Some(Shape::Text { at, .. }) => snap_point(at),
            None => {}
        }
    }

    /// Where the circuit's name goes: just above the topmost thing drawn.
    ///
    /// Read from the top of [`Appearance::bounds`] rather than from a height
    /// about the centre, so a shape added at the *bottom* of a symbol doesn't
    /// push the name up away from it.
    pub fn name_anchor(&self, center: Pos2) -> Pos2 {
        pos2(center.x, center.y + self.bounds().top() - NAME_GAP)
    }

    /// The box this symbol occupies on the canvas — what you click, drag and
    /// see the selection outline around.
    ///
    /// Turned with the symbol, because [`Appearance::draw`] turns every point
    /// it draws. Without that they part company, and a symbol drawn away from
    /// its own origin parts company *completely*: one drawn 120 above it lands
    /// 120 below when turned half a circle, while the box stayed where the
    /// drawing used to be — nothing left to click, so the component could no
    /// longer be selected or moved at all.
    pub fn rect(&self, center: Pos2, rotation: Rotation) -> egui::Rect {
        rotate_rect(self.bounds().translate(center.to_vec2()), center, rotation)
    }

    /// The symbol's extent about its centre — **not symmetric**.
    ///
    /// It used to be a half-width and a half-height, taken from the largest
    /// distance from the centre in each axis and mirrored. That made a
    /// lopsided symbol claim as much space on the empty side as on the drawn
    /// one: a shape low down pushed the box just as far *up*, so the name
    /// floated away above nothing and the clickable area reached into blank
    /// canvas. Reporting where the drawing actually is fixes both.
    ///
    /// Never smaller than one component box, so a symbol of two short lines
    /// is still something you can hit.
    pub fn bounds(&self) -> egui::Rect {
        let mut bounds: Option<egui::Rect> = None;
        let mut include = |x: f32, y: f32| {
            let point = pos2(x, y);
            bounds = Some(match bounds {
                Some(rect) => rect.union(egui::Rect::from_min_max(point, point)),
                None => egui::Rect::from_min_max(point, point),
            });
        };
        for shape in &self.shapes {
            match shape {
                Shape::Polyline { points, .. } => {
                    for &(x, y) in points {
                        include(x, y);
                    }
                }
                Shape::Circle {
                    center: (x, y),
                    radius,
                } => {
                    include(x - radius, y - radius);
                    include(x + radius, y + radius);
                }
                Shape::Arc { start, mid, end } => {
                    // Sampled rather than bounded by its three points: an arc
                    // bulges past them, and a symbol's extent is what can be
                    // seen of it.
                    for (x, y) in arc_points(*start, *mid, *end) {
                        include(x, y);
                    }
                }
                // Text is left out: its drawn size depends on the zoom, so
                // letting it decide the box would make the hit area change as
                // the wheel is scrolled.
                Shape::Text { .. } => {}
            }
        }
        for pin in &self.pins {
            include(pin.at.0, pin.at.1);
        }

        // Nothing drawn at all: one component box on the origin, so a symbol
        // with no shapes and no pins is still somewhere you can click.
        let Some(bounds) = bounds else {
            return egui::Rect::from_center_size(Pos2::ZERO, canvas::BOX_SIZE);
        };

        // The minimum is a minimum *size*, grown about the drawing's own
        // centre. It used to be a box on the origin that the drawing was
        // merged with, which is a different thing: a symbol drawn to one
        // side of its origin — as one naturally is once a pin has been
        // dragged out — had the box reach back across the origin and out the
        // far side, so its hover outline and its click area extended well
        // past anything visible.
        egui::Rect::from_center_size(
            bounds.center(),
            egui::vec2(
                bounds.width().max(canvas::BOX_SIZE.x),
                bounds.height().max(canvas::BOX_SIZE.y),
            ),
        )
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

    fn ports(inputs: usize, outputs: usize) -> Vec<InstancePort> {
        let mut ports: Vec<InstancePort> = (0..inputs)
            .map(|_| port(ComponentKind::InputPort))
            .collect();
        ports.extend((0..outputs).map(|_| port(ComponentKind::OutputPort)));
        ports
    }

    #[test]
    fn every_generated_pin_lands_on_a_grid_dot() {
        for inputs in 1..6 {
            for outputs in 1..6 {
                let generated = Appearance::generated(&ports(inputs, outputs));
                for pin in &generated.pins {
                    // Pins are placed relative to the centre, which is itself
                    // snapped, so both offsets have to be whole steps.
                    for offset in [pin.at.0, pin.at.1] {
                        let steps = offset / canvas::GRID_SPACING;
                        assert!(
                            (steps - steps.round()).abs() < 1e-3,
                            "{inputs} in, {outputs} out: {offset} is not a whole grid step"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_generated_box_has_one_pin_per_port_in_port_order() {
        let generated = Appearance::generated(&ports(2, 1));
        assert_eq!(generated.pins.len(), 3);
        // Inputs came first, so they take the left; the output faces right.
        assert_eq!(generated.pins[0].facing, Facing::Left);
        assert_eq!(generated.pins[1].facing, Facing::Left);
        assert_eq!(generated.pins[2].facing, Facing::Right);
        // Each side is numbered from the top independently, so the lone
        // output sits level with the *first* input, not below both.
        assert_eq!(generated.pins[0].at.1, generated.pins[2].at.1);
    }

    #[test]
    fn the_box_reaches_exactly_as_far_as_its_pins() {
        let generated = Appearance::generated(&ports(3, 3));
        let bounds = generated.bounds();
        assert_eq!(bounds.width(), canvas::BOX_SIZE.x);
        assert_eq!(bounds.height(), instance_height(&ports(3, 3)));
        // The generated box *is* symmetric, so it still sits on its centre.
        assert_eq!(bounds.center(), Pos2::ZERO);
    }

    #[test]
    fn a_symbol_drawn_to_one_side_of_its_origin_claims_no_space_on_the_other() {
        // What Romain saw: a hover outline reaching well past the right of a
        // symbol he had drawn to the left of its origin. The minimum size
        // used to be a box *on the origin* that the drawing was merged with,
        // so it reached back across the origin and out the far side.
        let symbol = Appearance {
            shapes: vec![Shape::Polyline {
                points: vec![(-140.0, -60.0), (-20.0, -60.0), (-20.0, 20.0)],
                closed: true,
            }],
            pins: Vec::new(),
            show_name: false,
        };

        let bounds = symbol.bounds();
        assert_eq!(bounds.min.x, -140.0);
        assert_eq!(bounds.max.x, -20.0, "nothing is drawn to the right of this");
    }

    #[test]
    fn a_symbol_smaller_than_a_component_box_is_still_something_you_can_hit() {
        // The minimum is a minimum *size*, grown about the drawing's own
        // centre rather than snapped back to the origin.
        let symbol = Appearance {
            shapes: vec![Shape::Polyline {
                points: vec![(100.0, 100.0), (110.0, 100.0)],
                closed: false,
            }],
            pins: Vec::new(),
            show_name: false,
        };

        let bounds = symbol.bounds();
        assert_eq!(bounds.width(), canvas::BOX_SIZE.x);
        assert_eq!(bounds.height(), canvas::BOX_SIZE.y);
        assert_eq!(bounds.center(), pos2(105.0, 100.0), "grown where it sits");
    }

    #[test]
    fn a_symbol_with_nothing_in_it_falls_back_to_a_box_on_the_origin() {
        let blank = Appearance {
            shapes: Vec::new(),
            pins: Vec::new(),
            show_name: false,
        };
        assert_eq!(blank.bounds().center(), Pos2::ZERO);
        assert_eq!(blank.bounds().width(), canvas::BOX_SIZE.x);
    }

    #[test]
    fn a_symbol_drawn_away_from_its_origin_is_clickable_where_it_lands() {
        let mut symbol = Appearance::generated(&ports(1, 1));
        // Drawn well above its own origin, which is what you naturally end
        // up with once a pin has been dragged out — and what Romain's
        // controlled buffer looks like.
        symbol.shapes.push(Shape::Polyline {
            points: vec![(-30.0, -140.0), (30.0, -120.0), (-30.0, -100.0)],
            closed: true,
        });

        let center = pos2(100.0, 100.0);
        let upright = symbol.rect(center, Rotation::Deg0);
        let turned = symbol.rect(center, Rotation::Deg180);

        // Half a circle takes the corner at (-30, -140) to (30, 140), so
        // that is where the box has to reach.
        let landed = center + egui::vec2(30.0, 140.0);
        assert!(turned.contains(landed));
        // And this is what makes the assertion above worth making: the box
        // used to stay where the drawing had been, which for a symbol this
        // far off its origin left the two with nothing in common.
        assert!(!upright.contains(landed));
    }

    #[test]
    fn a_nudged_name_is_stored_and_an_old_symbol_reads_back_without_one() {
        let mut symbol = Appearance::generated(&ports(1, 0));
        symbol.pins[0].name_offset = (-SHAPE_SNAP, 2.0 * SHAPE_SNAP);
        let json = serde_json::to_string(&symbol).expect("serialisable");
        assert!(json.contains("name_offset"));
        let back: Appearance = serde_json::from_str(&json).expect("readable");
        assert_eq!(back.pins[0].name_offset, (-SHAPE_SNAP, 2.0 * SHAPE_SNAP));

        // And a pin written before the field existed still reads, at rest.
        let old = r#"{"at":[0.0,0.0],"facing":"Left","lead":10.0,"show_name":true}"#;
        let pin: PinSlot = serde_json::from_str(old).expect("readable");
        assert_eq!(pin.name_offset, (0.0, 0.0));
    }

    #[test]
    fn something_drawn_low_down_does_not_push_the_name_up_away_from_the_symbol() {
        let mut symbol = Appearance::generated(&ports(1, 1));
        let before = symbol.name_anchor(Pos2::ZERO);

        // A shape well below everything else. The extent used to be a height
        // about the centre, mirrored — so this pushed the top up by just as
        // much and left the name floating above blank canvas.
        symbol.shapes.push(Shape::Polyline {
            points: vec![(-20.0, 200.0), (20.0, 200.0)],
            closed: false,
        });

        assert_eq!(symbol.name_anchor(Pos2::ZERO), before);
        // ...and the box grew downwards only.
        assert_eq!(symbol.bounds().bottom(), 200.0);
    }

    #[test]
    fn the_name_follows_something_drawn_above_the_symbol() {
        let mut symbol = Appearance::generated(&ports(1, 1));
        symbol.shapes.push(Shape::Circle {
            center: (0.0, -100.0),
            radius: 10.0,
        });

        // The other half of the rule: it stays *above the topmost thing*.
        assert!(symbol.name_anchor(Pos2::ZERO).y <= -110.0);
    }

    #[test]
    fn a_symbol_saved_before_the_option_existed_still_shows_its_name() {
        // The generated box says nothing on its own, so the name is the only
        // thing identifying it — a symbol written by an older build has to
        // keep drawing what it drew.
        let older = r#"{"shapes":[],"pins":[]}"#;
        let read: Appearance = serde_json::from_str(older).expect("readable");
        assert!(read.show_name);
    }

    #[test]
    fn an_arc_passes_through_all_three_of_its_points() {
        // A half circle over the top: the middle point is what says which
        // way round it goes, and it has to be on the curve.
        let points = arc_points((-10.0, 0.0), (0.0, -10.0), (10.0, 0.0));
        let near = |p: (f32, f32), (x, y): (f32, f32)| (p.0 - x).hypot(p.1 - y) < 0.01;

        assert!(near(points[0], (-10.0, 0.0)));
        assert!(near(*points.last().expect("sampled"), (10.0, 0.0)));
        assert!(
            points.iter().any(|&p| near(p, (0.0, -10.0))),
            "the middle point is on the curve"
        );
        // Every sample is on the circle the three points define.
        for &(x, y) in &points {
            assert!((x.hypot(y) - 10.0).abs() < 0.01);
        }
    }

    #[test]
    fn the_middle_point_is_what_picks_the_long_way_round() {
        let over = arc_points((-10.0, 0.0), (0.0, -10.0), (10.0, 0.0));
        let under = arc_points((-10.0, 0.0), (0.0, 10.0), (10.0, 0.0));

        // Same ends, opposite bulges — which is exactly why an arc is stored
        // as three points rather than as a centre and a radius.
        assert!(over.iter().all(|p| p.1 <= 0.01));
        assert!(under.iter().all(|p| p.1 >= -0.01));
    }

    #[test]
    fn three_points_in_a_line_degenerate_to_the_line() {
        // No circle passes through them; drawing nothing, or dividing by a
        // vanishing determinant, would both be worse than the straight run
        // the user plainly drew.
        assert_eq!(
            arc_points((0.0, 0.0), (5.0, 0.0), (10.0, 0.0)),
            vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]
        );
    }

    #[test]
    fn an_arc_is_bounded_by_its_bulge_not_by_its_ends() {
        let symbol = Appearance {
            shapes: vec![Shape::Arc {
                start: (-40.0, 0.0),
                mid: (0.0, -40.0),
                end: (40.0, 0.0),
            }],
            pins: Vec::new(),
            show_name: true,
        };

        // The three points reach 40 up; so does the curve, and a bound taken
        // from the ends alone would have missed it entirely.
        assert!(symbol.bounds().top() <= -40.0);
    }

    #[test]
    fn a_label_is_aimed_at_by_its_anchor_point() {
        let symbol = Appearance {
            shapes: vec![Shape::Text {
                at: (20.0, -10.0),
                align: TextAlign::Center,
                size: 10.0,
                text: "Q".to_string(),
            }],
            pins: Vec::new(),
            show_name: true,
        };

        // Text has no outline to measure against, so the point it hangs
        // from is what a click is compared to.
        assert!(symbol.distance_to_shape(0, pos2(20.0, -10.0), Pos2::ZERO) < 0.01);
        assert!(symbol.distance_to_shape(0, pos2(60.0, -10.0), Pos2::ZERO) > 30.0);
    }

    #[test]
    fn a_click_picks_the_shape_it_landed_on_and_not_the_one_behind_it() {
        let symbol = Appearance {
            shapes: vec![
                Shape::Polyline {
                    points: vec![(-40.0, 0.0), (40.0, 0.0)],
                    closed: false,
                },
                Shape::Circle {
                    center: (0.0, 60.0),
                    radius: 10.0,
                },
            ],
            pins: Vec::new(),
            show_name: true,
        };
        let origin = Pos2::ZERO;

        // On the line: near the first, far from the circle.
        assert!(symbol.distance_to_shape(0, pos2(10.0, 1.0), origin) < 2.0);
        assert!(symbol.distance_to_shape(1, pos2(10.0, 1.0), origin) > 20.0);
        // On the circle's *edge* — a circle is an outline, so its own centre
        // is a full radius away from it.
        assert!(symbol.distance_to_shape(1, pos2(0.0, 70.0), origin) < 2.0);
        assert!(symbol.distance_to_shape(1, pos2(0.0, 60.0), origin) > 8.0);
    }

    #[test]
    fn moving_a_shape_moves_every_point_of_it_and_nothing_else() {
        let mut symbol = Appearance {
            shapes: vec![
                Shape::Polyline {
                    points: vec![(0.0, 0.0), (10.0, 4.0)],
                    closed: false,
                },
                Shape::Circle {
                    center: (0.0, 0.0),
                    radius: 5.0,
                },
            ],
            pins: Vec::new(),
            show_name: true,
        };

        symbol.translate_shape(0, egui::vec2(3.0, -2.0));

        assert_eq!(
            symbol.shapes[0],
            Shape::Polyline {
                points: vec![(3.0, -2.0), (13.0, 2.0)],
                closed: false,
            }
        );
        assert_eq!(
            symbol.shapes[1],
            Shape::Circle {
                center: (0.0, 0.0),
                radius: 5.0
            },
            "the shape that wasn't grabbed stays put"
        );
    }

    #[test]
    fn dropping_a_shape_puts_its_points_back_on_the_drawing_step() {
        let mut symbol = Appearance {
            shapes: vec![Shape::Circle {
                center: (3.2, -7.9),
                radius: 12.4,
            }],
            pins: Vec::new(),
            show_name: true,
        };

        symbol.snap_shape(0);

        // A radius that doesn't snap too would let two circles drawn the
        // same way come out different sizes.
        assert_eq!(
            symbol.shapes[0],
            Shape::Circle {
                center: (SHAPE_SNAP, -2.0 * SHAPE_SNAP),
                radius: 2.0 * SHAPE_SNAP,
            }
        );
    }

    #[test]
    fn a_symbol_survives_a_round_trip_through_the_file() {
        let symbol = Appearance {
            shapes: vec![
                Shape::Polyline {
                    points: vec![(-10.0, -10.0), (10.0, 0.0), (-10.0, 10.0)],
                    closed: true,
                },
                Shape::Circle {
                    center: (14.0, 0.0),
                    radius: 4.0,
                },
                Shape::Text {
                    at: (0.0, 0.0),
                    align: TextAlign::Center,
                    size: 9.0,
                    text: "≥1".to_string(),
                },
            ],
            pins: vec![PinSlot {
                at: (-20.0, 0.0),
                facing: Facing::Left,
                lead: 10.0,
                show_name: false,
                name_offset: (0.0, 0.0),
            }],
            show_name: true,
        };

        let json = serde_json::to_string(&symbol).expect("serialisable");
        // A symbol that never nudges a name says nothing about it, so one
        // written by an earlier build is what a later one writes back.
        assert!(!json.contains("name_offset"));
        let back: Appearance = serde_json::from_str(&json).expect("readable");
        assert_eq!(back, symbol);
    }
}
