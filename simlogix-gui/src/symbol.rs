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

use crate::canvas::{Rotation, GRID_SPACING};
use crate::palette::ComponentKind;
use simlogix_core::PortDrive;

const PIN_RADIUS: f32 = 3.0;

/// Segments in the toolbar's arc icon — few, since it is 18 px across.
const ARC_ICON_SAMPLES: usize = 16;
/// How far a `Button`'s cap sinks towards its pin while held.
const CAP_TRAVEL: f32 = 2.5;

/// Where a symbol's text goes, and at what size.
///
/// Labels are painted into a layer that carries **no** transform, at a
/// position mapped out of the canvas's own coordinates and at a size scaled
/// by the zoom. That is the only way to keep them sharp: `egui::Scene`
/// transforms a whole layer, which scales glyphs that have already been
/// rasterised, so text inside it is resampled at any zoom but 1. Compensating
/// the font size cannot help — rasterised at `g` and shown at `g × zoom`, the
/// two agree only at zoom 1. Painting outside the transform makes the
/// rasterised size *be* the displayed size, at every zoom.
///
/// Outside a `Scene` (the palette, for one) the transform is the identity and
/// this behaves exactly like painting normally.
pub struct TextLayer {
    painter: Painter,
    /// Canvas coordinates to screen coordinates.
    to_screen: egui::emath::TSTransform,
}

impl TextLayer {
    /// Builds one for whatever layer `ui` is currently painting into.
    pub fn for_ui(ui: &egui::Ui) -> Self {
        let to_screen = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();

        // A **sublayer of the canvas**, which is how it gets to sit directly
        // on top of the drawing while still belonging to it.
        //
        // It was `Order::Foreground` at first, and that put it above every
        // floating window: the About box had circuit labels printed across
        // it. Foreground is where menus and popups go — this is neither. The
        // order is inherited from the canvas (`Scene` does the same thing for
        // its own layer, for the same reason), so windows, which are
        // `Order::Middle`, are above it again.
        let layer = Self::layer_id(ui.layer_id());
        ui.ctx().set_sublayer(ui.layer_id(), layer);

        Self {
            // Clipped to the same region as the caller: a layer of its own
            // is not bounded by the panel the canvas sits in, and labels
            // would otherwise spill over the panels beside it.
            painter: ui
                .ctx()
                .layer_painter(layer)
                .with_clip_rect(to_screen * ui.clip_rect()),
            to_screen,
        }
    }

    /// Which layer the text for a given canvas layer goes into.
    ///
    /// Derived rather than stored, and exposed so the one test that checks
    /// the paint order asks the same question the drawing answers — a second
    /// copy of this rule in a test is a second thing to keep in step.
    pub fn layer_id(canvas: egui::LayerId) -> egui::LayerId {
        egui::LayerId::new(canvas.order, egui::Id::new(("symbol_text", canvas.id)))
    }

    /// A plain painter, for callers with no transform to undo.
    pub fn plain(painter: Painter) -> Self {
        Self {
            painter,
            to_screen: egui::emath::TSTransform::IDENTITY,
        }
    }

    /// `size` is in canvas units; the zoom is applied here.
    pub fn text(&self, at: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
        let zoom = self.to_screen.scaling;
        self.painter.text(
            self.to_screen * at,
            align,
            text,
            FontId::proportional(size * zoom),
            color,
        );
    }
}

/// Where a drawn component's pins ended up — a wire attaches at these exact
/// points — in the same order `Circuit::pins` reports them.
#[derive(Default)]
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
    /// `Probe` and the ports: the net state being read, e.g. `"1"`/`"0"`.
    pub label: &'a str,
    /// What to draw that readout in, when it shouldn't follow the symbol's
    /// own colour.
    ///
    /// A `Probe` is nothing *but* its readout, so its whole symbol takes the
    /// signal colour. A port's body and arrow say which way the value
    /// crosses the boundary — structure, not state — so only the letter
    /// follows the signal.
    pub label_color: Option<Color32>,
    /// `Button`: whether its cap is currently down.
    pub pressed: bool,
    /// `TriStateSource`: which way its lever is thrown. The readout says
    /// what the *net* settled on, which is a different question — the whole
    /// point of the component is to let go and watch something else answer.
    pub level: Option<PortDrive>,
    /// `Splitter`: the width of each branch, from bit 0 upward. Empty in the
    /// palette and under the pointer, where there is no component yet — the
    /// symbol then draws a representative two-branch shape.
    pub branches: &'a [usize],
}

/// Draws `kind`'s icon within `rect`, oriented by `rotation`, in `color`, and
/// returns where its pins ended up.
/// Whether a kind keeps its body upright and moves only its pin when it is
/// turned.
///
/// **Every symbol that is mostly a readout.** Text is never drawn rotated —
/// it reads left to right on screen whatever the component does — so
/// turning the body of one on its side leaves a tall narrow box with a wide
/// value across it, which is not a symbol at all. Turning it a quarter and
/// keeping the words upright would be worse still: the box and its contents
/// would disagree about which way is up.
///
/// So they follow the rule the generic box has always followed — the body
/// stays axis-aligned and *which edge carries the pin* is what turns. It is
/// Romain's suggestion, and it is the same answer this project already gave
/// once.
pub fn keeps_upright(kind: &ComponentKind) -> bool {
    crate::properties::Properties::has_base(kind)
}

/// Where a pin sits once the body has been left upright and only the pin
/// moved around it.
///
/// `natural` is the edge it comes out of at rest, as a quarter-turn count
/// clockwise from the right — a port's lead leaves rightwards, a probe's
/// leftwards. Turning adds to that, so a quarter turn sends a port's pin to
/// the bottom rather than mapping a point of an unrotated box onto nothing.
fn pin_on_edge(rect: Rect, natural: usize, rotation: Rotation) -> Pos2 {
    let c = rect.center();
    match (natural + rotation.quarter_turns()) % 4 {
        0 => pos2(rect.right(), c.y),
        1 => pos2(c.x, rect.bottom()),
        2 => pos2(rect.left(), c.y),
        _ => pos2(c.x, rect.top()),
    }
}

