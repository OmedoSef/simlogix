//! Per-component properties, and the panel that edits them.
//!
//! Every property is optional, and an absent one means "behave exactly as
//! this component always has". That's what lets a property be added without
//! inventing a default for the components already drawn — and it's why none
//! of them are written to a project file unless they've been set.
//!
//! Which properties a component *has* depends on its kind; the panel only
//! offers the ones that apply. The struct holds them all flat rather than in
//! a per-kind enum, because a property tends to start on one kind and turn
//! out to make sense on several.

use egui::{Response, RichText, Ui};
use serde::{Deserialize, Serialize};

use simlogix_core::{all_ones, PortDrive, PortSetting};

use crate::appearance::{Appearance, Facing, PinSlot, Shape, TextAlign};
use crate::canvas;
use crate::i18n::Strings;
use crate::palette::ComponentKind;
use crate::placed_component::InstancePort;

/// What the user has set on one component.
/// Which base a value is shown in.
///
/// `Auto` is a *choice not made* rather than a fourth base: it resolves to
/// `0`/`1` on a plain wire and hexadecimal on a bus, which is how a bit
/// pattern is read. Kept as a variant of its own so "I never chose" and "I
/// chose hexadecimal" stay different answers — the same reasoning the
/// language setting already follows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberBase {
    #[default]
    Auto,
    Binary,
    Decimal,
    Hexadecimal,
}

impl NumberBase {
    /// What `Auto` means for a signal this wide.
    fn resolve(self, width: usize) -> Self {
        match self {
            Self::Auto if width <= 1 => Self::Decimal,
            Self::Auto => Self::Hexadecimal,
            chosen => chosen,
        }
    }

    /// How many characters the widest value of `width` bits takes.
    ///
    /// From the width, never from the value — a symbol is sized by this, and
    /// one that resized as the simulation ran would move its own pins under
    /// the wires.
    pub fn digits(self, width: usize) -> usize {
        match self.resolve(width) {
            Self::Binary => width.max(1),
            Self::Hexadecimal => width.div_ceil(4).max(1),
            // The decimal digits of 2^width - 1, without building it:
            // log10(2) is a little over 0.30103.
            _ => ((width as f64) * std::f64::consts::LOG10_2).floor() as usize + 1,
        }
    }