/// The point on `body` a lead to `pin` should start from — the middle of
/// whichever of its edges faces that way.
fn lead_from(body: Rect, pin: Pos2) -> Pos2 {
    let c = body.center();
    if (pin.x - c.x).abs() > (pin.y - c.y).abs() {
        pos2(
            if pin.x > c.x {
                body.right()
            } else {
                body.left()
            },
            c.y,
        )
    } else {
        pos2(
            c.x,
            if pin.y > c.y {
                body.bottom()
            } else {
                body.top()
            },
        )
    }
}

/// A length that is fixed on a real box and proportional on anything
/// smaller.
///
/// A symbol is drawn both on the canvas, where its box grows with what it
/// has to show, and as a palette icon a good deal smaller than one. A plain
/// fraction stretches with the box; a plain constant swallows the icon. The
/// cap is what lets one drawing serve both.
fn fixed(width: f32, fraction: f32, at_most: f32) -> f32 {
    (width * fraction).min(at_most)
}

/// The size every readout is drawn at.
///
/// One size for all of them, because the box is measured from it: a symbol
/// drawing its value larger than it was measured is one that clips it.
pub const READOUT_SIZE: f32 = 11.0;

/// How wide the widest character a readout can show is, in the real font.
///
/// The **widest character**, not the string in front of you: what a box is
/// sized by must not depend on the value, or the symbol would change size
/// as the simulation ran and take its pins with it. Measured rather than
/// estimated, since a box a little too narrow clips the value it exists to
/// show — and one too wide leaves a gap that grows with every character.
///
/// Worked out once a frame and handed to every component, rather than
/// sixteen layouts per readout.
pub fn readout_char_width(ui: &egui::Ui) -> f32 {
    let font = egui::FontId::proportional(READOUT_SIZE);
    // Every character a readout can hold: the hex digits, and the letters
    // that stand for a state rather than a value.
    "0123456789ABCDEFZ?"
        .chars()
        .map(|glyph| {
            ui.painter()
                .layout_no_wrap(glyph.to_string(), font.clone(), egui::Color32::WHITE)
                .rect
                .width()
        })
        .fold(0.0_f32, f32::max)
}

/// What a symbol shows when there is no component behind it yet — in the
/// palette, and under the pointer while placing one.
///
/// Two symbols are nothing but a readout, so drawn empty they say nothing
/// at all: the probe's circle and the constant's tag. A constant shows `0`
/// because that is genuinely what a freshly placed one drives; the probe
/// shows `1` because it has to show *some* level and any of them would do.
pub fn preview_label(kind: &ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Probe => "1",
        ComponentKind::Constant => "0",
        _ => "",
    }
}

pub fn draw(
    painter: &Painter,
    kind: &ComponentKind,
    rect: Rect,
    orientation: Orientation,
    color: Color32,
    state: SymbolState<'_>,
    text_layer: &TextLayer,
) -> PinPositions {
    let stroke = Stroke::new(1.6, color);
    let label = state.label;
    match kind {
        ComponentKind::Button => {
            draw_button(painter, rect, orientation, stroke, color, state.pressed)
        }
        ComponentKind::Switch => {
            draw_switch(painter, rect, orientation, stroke, color, state.pressed)
        }
        ComponentKind::Led => draw_led(painter, rect, orientation, stroke, color),
        ComponentKind::NTransistor => draw_transistor(painter, rect, orientation, stroke, true),
        ComponentKind::PTransistor => draw_transistor(painter, rect, orientation, stroke, false),
        ComponentKind::Ground => draw_ground(painter, rect, orientation, stroke),
        ComponentKind::Power => draw_power(painter, rect, orientation, stroke, color),
        ComponentKind::Probe => {
            draw_probe(painter, rect, orientation, stroke, color, label, text_layer)
        }
        ComponentKind::Clock => draw_clock(painter, rect, orientation, stroke),
        ComponentKind::And => draw_and_gate(painter, rect, orientation, stroke, false),
        ComponentKind::Nand => draw_and_gate(painter, rect, orientation, stroke, true),
        ComponentKind::Or => draw_or_gate(painter, rect, orientation, stroke, false, false),
        ComponentKind::Nor => draw_or_gate(painter, rect, orientation, stroke, false, true),
        ComponentKind::Xor => draw_or_gate(painter, rect, orientation, stroke, true, false),
        ComponentKind::Xnor => draw_or_gate(painter, rect, orientation, stroke, true, true),
        ComponentKind::Buffer => draw_triangle_gate(painter, rect, orientation, stroke, false),
        ComponentKind::Not => draw_triangle_gate(painter, rect, orientation, stroke, true),
        ComponentKind::InputPort => draw_port(
            painter,
            rect,
            orientation,
            stroke,
            color,
            1,
            state,
            text_layer,
        ),
        ComponentKind::OutputPort => draw_port(
            painter,
            rect,
            orientation,
            stroke,
            color,
            -1,
            state,
            text_layer,
        ),
        ComponentKind::InOutPort => draw_port(
            painter,
            rect,
            orientation,
            stroke,
            color,
            0,
            state,
            text_layer,
        ),
        ComponentKind::TriStateSource => {
            draw_tri_state_source(painter, rect, orientation, stroke, color, state, text_layer)
        }
        ComponentKind::Constant => {
            draw_constant(painter, rect, orientation, stroke, color, state, text_layer)
        }
        ComponentKind::Splitter => {
            draw_splitter(painter, rect, orientation, stroke, color, state, text_layer)
        }
        ComponentKind::SrLatch => draw_sr_latch(painter, rect, orientation, stroke, text_layer),
        // A circuit instance draws its own generated box, not a fixed symbol.
        ComponentKind::Circuit(_) => PinPositions::default(),
        ComponentKind::TriStateBuffer => draw_tri_state_buffer(painter, rect, orientation, stroke),
        ComponentKind::BusTransceiver => {
            draw_bus_transceiver(painter, rect, orientation, stroke, false, text_layer)
        }
        ComponentKind::BusTransceiverOe => {
            draw_bus_transceiver(painter, rect, orientation, stroke, true, text_layer)
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
#[allow(clippy::too_many_arguments)]
fn draw_bus_transceiver(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    active_low: bool,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    text_layer.text(
        r(pos2(body.left() + 3.0, rect.top() + 7.0)),
        Align2::LEFT_CENTER,
        "DIR",
        9.0,
        color,
    );
    text_layer.text(
        r(pos2(body.left() + 3.0, rect.bottom() - 7.0)),
        Align2::LEFT_CENTER,
        if active_low { "OE" } else { "EN" },
        9.0,
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
    orientation: Orientation,
    stroke: Stroke,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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

/// A latching switch: a lever that stays where it was put.
///
/// Drawn as the schematic single-pole switch — a pivot, a lever and a
/// contact — rather than as a variant of the button's cap. The two do the
/// same thing electrically and differ only in whether the level springs
/// back, so the symbol is the only place that difference can be read before
/// you touch it: a lever resting on its contact says "closed and staying
/// closed" in a way a filled circle never could.
fn draw_switch(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    on: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);

    let pin = pos2(rect.right(), c.y);
    let pivot = pos2(c.x - rect.width() * 0.22, c.y);
    let contact = pos2(c.x + rect.width() * 0.1, c.y);

    painter.line_segment([r(contact), r(pin)], stroke);
    painter.circle_stroke(r(pivot), 2.5, stroke);
    painter.circle_stroke(r(contact), 2.5, stroke);

    // Closed, the lever lies on the contact; open, it lifts off it. The
    // gap is what carries the state, so it is drawn wide enough to read at
    // a glance rather than as a hairline.
    let far = if on {
        pos2(contact.x, contact.y)
    } else {
        pos2(contact.x - 2.0, contact.y - rect.height() * 0.32)
    };
    painter.line_segment([r(pivot), r(far)], stroke);

    draw_pin(painter, r(pin), color);

    PinPositions {
        inputs: vec![],
        outputs: vec![r(pin)],
    }
}

/// A three-position source: a change-over lever whose pole is the pin, one
/// throw on each rail, and a centre position touching neither.
///
/// The lever *is* the setting, the way a `Button`'s sunk cap is — up for
/// high, down for low, level for letting go. Which throw is which is said by
/// the rails themselves: a bar for the supply, a ground tick below. Marks
/// rather than letters, so the appearance convention holds.
///
/// The readout is a different fact and is kept: with the lever centred, what
/// the net carries is whatever *else* is driving it, which is precisely what
/// you place this component to find out.
#[allow(clippy::too_many_arguments)]
/// A constant: a tag carrying the value it puts on the wire, with a lead
/// out of its point.
///
/// **It is the one component that is nothing but its value**, the same
/// bounded exception the `Probe` already is: a symbol drawn without the
/// digits would say only "a constant", when the number is the whole of
/// what distinguishes one from another. The tag's point aims at the pin,
/// so which way the value leaves is read from the shape rather than from
/// an arrow.
fn draw_constant(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    state: SymbolState<'_>,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();

    let pin = pin_on_edge(rect, 0, orientation.rotation);
    // Fixed lengths from the right rather than fractions of the box: the
    // box grows with the value, and a fraction would stretch the point and
    // the lead along with it.
    let half = rect.height() * 0.3;
    let point = rect.right() - fixed(rect.width(), 0.18, GRID_SPACING * 0.7);
    let right = point - fixed(rect.width(), 0.12, 10.0);
    let left = rect.left() + fixed(rect.width(), 0.1, 8.0);

    // The point marks which way the value leaves, so it follows its pin
    // when the pin is to one side. On the quarter turns there is no side to
    // follow — a tag pointing downwards with its number written across it
    // would be a shape at odds with its own contents — so it stays as it is
    // and the lead simply leaves from the edge facing the pin.
    let mirrored = pin.x < c.x;
    let flip = |p: Pos2| {
        if mirrored {
            pos2(2.0 * c.x - p.x, p.y)
        } else {
            p
        }
    };

    let outline = [
        pos2(left, c.y - half),
        pos2(right, c.y - half),
        pos2(point, c.y),
        pos2(right, c.y + half),
        pos2(left, c.y + half),
        pos2(left, c.y - half),
    ];
    painter.line(outline.into_iter().map(flip).collect(), stroke);

    // From the edge facing the pin, never from the point: leaving from the
    // point regardless sent the lead diagonally across the tag on a quarter
    // turn, and straight through the body of it on a half.
    // `from_two_pos`, not `from_min_max`: mirroring swaps which corner is
    // which, and an inverted rect reports its sides the wrong way round.
    let tag = Rect::from_two_pos(flip(pos2(left, c.y - half)), flip(pos2(point, c.y + half)));
    painter.line_segment([lead_from(tag, pin), pin], stroke);

    text_layer.text(
        flip(pos2((left + right) * 0.5, c.y)),
        Align2::CENTER_CENTER,
        state.label,
        READOUT_SIZE,
        color,
    );

    draw_pin(painter, pin, color);
    PinPositions {
        inputs: vec![],
        outputs: vec![pin],
    }
}

/// A splitter: the bus on the left, a spine, and one lead per branch on the
/// right, each labelled with the bits it carries.
///
/// **Labelled, by the rule the SR latch and the transceiver already follow**:
/// text belongs on a symbol exactly when its pins are not interchangeable
/// and its shape does not say which is which. Nothing about a branch's
/// position says it carries bits 4 to 7 — and getting that wrong is a bug
/// you would go looking for in the logic.
///
/// The spine is drawn thick, because what it stands for is the bus itself.
fn draw_splitter(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    state: SymbolState<'_>,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
    // With no component behind it — the palette, the placement ghost — a
    // representative two-branch shape, the same idea as `preview_label`.
    let widths: Vec<usize> = if state.branches.is_empty() {
        vec![1, 1]
    } else {
        state.branches.to_vec()
    };

    let bus = pos2(rect.left(), c.y);
    let spine_x = c.x - rect.width() * 0.12;
    let branch_x = rect.right();

    let branch_y = |index: usize| splitter_row(c.y, index, widths.len());

    let spine = Stroke::new(stroke.width * 2.0, stroke.color);
    painter.line_segment([r(bus), r(pos2(spine_x, c.y))], spine);
    painter.line_segment(
        [
            r(pos2(spine_x, branch_y(0).min(c.y))),
            r(pos2(spine_x, branch_y(widths.len() - 1).max(c.y))),
        ],
        spine,
    );

    let mut outputs = Vec::with_capacity(widths.len());
    let mut bit = 0;
    for (index, width) in widths.iter().enumerate() {
        let y = branch_y(index);
        let pin = pos2(branch_x, y);
        painter.line_segment([r(pos2(spine_x, y)), r(pin)], stroke);
        text_layer.text(
            r(pos2(spine_x + 6.0, y - 7.0)),
            Align2::LEFT_CENTER,
            &bit_range(bit, *width),
            10.0,
            color,
        );
        bit += width;
        draw_pin(painter, r(pin), color);
        outputs.push(r(pin));
    }

    draw_pin(painter, r(bus), color);
    PinPositions {
        inputs: vec![r(bus)],
        outputs,
    }
}

/// Where branch `index` of `count` sits, vertically.
///
/// One grid row each, **every one on a grid dot**, and the block **centred
/// on the bus**. Those three cannot all hold with the rows packed together:
/// a dot every step means the bus lead is always level with *some* row, and
/// for an even count no row is in the middle — so a centred block puts them
/// all half a step off, and a packed one puts the bus on the second branch,
/// where its lead runs straight through that branch's own and reads as
/// joining the two.
///
/// So an even count **leaves the middle row empty**, and the bus arrives in
/// that gap. It costs one grid row of height, and it is the only
/// arrangement where the bus meets the spine squarely in the middle without
/// a branch lying under it.
fn splitter_row(centre_y: f32, index: usize, count: usize) -> f32 {
    let (index, count) = (index as i32, count as i32);
    let steps = if count % 2 == 1 {
        index - (count - 1) / 2
    } else {
        // Below the middle it counts up to -1, above it starts again at +1.
        let offset = index - count / 2;
        offset + i32::from(offset >= 0)
    };
    centre_y + GRID_SPACING * steps as f32
}

/// How a branch says which bits it carries: `3` for one, `4-7` for several.
///
/// Shared with the properties panel, which names the same branches — two
/// spellings of the same range is one that eventually disagrees.
pub fn bit_range(from: usize, width: usize) -> String {
    if width <= 1 {
        from.to_string()
    } else {
        format!("{}-{}", from, from + width - 1)
    }
}

fn draw_tri_state_source(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    state: SymbolState<'_>,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    // Upright whatever the rotation, like every symbol that is mostly a
    // readout: only the pin moves around it.
    let r = |p: Pos2| p;

    let pin = pin_on_edge(rect, 0, orientation.rotation);
    let pivot = pos2(c.x + rect.width() * 0.14, c.y);
    let throw = rect.height() * 0.3;
    let contact_x = c.x - rect.width() * 0.06;

    painter.line_segment([r(pivot), r(pin)], stroke);
    painter.circle_stroke(r(pivot), 2.5, stroke);

    // The supply, above; ground, below. Drawn as the two marks every
    // schematic uses, which is what lets the lever's direction be read
    // without writing "1" and "0" on it.
    let bar = 5.0;
    painter.line_segment(
        [
            r(pos2(contact_x - bar, c.y - throw - 4.0)),
            r(pos2(contact_x + bar, c.y - throw - 4.0)),
        ],
        stroke,
    );
    for (index, half) in [bar, bar * 0.6, bar * 0.25].into_iter().enumerate() {
        let y = c.y + throw + 4.0 + index as f32 * 2.5;
        painter.line_segment(
            [r(pos2(contact_x - half, y)), r(pos2(contact_x + half, y))],
            stroke,
        );
    }
    for side in [-1.0, 1.0] {
        let contact = pos2(contact_x, c.y + throw * side);
        painter.circle_stroke(r(contact), 2.0, stroke);
        painter.line_segment(
            [r(contact), r(pos2(contact_x, contact.y + 4.0 * side))],
            stroke,
        );
    }

    // Centred, the lever stops short of both contacts: the gap is the whole
    // of what "not driving" looks like, so it is drawn wide enough to read.
    // The lever is a one-bit affair, which a tri-state source is: any bit
    // set reads as thrown high.
    let tip = match state.level.unwrap_or_default() {
        PortDrive::Driving(bits) if bits != 0 => pos2(contact_x, c.y - throw),
        PortDrive::Driving(_) => pos2(contact_x, c.y + throw),
        PortDrive::Undriven => pos2(contact_x - 3.0, c.y),
    };
    painter.line_segment([r(pivot), r(tip)], stroke);

    // Aligned to the body, for the same reason as a port's: centred on a
    // point near the left edge, anything longer than a character hangs off
    // the side of the symbol.
    text_layer.text(
        pos2(rect.left() + 4.0, c.y),
        Align2::LEFT_CENTER,
        state.label,
        READOUT_SIZE,
        state.label_color.unwrap_or(color),
    );

    draw_pin(painter, r(pin), color);

    // The one point, in both lists — as `draw_port` does, and for the same
    // reason: there is a single place to connect to, and it is not worth a
    // caller having to know which side of the symbol calls it what.
    PinPositions {
        inputs: vec![r(pin)],
        outputs: vec![r(pin)],
    }
}

/// A circuit port: the boundary marker a parent will connect to.
///
/// `flow` is `1` for a value entering the circuit, `-1` for one leaving, and
/// `0` for both ways — drawn as the arrowhead's direction inside a plain
/// body. The arrow points *along the signal relative to the lead*, so the
/// shape reads without a label: heads towards the lead means it feeds the
/// circuit, away means it comes from it.
///
/// Every port also shows what its net resolves to, in the same one-letter
/// readout a `Probe` uses. A fill would have been enough for a switch with
/// two positions; a port has three it can be *put* in and five it can
/// *read*, and a letter says all of them.
#[allow(clippy::too_many_arguments)]
fn draw_port(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    flow: i32,
    state: SymbolState<'_>,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    // Upright whatever the rotation: only the pin moves around it.
    let r = |p: Pos2| p;

    let pin = pin_on_edge(rect, 0, orientation.rotation);
    // The lead and the inset are *capped* lengths rather than fractions:
    // the box grows with the readout, and a plain fraction would stretch
    // the lead along with it until the pin sat a long way from the body.
    // Capped rather than fixed because a palette icon is smaller than a
    // box, and fixed insets swallowed it whole.
    let inset = fixed(rect.width(), 0.14, 10.0);
    let lead = fixed(rect.width(), 0.24, GRID_SPACING);
    let body = Rect::from_min_max(
        pos2(rect.left() + inset, c.y - rect.height() * 0.3),
        pos2(rect.right() - lead, c.y + rect.height() * 0.3),
    );

    painter.line_segment([lead_from(body, pin), pin], stroke);
    let corners = [
        body.left_top(),
        body.right_top(),
        body.right_bottom(),
        body.left_bottom(),
        body.left_top(),
    ];
    painter.line(corners.into_iter().map(r).collect(), stroke);

    // Readout on the left, arrow on the right: the value changes constantly
    // and the arrow never does, so the eye should land on the value first.
    //
    // **Aligned to the body**, which it was not: centring it on a point 8
    // points inside the left edge put anything longer than a character or
    // two outside the body altogether, hanging off the side of the symbol.
    text_layer.text(
        pos2(body.left() + 5.0, c.y),
        Align2::LEFT_CENTER,
        state.label,
        READOUT_SIZE,
        state.label_color.unwrap_or(color),
    );

    // The arrow keeps its own length against the right edge rather than
    // spanning whatever is left, so it looks the same on a one-bit port and
    // on a thirty-two-bit one.
    let right = body.right() - fixed(rect.width(), 0.06, 5.0);
    let left = (right - fixed(rect.width(), 0.28, 22.0)).max(body.left() + 5.0);
    let head = 4.0;
    let arrow = |tip: f32, towards: f32| {
        for side in [-1.0, 1.0] {
            painter.line_segment(
                [
                    r(pos2(tip, c.y)),
                    r(pos2(tip - head * towards, c.y + head * side)),
                ],
                stroke,
            );
        }
    };
    painter.line_segment([r(pos2(left, c.y)), r(pos2(right, c.y))], stroke);
    if flow >= 0 {
        arrow(right, 1.0);
    }
    if flow <= 0 {
        arrow(left, -1.0);
    }

    draw_pin(painter, r(pin), color);

    PinPositions {
        inputs: vec![r(pin)],
        outputs: vec![r(pin)],
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
///
/// Both outputs are lettered `Q`, and the **bubble** on the lower one is
/// what says which is the complement — a mark rather than a glyph, so
/// nothing has to be in a font for it to appear.
fn draw_sr_latch(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    // The complement carries an inversion bubble, and that is what tells the
    // two outputs apart. It used to be said by an overline on the letter — a
    // combining macron, which egui's font does not draw, so both outputs
    // read `Q` and the symbol answered no question at all. The bubble is a
    // mark rather than a glyph, so nothing has to be in a font for it to
    // appear, and it is the mark every other inverted output here already
    // uses.
    let inverted = bubble_end(painter, pos2(body.right(), body.bottom()), r, stroke);
    painter.line_segment([r(q_bar), r(inverted)], stroke);

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
    let pad = 4.0;
    let label = |at: Pos2, align: Align2, text: &str| {
        text_layer.text(r(at), align, text, 9.0, color);
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
    // Plain `Q` on both, since the bubble is what says which is which — the
    // usual schematic reading, and one letter fewer to get wrong.
    label(
        pos2(body.right() - pad, body.bottom() - pad - 3.0),
        Align2::RIGHT_CENTER,
        "Q",
    );

    for pin in [set, reset, q, q_bar] {
        draw_pin(painter, r(pin), color);
    }

    PinPositions {
        inputs: vec![r(set), r(reset)],
        outputs: vec![r(q), r(q_bar)],
    }
}

/// The marquee tool's icon: a dashed rectangle, the mark every editor uses
/// for a selection sweep.
pub fn draw_marquee_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    let box_rect = rect.shrink(rect.width() * 0.12);
    let dash = rect.width() * 0.16;

    for (from, to) in [
        (box_rect.left_top(), box_rect.right_top()),
        (box_rect.right_top(), box_rect.right_bottom()),
        (box_rect.right_bottom(), box_rect.left_bottom()),
        (box_rect.left_bottom(), box_rect.left_top()),
    ] {
        let span = to - from;
        let length = span.length();
        let step = span / length;
        let mut travelled = 0.0;
        while travelled < length {
            let end = (travelled + dash).min(length);
            painter.line_segment([from + step * travelled, from + step * end], stroke);
            travelled = end + dash;
        }
    }
}

/// The pan tool's icon: arrows out of a centre, the universal "move the
/// view" mark. A hand would be the other convention; four arrows read at
/// 18 px, a hand doesn't.
/// One step forward: a triangle running into a bar, the mark every player
/// and debugger uses for "advance and stop again".
pub fn draw_step_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.3;

    painter.line(
        vec![
            pos2(c.x - reach, c.y - reach),
            pos2(c.x + reach * 0.35, c.y),
            pos2(c.x - reach, c.y + reach),
            pos2(c.x - reach, c.y - reach),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(c.x + reach * 0.75, c.y - reach),
            pos2(c.x + reach * 0.75, c.y + reach),
        ],
        stroke,
    );
}

/// Straight to the next thing that happens: two triangles into a bar, the
/// mark for "skip ahead" everywhere it is used.
pub fn draw_skip_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.3;

    for offset in [-reach, -reach * 0.2] {
        painter.line(
            vec![
                pos2(c.x + offset, c.y - reach),
                pos2(c.x + offset + reach * 0.8, c.y),
                pos2(c.x + offset, c.y + reach),
                pos2(c.x + offset, c.y - reach),
            ],
            stroke,
        );
    }
    painter.line_segment(
        [
            pos2(c.x + reach * 0.75, c.y - reach),
            pos2(c.x + reach * 0.75, c.y + reach),
        ],
        stroke,
    );
}

/// One clock edge: a square wave with the step ahead of it marked.
pub fn draw_edge_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.3;

    // Low, up, high, down, low — one whole beat, so the edge is the shape.
    painter.line(
        vec![
            pos2(c.x - reach, c.y + reach * 0.6),
            pos2(c.x - reach * 0.45, c.y + reach * 0.6),
            pos2(c.x - reach * 0.45, c.y - reach * 0.6),
            pos2(c.x + reach * 0.2, c.y - reach * 0.6),
            pos2(c.x + reach * 0.2, c.y + reach * 0.6),
            pos2(c.x + reach * 0.6, c.y + reach * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(c.x + reach * 0.9, c.y - reach),
            pos2(c.x + reach * 0.9, c.y + reach),
        ],
        stroke,
    );
}

/// Free-running: the same square wave as the edge tool, repeated and with
/// no bar to stop it.
pub fn draw_free_run_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.32;
    let high = c.y - reach * 0.55;
    let low = c.y + reach * 0.55;

    let mut points = vec![pos2(c.x - reach, low)];
    let step = reach * 0.5;
    for index in 0..4 {
        let x = c.x - reach + step * index as f32;
        let y = if index % 2 == 0 { high } else { low };
        points.push(pos2(x, y));
        points.push(pos2(x + step, y));
    }
    painter.line(points, stroke);
}

/// Run: the usual triangle.
pub fn draw_play_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.3;
    painter.line(
        vec![
            pos2(c.x - reach * 0.7, c.y - reach),
            pos2(c.x + reach * 0.8, c.y),
            pos2(c.x - reach * 0.7, c.y + reach),
            pos2(c.x - reach * 0.7, c.y - reach),
        ],
        stroke,
    );
}

/// Pause: the usual two bars.
pub fn draw_pause_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.3;
    for side in [-1.0, 1.0] {
        let x = c.x + reach * 0.4 * side;
        painter.line_segment([pos2(x, c.y - reach), pos2(x, c.y + reach)], stroke);
    }
}

pub fn draw_pan_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let reach = rect.width() * 0.34;
    let head = reach * 0.35;

    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let tip = pos2(c.x + reach * dx, c.y + reach * dy);
        painter.line_segment([c, tip], stroke);
        // The two barbs are the arrow direction turned a quarter each way.
        for side in [-1.0, 1.0] {
            let back = pos2(tip.x - head * dx, tip.y - head * dy);
            painter.line_segment(
                [
                    tip,
                    pos2(back.x - head * dy * side, back.y + head * dx * side),
                ],
                stroke,
            );
        }
    }
}