    /// What a value written in this base is prefixed with, so that what is
    /// on screen can be typed straight back in.
    fn prefix(self) -> &'static str {
        match self {
            Self::Binary => "0b",
            Self::Hexadecimal => "0x",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Properties {
    /// A label of the user's own, drawn under the symbol.
    ///
    /// This is a deliberate exception to the appearance convention (symbols
    /// carry no text, not even pin names): an annotation you wrote is not
    /// the same thing as a label the editor generated for you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// `Button` only — whether it rests pressed, so that clicking releases
    /// it instead of pressing it. The normally-closed switch.
    ///
    /// This is the *resting* state, not the current one: runtime state is
    /// still never saved, and a loaded project starts from this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressed: Option<bool>,

    /// `Led` only — what colour it glows when lit. Unset means the red a
    /// LED is by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,

    /// Driving ports only — whether clicking can put the port in its
    /// undriven position, and therefore whether the interface admits a
    /// third state at all. Unset means two-state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tri_state: Option<bool>,

    /// Driving ports only — where the port rests when the circuit is
    /// loaded. Like a button's `pressed`, this is the *resting* value, not
    /// the current one: runtime state is still never saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<PortSetting>,

    /// How many bits this component's pins carry. Unset means one — a plain
    /// wire, which is what everything drawn before buses existed is.
    ///
    /// Set on the *component*, never on a wire: a wire takes its width from
    /// what it joins, so asking for it there would be asking twice and
    /// letting the two disagree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<usize>,

    /// `DFlipFlop` only — whether it has asynchronous set and reset pins.
    /// Unset means it has not, which is what every flip-flop drawn before
    /// this existed has.
    ///
    /// Opt-in rather than always present, and that is what settles what an
    /// undriven one means: there isn't one. Present, they follow the rule
    /// every control pin follows — undriven is `Unknown`, not "not
    /// asserted" — and that costs nothing precisely because you add them
    /// when you mean to wire them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_set_reset: Option<bool>,

    /// Which base this component shows its value in.
    ///
    /// Unset follows the setting, which itself defaults to `Auto` — so "I
    /// never chose" reaches all the way down, and changing the global
    /// default still moves everything that never asked for something else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<NumberBase>,

    /// A `Splitter`'s branch widths, from bit 0 upward.
    ///
    /// Unset means one branch per bit, which is the plain meaning of
    /// splitting a bus and what a fresh one should do without being told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<usize>>,

    /// What a `Constant` puts on its wire. Unset means zero.
    ///
    /// A *setting*, unlike the value a port is driving right now: it is
    /// what the component is, so it is saved, undone and redone like any
    /// other property. The digits are the same; the natures are not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
}

impl Properties {
    /// Whether nothing has been set, so the whole thing can be left out of
    /// the file rather than written as a row of nulls.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether this port's click cycle includes the undriven position.
    /// Whether clicking this component offers the undriven position.
    ///
    /// A tri-state source always does — being able to let go is the whole of
    /// what it is, and a two-state one would just be a `Switch`. A port
    /// *declares* it, because there the number of states is a promise made
    /// to whatever will drive the pin from outside.
    pub fn cycles_undriven(&self, kind: &ComponentKind) -> bool {
        *kind == ComponentKind::TriStateSource || self.is_tri_state()
    }

    /// How many bits this component's pins carry. One unless it says wider.
    pub fn width(&self) -> usize {
        self.width.unwrap_or(1).max(1)
    }

    /// Whether this component has asynchronous set and reset pins.
    ///
    /// **A T flip-flop always does, and it is not a choice.** It has no data
    /// path — it only ever transforms what it already holds — and it starts
    /// holding nothing, so `Unknown` toggled is still `Unknown`: without a
    /// way to force a definite value in, one could never leave the unknown
    /// state at all. Offering a setting whose other position is a component
    /// that can do nothing is worse than not offering it.
    ///
    /// A D flip-flop and a D latch are the other way round: `D` puts a value
    /// in, so they work perfectly well without, and the pins stay opt-in —
    /// which is what keeps "an undriven control is `Unknown`" from costing
    /// two ground rails on every one you draw.
    pub fn async_set_reset(&self, kind: &ComponentKind) -> bool {
        matches!(
            kind,
            ComponentKind::TFlipFlop | ComponentKind::TFlipFlopFalling
        ) || self.async_set_reset.unwrap_or(false)
    }

    /// What a constant drives. Zero unless it says otherwise — the value a
    /// wire sits at when nothing has been typed.
    pub fn constant_value(&self) -> u64 {
        self.value.unwrap_or(0) & all_ones(self.width())
    }

    /// A splitter's branch widths, from bit 0 upward — one branch per bit
    /// unless it says otherwise.
    ///
    /// Never empty: a splitter with no branches is a component with one pin,
    /// which is a wire that took a tick.
    pub fn branch_widths(&self) -> Vec<usize> {
        match &self.branches {
            Some(branches) if !branches.is_empty() => {
                branches.iter().map(|width| (*width).max(1)).collect()
            }
            _ => vec![1; self.width()],
        }
    }

    /// Whether this kind shows a value, and so can be told which base to
    /// show it in.
    ///
    /// Everything that puts a number on screen. A LED and a button show a
    /// state rather than a value, and have nothing to choose.
    pub fn has_base(kind: &ComponentKind) -> bool {
        matches!(
            kind,
            ComponentKind::Probe
                | ComponentKind::InputPort
                | ComponentKind::OutputPort
                | ComponentKind::InOutPort
                | ComponentKind::TriStateSource
                | ComponentKind::Constant
        )
    }

    /// Whether this kind can be told how wide its pins are.
    ///
    /// The ports, because a boundary has to declare what it carries, and the
    /// plain gates, because a gate on a bus is that gate applied bit by bit
    /// and every one of its pins is that same width.
    ///
    /// A component whose pins are *not* all alike is offered it too — a
    /// tri-state buffer, a transceiver — because
    /// [`crate::placed_component::PlacedComponent::pin_width`] answers per
    /// pin, so the enable and the direction stay one bit while the data
    /// widens.
    ///
    /// The SR latch is still absent: a wide one is a register, and what
    /// `S` and `R` would mean for it is a design question rather than a
    /// width.
    pub fn has_width(kind: &ComponentKind) -> bool {
        matches!(
            kind,
            ComponentKind::InputPort
                | ComponentKind::OutputPort
                | ComponentKind::InOutPort
                | ComponentKind::And
                | ComponentKind::Or
                | ComponentKind::Nand
                | ComponentKind::Nor
                | ComponentKind::Xor
                | ComponentKind::Xnor
                | ComponentKind::Not
                | ComponentKind::Buffer
                | ComponentKind::Constant
                // Their data pins are as wide as they are told; the enable
                // and the direction stay one bit, which `pin_width` says.
                | ComponentKind::TriStateBuffer
                | ComponentKind::BusTransceiver
                | ComponentKind::BusTransceiverOe
                // Its bus pin. Its branches carry their own widths, which
                // are the other half of the same setting.
                | ComponentKind::Splitter
                // All four pins alike: on a bus it is one latch per bit, so
                // set, reset and both outputs are the same width.
                | ComponentKind::SrLatch
                // `D`, `Q` and `Q̄` widen together; the clock and the
                // asynchronous inputs stay one bit, which `pin_width` says.
                | ComponentKind::DFlipFlop
                | ComponentKind::DFlipFlopFalling
                | ComponentKind::DLatch
                | ComponentKind::TFlipFlop
                | ComponentKind::TFlipFlopFalling
        )
    }

    pub fn is_tri_state(&self) -> bool {
        self.tri_state.unwrap_or(false)
    }

    /// Where a driving port rests. Undriven unless told otherwise — the
    /// honest starting point, since nothing has said what it carries yet.
    pub fn initial_level(&self) -> PortSetting {
        self.initial.unwrap_or_default()
    }

    /// The name to draw under the symbol, if there is one worth drawing.
    /// A name of nothing but spaces counts as none.
    pub fn label(&self) -> Option<&str> {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
    }
}

/// Component kinds that come in two flavours differing only in a setting —
/// the channel of a transistor, the polarity of a transceiver's enable.
///
/// Which flavour a component is lives in its `ComponentKind`, not in
/// [`Properties`]: an NMOS and a PMOS really are two components rather than
/// one with a switch, the symbol is already drawn from the kind, and a saved
/// `simlogix:PTransistor` says more than a `Transistor` with a flag beside
/// it. All the panel adds is the ability to change your mind after placing.
const VARIANTS: [[ComponentKind; 2]; 4] = [
    [ComponentKind::NTransistor, ComponentKind::PTransistor],
    [
        ComponentKind::BusTransceiver,
        ComponentKind::BusTransceiverOe,
    ],
    [ComponentKind::DFlipFlop, ComponentKind::DFlipFlopFalling],
    [ComponentKind::TFlipFlop, ComponentKind::TFlipFlopFalling],
];

/// Bounds on a label's size: small enough to annotate, large enough to
/// title a symbol, and never zero — which would be a shape you can't see and
/// can't get back.
const MIN_TEXT_SIZE: f32 = 5.0;
const MAX_TEXT_SIZE: f32 = 32.0;

/// A lead longer than this stops being part of the symbol and starts being a
/// wire the user didn't draw.
const MAX_PIN_LEAD: f32 = 40.0;

/// The widest bus offered. Not a limit the engine has — a signal is a list
/// and could be any length — but a number past which a schematic is not
/// what you want, and a spinner with no ceiling invites a typo that turns
/// into a million-entry vector.
const MAX_WIDTH: usize = 64;

fn siblings(kind: &ComponentKind) -> Option<[ComponentKind; 2]> {
    VARIANTS.into_iter().find(|pair| pair.contains(kind))
}

/// Whether reflecting this kind means anything.
///
/// **Not** the symbols that keep their body upright: a port, a probe, a
/// constant or a source has one pin, which already goes round the box by
/// rotation, so a mirror there would be a control that does nothing. Every
/// other symbol has a direction a schematic can want the other way round.
pub fn has_mirror(kind: &ComponentKind) -> bool {
    !crate::symbol::keeps_upright(kind)
}

/// What the panel wants done, beyond the properties it edited in place.
#[derive(Default)]
pub struct PanelResult {
    /// This frame begins an editing session — the moment to snapshot for
    /// undo, rather than every frame a value changes.
    pub edit_started: bool,
    /// Turn this component into its sibling kind.
    pub change_kind: Option<ComponentKind>,
    /// Reflect it, or stop. Outside `Properties` because a mirror is
    /// placement, like the rotation it sits beside — not something the
    /// component is *set* to.
    pub mirrored: Option<bool>,
}

/// The **value** panel: what a port is driving right now.
///
/// A section of its own, below the properties and separated from them,
/// because there are two things here and confusing them would be easy. The
/// value is **runtime state** — no undo step, never saved — while *resting
/// value* above it is a property and *is* saved. The same digits, two
/// natures; one field for both would have to lie about one of them.
///
/// Returns the new drive when it changed. Reading is not editing, so
/// nothing here reports an edit for undo to snapshot.
pub fn show_value(
    ui: &mut Ui,
    strings: &Strings,
    drive: PortDrive,
    width: usize,
    base: NumberBase,
) -> Option<PortDrive> {
    let mut edited = None;

    ui.add_space(12.0);
    ui.separator();
    ui.label(RichText::new(strings.value_heading).strong());
    ui.label(RichText::new(strings.value_runtime).weak());
    ui.add_space(4.0);

    let mut driving = drive != PortDrive::Undriven;
    if ui.checkbox(&mut driving, strings.value_driving).changed() {
        edited = Some(if driving {
            PortDrive::Driving(0)
        } else {
            PortDrive::Undriven
        });
    }

    if let PortDrive::Driving(bits) = drive {
        if let (Some(value), _) = value_field(ui, "port_value", bits, width, base) {
            edited = Some(PortDrive::Driving(value));
        }
        ui.label(RichText::new(strings.value_bases).weak());
    }

    edited
}

/// The field a value is typed into: shown by [`format_value`], read by
/// `parse_value`, and masked to `width` so what is typed can never drive
/// bits that do not exist.
///
/// Shared by the value panel and by a constant's property, which ask the
/// identical question — building it twice is how the two answers would come
/// to differ. The response comes back too, so a caller can tell an editing
/// session beginning from a redraw.
fn value_field(
    ui: &mut Ui,
    salt: &str,
    bits: u64,
    width: usize,
    base: NumberBase,
) -> (Option<u64>, Response) {
    let id = ui.id().with(salt);
    let mut text = ui
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| format_value(u128::from(bits), width, base, true));
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
    if response.has_focus() {
        // Its own text while being typed, so a half-written number is not
        // rewritten under the caret.
        ui.data_mut(|data| data.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|data| data.remove_temp::<String>(id));
    }
    let edited = response
        .changed()
        .then(|| parse_value(&text).map(|parsed| parsed & all_ones(width)))
        .flatten();
    (edited, response)
}

/// How a value is written for a reader.
///
/// **One rule in one place**, read by a symbol's readout, by the value
/// panel and by a constant's field. There used to be two — a bare hex form
/// for the readouts and a hex-or-decimal one for the typed fields — so a
/// constant showed `10` where a probe on the same value showed `A`. Which
/// base is a *choice*; whether it is prefixed is a matter of where it is
/// written, and only that second part legitimately differs.
///
/// `prefix` is for a field you type into, where `0xC` has to read back; a
/// symbol takes the bare digits, since a prefix on a schematic is noise.
/// Neither says anything about what may be *typed* — `parse_value` takes
/// all three bases whatever this shows.
///
/// **Not padded**, deliberately, and only for now. Padding to
/// [`NumberBase::digits`] would keep a readout the same size as its value
/// changes, which is worth having — but only once a symbol is *sized* from
/// that number. Until then it would turn `FF` into `000000FF` on a 32-bit
/// port and push the overflow further out, which is the complaint it is
/// meant to help with. The two land together.
pub fn format_value(bits: u128, width: usize, base: NumberBase, prefix: bool) -> String {
    let base = base.resolve(width);
    let digits = match base {
        NumberBase::Binary => format!("{bits:b}"),
        NumberBase::Hexadecimal => format!("{bits:X}"),
        _ => bits.to_string(),
    };
    format!("{}{digits}", if prefix { base.prefix() } else { "" })
}

/// The **value** panel for a switch: where it is right now.
///
/// A section of its own beside a port's, and for the same reason — this is
/// runtime state, so it takes no undo step and is never saved, while
/// *closed at rest* above it is a property and is. The same position, two
/// natures; one control for both would have to lie about one of them.
///
/// Returns the new position when it changed.
pub fn show_switch_value(ui: &mut Ui, strings: &Strings, closed: bool) -> Option<bool> {
    ui.add_space(12.0);
    ui.separator();
    ui.label(RichText::new(strings.value_heading).strong());
    ui.label(RichText::new(strings.value_runtime).weak());
    ui.add_space(4.0);

    let mut now = closed;
    ui.checkbox(&mut now, strings.value_switch_closed)
        .changed()
        .then_some(now)
}

/// Reads a value written in hex, binary or decimal.
///
/// All three because a value copied from a datasheet, from code, or from a
/// probe arrives in whichever the source used, and retyping it in another
/// base is exactly the sort of arithmetic a tool should do for you.
fn parse_value(text: &str) -> Option<u64> {
    let text = text.trim().replace('_', "");
    let lower = text.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("0x") {
        return u64::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = lower.strip_prefix("0b") {
        return u64::from_str_radix(rest, 2).ok();
    }
    text.parse().ok()
}

/// The swatches offered wherever a colour is chosen.
///
/// A fixed set rather than a wheel, because the thing colour is *for* here
/// is telling two wires apart at a crossing — which needs a handful of hues
/// that are obviously different, not sixteen million that mostly aren't. It
/// also makes a colour reusable: the same swatch is the same bytes, where
/// two trips through a wheel never land on quite the same place.
///
/// Chosen to stay legible on either theme, so nothing is very dark or very
/// pale.
const SWATCHES: [[u8; 3]; 12] = [
    [0xE0, 0x3B, 0x3B], // red
    [0xE8, 0x7A, 0x22], // orange
    [0xE0, 0xB0, 0x21], // amber
    [0x8B, 0xC3, 0x2E], // lime
    [0x35, 0xA8, 0x53], // green
    [0x1F, 0xA8, 0x9E], // teal
    [0x2E, 0x9B, 0xD8], // sky
    [0x3B, 0x62, 0xD0], // blue
    [0x7A, 0x4F, 0xD0], // violet
    [0xC0, 0x48, 0xC0], // magenta
    [0xD0, 0x5A, 0x8A], // pink
    [0x8A, 0x6F, 0x5A], // brown
];

/// Side of one swatch.
const SWATCH_SIZE: f32 = 20.0;

fn to_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// Reads `#RRGGBB`, or `RRGGBB` — a code pasted from somewhere else arrives
/// spelled either way, and refusing one of them would be pedantry.
fn from_hex(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A colour control: the swatches, the hex code, and the full picker behind
/// a button for anything the swatches don't cover.
///
/// Returns `true` when the colour changed this frame.
fn color_control(ui: &mut Ui, strings: &Strings, color: &mut [u8; 3]) -> bool {
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for swatch in SWATCHES {
            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(SWATCH_SIZE), egui::Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if ui.is_rect_visible(rect) {
                let fill = egui::Color32::from_rgb(swatch[0], swatch[1], swatch[2]);
                ui.painter().rect_filled(rect, 3.0, fill);
                // The one in use is ringed rather than ticked: a tick would
                // have to be drawn in a colour that reads against every
                // swatch, and there isn't one.
                let stroke = if *color == swatch {
                    egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                } else {
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color())
                };
                ui.painter()
                    .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Outside);
            }
            if response.on_hover_text(to_hex(swatch)).clicked() {
                *color = swatch;
                changed = true;
            }
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // The field keeps its own text while it is being typed into, so a
        // half-finished code like "#1a" survives the frames before it parses.
        // Out of focus it is rewritten from the colour, which is what keeps
        // it honest when a swatch or the wheel changes it instead.
        let id = ui.id().with("hex");
        let mut text = ui
            .data_mut(|data| data.get_temp::<String>(id))
            .unwrap_or_else(|| to_hex(*color));

        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(74.0)
                .font(egui::TextStyle::Monospace),
        );
        if field.has_focus() {
            if let Some(parsed) = from_hex(&text) {
                if parsed != *color {
                    *color = parsed;
                    changed = true;
                }
            }
        } else {
            text = to_hex(*color);
        }
        ui.data_mut(|data| data.insert_temp(id, text));

        if ui.color_edit_button_srgb(color).changed() {
            changed = true;
        }
        ui.label(egui::RichText::new(strings.property_color_more).weak());
    });

    changed
}