/// An instance of another circuit: a named box with a labelled pin per port.
///
/// Generated rather than drawn, so it exists the moment a circuit has ports
/// — the appearance editor on the roadmap will replace it. Inputs go down
/// the left and outputs down the right, in the order `flatten` sorted them
/// into, which is where the ports sit in the sub-circuit: move one up there
/// and its pin moves up here. Bidirectional ports join the inputs, since
/// they have to be on *a* side and the left is where a reader starts.
///
/// Labelled, by the same rule the latch and the transceiver follow: these
/// pins are not interchangeable and nothing about their position says which
/// is which.
#[allow(clippy::too_many_arguments)]
pub fn draw_instance(
    painter: &Painter,
    center: Pos2,
    orientation: Orientation,
    color: Color32,
    name: &str,
    ports: &[crate::placed_component::InstancePort],
    appearance: &crate::appearance::Appearance,
    text_layer: &TextLayer,
) -> PinPositions {
    // The circuit's own name sits above the symbol rather than inside it: it
    // belongs to the instance, not to the drawing, and a symbol you drew has
    // no reason to leave room for it — which is also why it can be turned
    // off, once the shape says what the name used to have to.
    if appearance.show_name {
        text_layer.text(
            orientation.place(appearance.name_anchor(center), center),
            Align2::CENTER_BOTTOM,
            name,
            10.0,
            color,
        );
    }

    let names: Vec<&str> = ports.iter().map(|port| port.name.as_str()).collect();
    let positions = appearance.draw(painter, center, orientation, color, &names, text_layer);

    // All in `inputs`, in port order: the caller maps pin *index* to anchor
    // pin, and splitting them by side here would only make it re-merge them.
    PinPositions {
        inputs: positions,
        outputs: vec![],
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
/// The drawing tools of the appearance editor.
///
/// Each icon *is* the shape it draws — a diagonal line, a rectangle, a
/// circle, an arc — rather than a picture standing for one. There is nothing
/// to abstract here: the tool and its result are the same object.
pub fn draw_line_shape_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let inner = rect.shrink(rect.width() * 0.16);
    painter.line_segment([inner.left_bottom(), inner.right_top()], stroke);
    for point in [inner.left_bottom(), inner.right_top()] {
        painter.circle_filled(point, 1.8, color);
    }
}

pub fn draw_rect_shape_tool(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_stroke(
        rect.shrink(rect.width() * 0.18),
        0.0,
        Stroke::new(1.6, color),
        egui::StrokeKind::Inside,
    );
}

pub fn draw_circle_shape_tool(painter: &Painter, rect: Rect, color: Color32) {
    painter.circle_stroke(rect.center(), rect.width() * 0.32, Stroke::new(1.6, color));
}

pub fn draw_arc_shape_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let r = rect.width() * 0.34;
    // The upper half only, so it reads as an arc rather than as a circle
    // that failed to close.
    let points: Vec<Pos2> = (0..=ARC_ICON_SAMPLES)
        .map(|step| {
            let angle = std::f32::consts::PI
                + std::f32::consts::PI * (step as f32 / ARC_ICON_SAMPLES as f32);
            pos2(c.x + r * angle.cos(), c.y + r * 0.5 + r * angle.sin())
        })
        .collect();
    painter.line(points.clone(), stroke);
    for point in [points[0], points[points.len() - 1]] {
        painter.circle_filled(point, 1.8, color);
    }
}

/// A capital "A", drawn rather than typed: these icons are painted into the
/// canvas layer, where a glyph would be resampled by the zoom.
pub fn draw_text_shape_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let inner = rect.shrink(rect.width() * 0.2);
    let apex = pos2(inner.center().x, inner.top());
    painter.line_segment([apex, inner.left_bottom()], stroke);
    painter.line_segment([apex, inner.right_bottom()], stroke);
    let bar = inner.top() + inner.height() * 0.62;
    painter.line_segment(
        [
            pos2(inner.left() + inner.width() * 0.2, bar),
            pos2(inner.right() - inner.width() * 0.2, bar),
        ],
        stroke,
    );
}

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

/// How a symbol is placed: turned, and possibly reflected.
///
/// A **mirror is not a rotation**, and no amount of turning stands in for
/// one. Four quarter-turns give four orientations, all of them preserving
/// the order of a symbol's pins; a splitter used as a merger wants to face
/// the other way *without* its branches ending up bottom to top, which is
/// exactly what a half turn does to them.
///
/// Mirrored first, in the symbol's own frame, then turned — so the two
/// compose the way a schematic reads them: "this symbol, reflected, placed
/// that way round".
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Orientation {
    pub rotation: Rotation,
    pub mirrored: bool,
}

impl Orientation {
    pub fn new(rotation: Rotation, mirrored: bool) -> Self {
        Self { rotation, mirrored }
    }