/// Draws the panel for the symbol as a whole — what there is to set when
/// nothing in particular is picked.
pub fn show_symbol(ui: &mut Ui, strings: &Strings, appearance: &mut Appearance) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(strings.symbol_selected);
    ui.add_space(8.0);

    if ui
        .checkbox(&mut appearance.show_name, strings.symbol_show_name)
        .changed()
    {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    ui.label(RichText::new(strings.shape_none_selected).weak());

    result
}

/// Draws the panel for a selected pin of a symbol.
///
/// The direction is **set here, not worked out from where the pin sits.** It
/// used to be taken from the nearest edge of the line art on every drop,
/// which reads well on a rectangle and fights you on anything else — a pin
/// beside a curve, or one deliberately pointing across the body, kept being
/// turned back. Romain's call after using it, and the right one: guessing is
/// only worth it when it is right nearly always.
pub fn show_pin(
    ui: &mut Ui,
    strings: &Strings,
    pin: &mut PinSlot,
    port: Option<&InstancePort>,
) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    // Which pin this is, said by name rather than left to be worked out from
    // its position — that is the whole reason a port has a name, and on a
    // symbol with four identical-looking pins it is the only way to tell.
    match port
        .map(|port| port.name.as_str())
        .filter(|name| !name.is_empty())
    {
        Some(name) => ui.label(format!("{} — {name}", strings.pin_selected)),
        None => ui.label(format!(
            "{} — {}",
            strings.pin_selected, strings.pin_unnamed
        )),
    };
    if let Some(port) = port {
        ui.label(RichText::new(port_kind_label(strings, &port.kind)).weak());
    }
    ui.add_space(8.0);

    ui.label(strings.pin_facing);
    ui.horizontal_wrapped(|ui| {
        for (facing, label) in [
            (Facing::Left, strings.pin_facing_left),
            (Facing::Right, strings.pin_facing_right),
            (Facing::Up, strings.pin_facing_up),
            (Facing::Down, strings.pin_facing_down),
        ] {
            if ui.selectable_label(pin.facing == facing, label).clicked() {
                result.edit_started = true;
                pin.facing = facing;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(strings.shape_position);
    // A pin has to land on a grid dot, so its fields step by a whole one —
    // unlike a shape's, which are free. Typing an off-grid value is still
    // possible and is the user's business; the step is what a drag gives.
    result.edit_started |= point_row(ui, &mut pin.at, canvas::GRID_SPACING);

    ui.add_space(8.0);
    ui.label(strings.pin_lead);
    let response = ui.add(egui::Slider::new(&mut pin.lead, 0.0..=MAX_PIN_LEAD));
    if response.drag_started() || response.gained_focus() {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    // Beside the lead, since that is what it changes: the bubble takes the
    // last of it, against the body.
    if ui
        .checkbox(&mut pin.inverted, strings.pin_inverted)
        .on_hover_text(strings.pin_inverted_hint)
        .changed()
    {
        result.edit_started = true;
    }

    ui.add_space(8.0);
    if ui
        .checkbox(&mut pin.show_name, strings.pin_show_name)
        .changed()
    {
        result.edit_started = true;
    }

    // Only worth offering while there is a name on screen to move.
    if pin.show_name {
        ui.add_space(8.0);
        ui.label(strings.pin_name_offset);
        // On a shape's step rather than a pin's: this moves a label clear of
        // the line art, and a nudge of a whole grid space is not a nudge.
        result.edit_started |= point_row(ui, &mut pin.name_offset, crate::appearance::SHAPE_SNAP);
        ui.horizontal(|ui| {
            if pin.name_offset != (0.0, 0.0) && ui.button(strings.property_reset).clicked() {
                result.edit_started = true;
                pin.name_offset = (0.0, 0.0);
            }
        });
    }

    result
}

fn port_kind_label(strings: &Strings, kind: &ComponentKind) -> &'static str {
    match kind {
        ComponentKind::OutputPort => strings.component_output_port,
        ComponentKind::InOutPort => strings.component_inout_port,
        _ => strings.component_input_port,
    }
}

/// One `X` / `Y` pair. Reports whether an editing session began this frame.
fn point_row(ui: &mut Ui, point: &mut (f32, f32), step: f32) -> bool {
    let mut started = false;
    ui.horizontal(|ui| {
        for (axis, value) in [("X", &mut point.0), ("Y", &mut point.1)] {
            ui.label(axis);
            let response = ui.add(
                egui::DragValue::new(value)
                    .speed(step / 4.0)
                    .fixed_decimals(1),
            );
            if response.drag_started() || response.gained_focus() {
                started = true;
            }
        }
    });
    started
}

/// Draws the panel for a selected shape of a symbol.
///
/// Every shape shows the points it is made of, editable by hand. Dragging is
/// how you sketch one; typing is how you make it exact — and the drawing step
/// deliberately doesn't apply here, so a curve can sit off the grid when
/// that is what it takes to look right.
pub fn show_shape(ui: &mut Ui, strings: &Strings, shape: &mut Shape) -> PanelResult {
    let mut result = PanelResult::default();

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(shape_kind_label(strings, shape));
    ui.add_space(8.0);

    let step = crate::appearance::SHAPE_SNAP;
    match shape {
        Shape::Polyline { points, closed } => {
            if ui.checkbox(closed, strings.shape_closed).changed() {
                result.edit_started = true;
            }
            ui.add_space(8.0);
            ui.label(strings.shape_points);
            for (index, point) in points.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", index + 1)).weak());
                    result.edit_started |= point_row(ui, point, step);
                });
            }
        }
        Shape::Circle { center, radius } => {
            ui.label(strings.shape_center);
            result.edit_started |= point_row(ui, center, step);

            ui.add_space(8.0);
            ui.label(strings.shape_radius);
            let response = ui.add(
                egui::DragValue::new(radius)
                    .speed(step / 4.0)
                    .fixed_decimals(1)
                    // Never zero: a circle you can't see is one you can't
                    // select either, and there'd be no way back to it.
                    .range(0.5..=f32::INFINITY),
            );
            if response.drag_started() || response.gained_focus() {
                result.edit_started = true;
            }
        }
        Shape::Arc { start, mid, end } => {
            for (label, point) in [
                (strings.shape_arc_start, start),
                (strings.shape_arc_mid, mid),
                (strings.shape_arc_end, end),
            ] {
                ui.label(label);
                result.edit_started |= point_row(ui, point, step);
            }
        }
        Shape::Text {
            at,
            align,
            size,
            text,
        } => {
            ui.label(strings.shape_text_content);
            // The snapshot is taken when the session *begins*, so a typed
            // label is one undo step rather than one per keystroke — the same
            // rule the component name field follows.
            if ui.text_edit_singleline(text).gained_focus() {
                result.edit_started = true;
            }

            ui.add_space(8.0);
            ui.label(strings.shape_position);
            result.edit_started |= point_row(ui, at, step);

            ui.add_space(8.0);
            ui.label(strings.shape_text_size);
            let response = ui.add(egui::Slider::new(size, MIN_TEXT_SIZE..=MAX_TEXT_SIZE));
            if response.drag_started() || response.gained_focus() {
                result.edit_started = true;
            }

            ui.add_space(8.0);
            ui.label(strings.shape_text_align);
            ui.horizontal_wrapped(|ui| {
                for (option, label) in [
                    (TextAlign::Left, strings.pin_facing_left),
                    (TextAlign::Center, strings.shape_align_center),
                    (TextAlign::Right, strings.pin_facing_right),
                ] {
                    if ui.selectable_label(*align == option, label).clicked() {
                        result.edit_started = true;
                        *align = option;
                    }
                }
            });
        }
    }

    result
}