    /// Where `point` ends up, given `center` as the symbol's own origin.
    ///
    /// **Only geometry passes through here.** Text never does: glyphs are
    /// not drawn reversed, so a mirror moves *where* a label sits and
    /// leaves the label itself readable — the same rule the readouts follow
    /// under rotation.
    pub fn place(self, point: Pos2, center: Pos2) -> Pos2 {
        let point = if self.mirrored {
            pos2(2.0 * center.x - point.x, point.y)
        } else {
            point
        };
        rotate(point, center, self.rotation)
    }
}

/// Rotates `point` clockwise around `center` by `rotation`'s quarter-turns —
/// the same clockwise convention the old edge-based layout used (a point on
/// the left ends up on top after one quarter-turn, and so on), so a symbol's
/// canonical `Deg0` geometry keeps its intended orientation under rotation.
pub fn rotate(point: Pos2, center: Pos2, rotation: Rotation) -> Pos2 {
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

/// A rectangle turned about `center`, still axis-aligned.
///
/// Quarter turns only, so this is exact rather than an approximation of one:
/// turning two opposite corners and normalising is the whole of it.
pub fn rotate_rect(rect: egui::Rect, center: Pos2, rotation: Rotation) -> egui::Rect {
    egui::Rect::from_two_pos(
        rotate(rect.min, center, rotation),
        rotate(rect.max, center, rotation),
    )
}

pub fn draw_pin(painter: &Painter, point: Pos2, color: Color32) {
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
pub const BUBBLE_RADIUS: f32 = 3.5;

/// The inversion bubble on its own, for a symbol that places its own pins.
///
/// The same radius every gate in this file draws, so one you drew and one
/// we drew do not come out slightly different sizes on the same schematic.
pub fn draw_bubble(painter: &Painter, center: Pos2, stroke: Stroke) {
    painter.circle_stroke(center, BUBBLE_RADIUS, stroke);
}

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
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    pressed: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);

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
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    orientation: Orientation,
    stroke: Stroke,
    is_n_type: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
fn draw_ground(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);

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
    orientation: Orientation,
    stroke: Stroke,
    color: Color32,
    label: &str,
    text_layer: &TextLayer,
) -> PinPositions {
    let c = rect.center();
    let radius = rect.height() * 0.4;

    // A **stadium**, not a circle. A circle is only right while what is
    // inside it is one character; past that the value runs out of both
    // sides of it. Drawn as a rounded rectangle whose corners are half its
    // height, so at one character it *is* the circle it always was.
    let pin = pin_on_edge(rect, 2, orientation.rotation);
    let body = Rect::from_min_max(
        pos2(
            rect.left() + fixed(rect.width(), 0.25, GRID_SPACING),
            c.y - radius,
        ),
        pos2(rect.right() - fixed(rect.width(), 0.05, 4.0), c.y + radius),
    );
    painter.line_segment([pin, lead_from(body, pin)], stroke);
    // Rotating a rounded rectangle is not something a painter will do, so
    // the body is drawn from its own corners: a quarter turn maps the rect
    // onto another axis-aligned one, which is all four rotations here.
    painter.rect_stroke(body, radius, stroke, egui::StrokeKind::Middle);
    text_layer.text(
        body.center(),
        Align2::CENTER_CENTER,
        label,
        READOUT_SIZE,
        color,
    );

    draw_pin(painter, pin, color);
    PinPositions {
        inputs: vec![pin],
        outputs: vec![],
    }
}