fn shape_kind_label(strings: &Strings, shape: &Shape) -> &'static str {
    match shape {
        Shape::Polyline { closed: true, .. } => strings.shape_rect,
        Shape::Polyline { .. } => strings.shape_line,
        Shape::Circle { .. } => strings.shape_circle,
        Shape::Arc { .. } => strings.shape_arc,
        Shape::Text { .. } => strings.shape_text,
    }
}

/// Draws the panel for the selected component and applies what's edited.
pub fn show(
    ui: &mut Ui,
    strings: &Strings,
    kind: &ComponentKind,
    properties: &mut Properties,
    mirrored: bool,
    default_base: NumberBase,
) -> PanelResult {
    let mut result = PanelResult::default();
    let mut edit_started = false;
    // What this component shows values in: its own choice, or the setting
    // it is following.
    let base = properties.base.unwrap_or(default_base);

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);

    match siblings(kind) {
        // A component with a sibling shows the choice instead of a plain
        // name, so the name is still there but now says which one it is.
        Some(pair) => {
            ui.label(strings.property_variant);
            for option in pair {
                let label = strings.component_kind_label(&option);
                if ui.radio(option == *kind, label).clicked() && option != *kind {
                    result.change_kind = Some(option);
                }
            }
        }
        None => {
            ui.label(strings.component_kind_label(kind));
        }
    }
    ui.add_space(8.0);

    ui.label(strings.property_name);
    // Kept as an owned buffer so an empty field can mean "no name at all"
    // rather than "a name that happens to be empty".
    let mut name = properties.name.clone().unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::singleline(&mut name)
            .desired_width(f32::INFINITY)
            .hint_text(strings.property_name_hint),
    );
    // The snapshot is taken when the field is entered, so a whole typed name
    // is one undo step.
    if response.gained_focus() {
        edit_started = true;
    }
    if response.changed() {
        properties.name = if name.is_empty() { None } else { Some(name) };
    }

    match kind {
        ComponentKind::Button | ComponentKind::Switch => {
            ui.add_space(8.0);
            let mut pressed = properties.pressed.unwrap_or(false);
            // The same stored value, said two ways, and now the same
            // *nature* too: a **resting** state. A press springs back, and
            // a switch is put back where this says when the project opens.
            // Where a switch is right now is runtime state, edited in the
            // value panel — which is what stops flipping one to see what
            // happens from filling the undo history.
            let (label, hint) = if *kind == ComponentKind::Switch {
                (strings.property_switch_on, strings.property_switch_on_hint)
            } else {
                (strings.property_pressed, strings.property_pressed_hint)
            };
            let response = ui.checkbox(&mut pressed, label).on_hover_text(hint);
            if response.changed() {
                edit_started = true;
                // Back to unset when it's the default again, so a project
                // that says nothing keeps saying nothing.
                properties.pressed = pressed.then_some(true);
            }
        }
        ComponentKind::Led => {
            ui.add_space(8.0);
            ui.label(strings.property_color);
            let mut color = properties.color.unwrap_or(DEFAULT_LED_COLOR);
            // One undo step per change rather than per editing session: a
            // swatch click is one discrete change and lands as one step, but
            // the wheel behind the button still gives no "started" signal to
            // hang a snapshot on, so dragging through it leaves a few behind.
            if color_control(ui, strings, &mut color) {
                edit_started = true;
                properties.color = Some(color);
            }
            ui.horizontal(|ui| {
                if properties.color.is_some() && ui.button(strings.property_reset).clicked() {
                    edit_started = true;
                    properties.color = None;
                }
            });
        }
        // Everything that drives a level you set by hand. An output port
        // only reads, so it has neither a resting value nor a say in how
        // many states the interface has — but it does have a width, which
        // is offered to all three below rather than here.
        // Deliberately not the T flip-flop: there the pins are not a choice,
        // so a checkbox there could only ever say what it already says.
        ComponentKind::DFlipFlop | ComponentKind::DFlipFlopFalling | ComponentKind::DLatch => {
            ui.add_space(8.0);
            let mut async_inputs = properties.async_set_reset(kind);
            if ui
                .checkbox(&mut async_inputs, strings.property_async_set_reset)
                .on_hover_text(strings.property_async_set_reset_hint)
                .changed()
            {
                edit_started = true;
                properties.async_set_reset = async_inputs.then_some(true);
            }
        }
        ComponentKind::InputPort | ComponentKind::InOutPort | ComponentKind::TriStateSource => {
            // A source has nothing to declare: three positions is what it
            // is. A port's count is a promise to whatever drives its pin
            // from outside, so there it is a choice.
            if *kind != ComponentKind::TriStateSource {
                ui.add_space(8.0);
                let mut tri_state = properties.is_tri_state();
                if ui
                    .checkbox(&mut tri_state, strings.property_tri_state)
                    .on_hover_text(strings.property_tri_state_hint)
                    .changed()
                {
                    edit_started = true;
                    properties.tri_state = tri_state.then_some(true);
                }
            }

            ui.add_space(8.0);
            ui.label(strings.property_initial);
            let current = properties.initial_level();
            for (level, label) in [
                (PortSetting::Undriven, strings.port_level_undriven),
                (PortSetting::Low, strings.port_level_low),
                (PortSetting::High, strings.port_level_high),
            ] {
                // Undriven stays offered even on a two-state port: it is
                // still where a port sits before anything drives it, and
                // hiding it would make that state unreachable rather than
                // absent.
                if ui.radio(current == level, label).clicked() && current != level {
                    edit_started = true;
                    // Back to unset when it's the default again, so a
                    // project that says nothing keeps saying nothing.
                    properties.initial = (level != PortSetting::default()).then_some(level);
                }
            }
        }
        ComponentKind::Splitter => {
            ui.add_space(8.0);
            ui.label(strings.property_branches);
            ui.label(RichText::new(strings.property_branches_hint).weak());

            let mut widths = properties.branch_widths();
            let mut count = widths.len();
            let response = ui.add(egui::DragValue::new(&mut count).range(1..=MAX_WIDTH));
            if response.drag_started() || response.gained_focus() {
                edit_started = true;
            }
            if response.changed() {
                // A branch that appears carries one bit; one that goes takes
                // its own with it. Neither renumbers the others, so adding a
                // branch at the end never moves the bits already assigned.
                widths.resize(count.max(1), 1);
                properties.branches = Some(widths.clone());
            }

            let mut bit = 0;
            let mut edited = false;
            for width in &mut widths {
                ui.horizontal(|ui| {
                    ui.label(crate::symbol::bit_range(bit, *width));
                    let response = ui.add(egui::DragValue::new(width).range(1..=MAX_WIDTH));
                    if response.drag_started() || response.gained_focus() {
                        edit_started = true;
                    }
                    edited |= response.changed();
                });
                bit += *width;
            }
            if edited {
                properties.branches = Some(widths);
            }

            // What the branches add up to, against what the bus carries.
            // Said rather than enforced: a drawing may legitimately leave
            // the top bits unbranched, and one that overshoots is a mistake
            // worth seeing rather than one to be silently corrected.
            let total = strings
                .property_branches_total
                .replace("{}", &bit.to_string())
                .replace("{bus}", &properties.width().to_string());
            let color = if bit == properties.width() {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().warn_fg_color
            };
            ui.label(RichText::new(total).color(color));
        }
        ComponentKind::Constant => {
            ui.add_space(8.0);
            ui.label(strings.property_value);
            let (edited, response) = value_field(
                ui,
                "constant_value",
                properties.constant_value(),
                properties.width(),
                base,
            );
            if response.gained_focus() {
                edit_started = true;
            }
            if let Some(value) = edited {
                // Unset at zero, so a project that never typed a value keeps
                // saying nothing about one.
                properties.value = (value != 0).then_some(value);
            }
            ui.label(RichText::new(strings.value_bases).weak());
        }
        _ => {}
    }

    if has_mirror(kind) {
        ui.add_space(8.0);
        let mut now = mirrored;
        if ui
            .checkbox(&mut now, strings.property_mirrored)
            .on_hover_text(strings.property_mirrored_hint)
            .changed()
        {
            edit_started = true;
            result.mirrored = Some(now);
        }
    }

    // Outside the match for the same reason the width is: several kinds
    // show a value, and which base is the same question for all of them.
    if Properties::has_base(kind) {
        ui.add_space(8.0);
        ui.label(strings.property_base);
        for (choice, label) in [
            (None, strings.base_follow_setting),
            (Some(NumberBase::Binary), strings.base_binary),
            (Some(NumberBase::Decimal), strings.base_decimal),
            (Some(NumberBase::Hexadecimal), strings.base_hexadecimal),
        ] {
            // *Follow the setting* is offered as a position of its own, not
            // as the absence of one: it is what almost everything is on, and
            // going back to it has to be as easy as leaving it.
            if ui.radio(properties.base == choice, label).clicked() && properties.base != choice {
                edit_started = true;
                properties.base = choice;
            }
        }
    }

    // Outside the match because it is not one kind's extra: a port declares
    // what its boundary carries, and a gate is the same gate on every bit of
    // it. `has_width` is what says which, in one place rather than here.
    if Properties::has_width(kind) {
        ui.add_space(8.0);
        ui.label(strings.property_width);
        let mut width = properties.width();
        let response = ui
            .add(egui::DragValue::new(&mut width).range(1..=MAX_WIDTH))
            .on_hover_text(strings.property_width_hint);
        if response.drag_started() || response.gained_focus() {
            edit_started = true;
        }
        if response.changed() {
            // Unset when it is back to a plain wire, so a project that never
            // asked for a bus keeps saying nothing about widths.
            properties.width = (width > 1).then_some(width);
        }
    }

    result.edit_started = edit_started;
    result
}