/// A clock/oscillator, Logisim-style: a small chip-like box framing the
/// square-wave icon, with a lead reaching its one output pin.
fn draw_clock(
    painter: &Painter,
    rect: Rect,
    orientation: Orientation,
    stroke: Stroke,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    orientation: Orientation,
    stroke: Stroke,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    orientation: Orientation,
    stroke: Stroke,
    xor: bool,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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
    orientation: Orientation,
    stroke: Stroke,
    invert: bool,
) -> PinPositions {
    let c = rect.center();
    let r = |p: Pos2| orientation.place(p, c);
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

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::BOX_SIZE;

    #[test]
    fn turning_a_readout_moves_its_pin_around_an_upright_body() {
        // Romain's: at a quarter turn everything fell apart, because the
        // box turned with the symbol and left a wide value across a tall
        // narrow body. Text is never drawn rotated, so the body stays put
        // and the pin goes round it — which is the rule the generic box has
        // followed since the beginning.
        let rect = Rect::from_center_size(Pos2::ZERO, egui::vec2(160.0, 40.0));

        // A port's lead leaves rightwards at rest, and works round
        // clockwise: right, bottom, left, top.
        let corners = [
            (Rotation::Deg0, pos2(80.0, 0.0)),
            (Rotation::Deg90, pos2(0.0, 20.0)),
            (Rotation::Deg180, pos2(-80.0, 0.0)),
            (Rotation::Deg270, pos2(0.0, -20.0)),
        ];
        for (rotation, expected) in corners {
            assert_eq!(pin_on_edge(rect, 0, rotation), expected, "{rotation:?}");
            // And it is on the box's own edge, not somewhere a rotated
            // point of an unrotated box would have landed.
            assert!(rect.contains(pin_on_edge(rect, 0, rotation)));
        }

        // A probe's leaves the other way, and turns from there.
        assert_eq!(pin_on_edge(rect, 2, Rotation::Deg0), pos2(-80.0, 0.0));
        assert_eq!(pin_on_edge(rect, 2, Rotation::Deg90), pos2(0.0, -20.0));
    }

    #[test]
    fn a_lead_leaves_from_the_edge_facing_its_pin() {
        // Romain's constant: the lead left from the tag's point whatever
        // the rotation, so it ran diagonally across the tag on a quarter
        // turn and straight through the body of it on a half.
        let body = Rect::from_center_size(Pos2::ZERO, egui::vec2(80.0, 24.0));
        for (pin, expected) in [
            (pos2(100.0, 0.0), pos2(40.0, 0.0)),
            (pos2(-100.0, 0.0), pos2(-40.0, 0.0)),
            (pos2(0.0, 20.0), pos2(0.0, 12.0)),
            (pos2(0.0, -20.0), pos2(0.0, -12.0)),
        ] {
            let from = lead_from(body, pin);
            assert_eq!(from, expected, "for a pin at {pin:?}");
            // Never across the middle: a lead that starts on the far side
            // has to cross the symbol to reach its pin.
            assert!(
                (from - pin).length() < (body.center() - pin).length(),
                "the lead started further from the pin than the centre is"
            );
        }
    }

    #[test]
    fn a_splitter_lays_its_branches_out_on_the_grid_around_its_bus() {
        // Three properties that have to hold together, and the third is the
        // one Romain saw missing: every branch on a grid dot, in order, and
        // none of them level with the bus — where a lead through a branch's
        // own reads as joining the two rather than tapping the spine.
        for count in 1..=8 {
            let rows: Vec<f32> = (0..count).map(|i| splitter_row(0.0, i, count)).collect();

            for (index, row) in rows.iter().enumerate() {
                assert_eq!(
                    row % GRID_SPACING,
                    0.0,
                    "branch {index} of {count} at {row}"
                );
            }
            assert!(
                rows.windows(2).all(|pair| pair[1] > pair[0]),
                "branches of {count} are out of order: {rows:?}"
            );
            // An odd count has a middle row and the bus meets it there,
            // which is the classic drawing and is what it always did. An
            // even count has none, so the gap is where the bus goes.
            assert_eq!(
                rows.contains(&0.0),
                count % 2 == 1,
                "branches of {count}: {rows:?}"
            );
            // And centred on the bus, so it meets the spine in the middle.
            assert_eq!(
                rows[0] + rows[count - 1],
                0.0,
                "branches of {count} are not centred: {rows:?}"
            );
        }

        // A lone branch is the one that *does* sit on the bus: there is
        // nothing to be off-centre from, and a gap would be a splitter
        // splitting into nothing.
        assert_eq!(splitter_row(0.0, 0, 1), 0.0);
        // An odd count keeps the middle row it always had.
        assert_eq!(splitter_row(0.0, 1, 3), 0.0);
        // An even one leaves it empty, and straddles it.
        assert_eq!(splitter_row(0.0, 1, 4), -GRID_SPACING);
        assert_eq!(splitter_row(0.0, 2, 4), GRID_SPACING);
    }

    #[test]
    fn a_mirror_faces_the_other_way_without_reversing_the_pins() {
        // The whole reason a mirror exists: Romain's splitter used as a
        // merger has to face left with branch 0 still at the top, and a
        // half turn — the only way to face left before this — puts it at
        // the bottom instead.
        let c = Pos2::ZERO;
        let top = pos2(40.0, -20.0);

        let half_turn = Orientation::new(Rotation::Deg180, false);
        let mirror = Orientation::new(Rotation::Deg0, true);

        // Both send a pin on the right over to the left.
        assert_eq!(half_turn.place(top, c).x, -40.0);
        assert_eq!(mirror.place(top, c).x, -40.0);

        // Only the half turn takes it to the bottom with it.
        assert_eq!(half_turn.place(top, c).y, 20.0, "a half turn also flips");
        assert_eq!(mirror.place(top, c).y, -20.0, "a mirror leaves it on top");
    }

    #[test]
    fn mirroring_is_done_in_the_symbols_own_frame_then_turned() {
        // So the two compose the way a schematic reads them, rather than
        // the reflection axis following whatever the rotation happens to
        // be. Mirrored and turned a quarter, a point out to the right ends
        // up where the left-hand one would have.
        let c = Pos2::ZERO;
        let right = pos2(40.0, 0.0);
        let turned = Orientation::new(Rotation::Deg90, false);
        let both = Orientation::new(Rotation::Deg90, true);

        assert_eq!(both.place(right, c), turned.place(pos2(-40.0, 0.0), c));
        // And with nothing to reflect it is exactly the rotation it was.
        assert_eq!(
            Orientation::new(Rotation::Deg90, false).place(right, c),
            rotate(right, c, Rotation::Deg90)
        );
    }

    #[test]
    fn a_capped_length_is_fixed_on_a_box_and_proportional_below_one() {
        // The second half of a fault Romain saw: fixed insets made a symbol
        // hold a wide readout and swallowed the palette icon, which is a
        // good deal smaller than a box.
        let icon = 28.0;
        assert!(
            fixed(icon, 0.24, GRID_SPACING) < icon * 0.5,
            "an icon must keep most of its width for the symbol"
        );
        // At an ordinary box the cap does not bite at all, which is the
        // point: everything already drawn is drawn exactly as it was.
        assert_eq!(
            fixed(BOX_SIZE.x, 0.24, GRID_SPACING),
            BOX_SIZE.x * 0.24,
            "a plain box keeps the proportions it always had"
        );
        // It bites past that, so a 32-bit port's lead is not four times a
        // one-bit port's.
        assert_eq!(fixed(BOX_SIZE.x * 8.0, 0.24, GRID_SPACING), GRID_SPACING);
    }
}