/// Draws the panel for the selected wire. Returns the colour it should now
/// have, when that changed this frame — `Some(None)` is a reset.
///
/// A wire's colour is really the *net's*: every wire of a net gets it, so
/// a conductor is one colour end to end. The caller does that spreading,
/// since only it knows what's connected to what.
pub fn show_wire(
    ui: &mut Ui,
    strings: &Strings,
    color: Option<[u8; 3]>,
    width: usize,
) -> Option<Option<[u8; 3]>> {
    let mut edited = None;

    ui.heading(strings.properties_heading);
    ui.add_space(4.0);
    ui.label(strings.property_wire);
    // Shown, never set: a wire takes its width from what it joins, so this
    // is a reading rather than a choice. Set it on a component.
    if width > 1 {
        ui.label(
            RichText::new(strings.property_wire_bits.replace("{}", &width.to_string())).weak(),
        );
    }
    ui.add_space(8.0);

    ui.label(strings.property_color);
    let mut picked = color.unwrap_or(DEFAULT_WIRE_COLOR);
    if color_control(ui, strings, &mut picked) {
        edited = Some(Some(picked));
    }
    ui.horizontal(|ui| {
        if color.is_some() && ui.button(strings.property_reset).clicked() {
            edited = Some(None);
        }
    });
    ui.add_space(4.0);
    ui.label(strings.property_wire_color_hint);

    edited
}

/// What the colour picker starts from for a wire that has none. Only a
/// starting point for the picker — an unset wire draws no casing at all.
pub const DEFAULT_WIRE_COLOR: [u8; 3] = [90, 127, 214];

/// What a LED glows when nothing says otherwise — a real one is red, which
/// is why this one colour doesn't follow the editor's theme.
pub const DEFAULT_LED_COLOR: [u8; 3] = [220, 30, 30];

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_properties_are_left_out_of_the_file_entirely() {
        let json = serde_json::to_string(&Properties::default()).expect("serializes");
        assert_eq!(json, "{}");
        assert!(Properties::default().is_empty());
    }

    #[test]
    fn a_hex_code_round_trips() {
        for color in SWATCHES {
            assert_eq!(from_hex(&to_hex(color)), Some(color));
        }
    }

    #[test]
    fn a_pasted_code_is_read_with_or_without_its_hash() {
        // Both spellings turn up when a code is copied from somewhere else,
        // and refusing one of them would be pedantry.
        assert_eq!(from_hex("#1A2B3C"), Some([0x1A, 0x2B, 0x3C]));
        assert_eq!(from_hex("1a2b3c"), Some([0x1A, 0x2B, 0x3C]));
        assert_eq!(from_hex("  #1a2b3c  "), Some([0x1A, 0x2B, 0x3C]));
    }

    #[test]
    fn a_half_typed_code_is_not_read_as_a_colour() {
        // The field is edited a character at a time, so most of what it
        // holds mid-typing is not yet a colour — and guessing at one would
        // make the value jump around while it is being entered.
        for text in ["", "#", "#1a2", "#1a2b3", "#1a2b3c4", "#gggggg", "hello"] {
            assert_eq!(from_hex(text), None, "{text:?} should not parse");
        }
    }

    #[test]
    fn no_two_swatches_are_the_same_colour() {
        // A duplicate would be a slot that says nothing, on a palette whose
        // whole job is telling things apart.
        let mut seen = SWATCHES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), SWATCHES.len());
    }

    #[test]
    fn a_name_of_nothing_but_spaces_is_not_a_name() {
        let properties = Properties {
            name: Some("   ".to_string()),
            ..Default::default()
        };
        assert_eq!(properties.label(), None);
    }

    #[test]
    fn a_value_is_read_in_whichever_base_it_was_written() {
        // All three, because a value copied from a datasheet, from code or
        // from a probe arrives in whichever base that source used.
        assert_eq!(parse_value("42"), Some(42));
        assert_eq!(parse_value("0x2A"), Some(42));
        assert_eq!(parse_value("0b101010"), Some(42));
        assert_eq!(parse_value(" 0x2a "), Some(42));
        // Underscores group digits in every language that has them.
        assert_eq!(parse_value("0b1010_1010"), Some(0b1010_1010));
        // And nonsense is nothing rather than zero: a half-typed value must
        // not read as 0 while the caret is still in the field.
        assert_eq!(parse_value("0x"), None);
        assert_eq!(parse_value("twelve"), None);
    }

    #[test]
    fn one_value_reads_the_same_wherever_it_is_written() {
        // The fault Romain saw: a constant showed `10` where a probe on the
        // same value showed `A`, because there were two formatting rules.
        // The base is the same question everywhere; only the prefix is a
        // matter of where it is written.
        let bare = format_value(10, 8, NumberBase::Auto, false);
        let typed = format_value(10, 8, NumberBase::Auto, true);
        assert_eq!(bare, "A", "a readout takes the bare digits");
        assert_eq!(typed, "0xA", "a field you type into carries its prefix");
    }

    #[test]
    fn auto_is_a_choice_not_made_rather_than_a_fourth_base() {
        // A plain wire reads 0/1 whatever the base, so `Auto` there is
        // decimal; a bus is a bit pattern, and hex is how one is read.
        assert_eq!(format_value(1, 1, NumberBase::Auto, false), "1");
        assert_eq!(format_value(12, 8, NumberBase::Auto, false), "C");
        // And naming a base overrides that, at any width.
        assert_eq!(format_value(12, 8, NumberBase::Decimal, false), "12");
        assert_eq!(format_value(12, 4, NumberBase::Binary, false), "1100");
    }

    #[test]
    fn properties_survive_a_round_trip() {
        let properties = Properties {
            name: Some("clk".to_string()),
            pressed: Some(true),
            color: Some([1, 2, 3]),
            tri_state: Some(true),
            initial: Some(PortSetting::High),
            width: Some(8),
            async_set_reset: Some(true),
            base: Some(NumberBase::Binary),
            branches: Some(vec![4, 4]),
            value: Some(0xAB),
        };

        let json = serde_json::to_string(&properties).expect("serializes");
        let parsed: Properties = serde_json::from_str(&json).expect("parses");

        assert_eq!(parsed, properties);
        // And the *text*, not only the round trip. What reaches the file is
        // the field and variant names, never the Rust type names — which is
        // what makes renaming a type free, and is worth checking rather than
        // asserting: `PortLevel` became `PortSetting` on the strength of it.
        assert_eq!(
            json,
            r#"{"name":"clk","pressed":true,"color":[1,2,3],"tri_state":true,"initial":"High","width":8,"async_set_reset":true,"base":"Binary","branches":[4,4],"value":171}"#
        );
    }

    #[test]
    fn a_ports_resting_level_defaults_to_undriven() {
        // Nothing has said what it carries yet, so claiming a level would be
        // inventing one — and undriven is the case worth being able to test.
        assert_eq!(Properties::default().initial_level(), PortSetting::Undriven);
        assert!(!Properties::default().is_tri_state());
    }

    #[test]
    fn a_component_saved_before_properties_existed_reads_as_having_none() {
        let parsed: Properties = serde_json::from_str("{}").expect("parses");
        assert!(parsed.is_empty());
    }
}
