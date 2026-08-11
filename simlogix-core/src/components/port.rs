//! A circuit's boundary: the pins it exposes to whatever contains it.
//!
//! These have to be useful *before* anything instantiates the circuit — it
//! is opened and tested on its own long before it is reused, and a port that
//! only made sense inside a parent would make the circuit untestable exactly
//! while it's being written. So a driving port is something you set by hand,
//! and every port reads back what its net carries.
//!
//! When instantiation arrives, the parent supplies the values instead;
//! nothing here needs to change shape for that.

use std::cell::Cell;
use std::rc::Rc;

use crate::component::{scalar_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// Where a driving port has been *set*, which is not the same thing as
/// what its net comes to carry — hence a name of its own rather than
/// `PortLevel`, which read like a [`Level`] and sat next to one.
///
/// Three positions rather than a `bool` because "not driving" is a real
/// third choice, not the absence of one — and it's the case worth testing,
/// since it's what an unconnected parent pin gives you.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PortSetting {
    /// Driving nothing. What that *means* depends on the port — see
    /// [`CircuitPort::input`] and [`CircuitPort::bidirectional`].
    #[default]
    Undriven,
    Low,
    High,
}

impl PortSetting {
    /// What this resting setting means for a port `width` bits wide.
    ///
    /// Coarse on purpose: a *resting* value is one of three things — not
    /// driving, all low, all high — because it is what the port sits at
    /// before anyone has touched it. Typing an arbitrary value is done to
    /// the live [`PortDrive`], which is runtime state and is never saved.
    pub fn to_drive(self, width: usize) -> PortDrive {
        match self {
            Self::Undriven => PortDrive::Undriven,
            Self::Low => PortDrive::Driving(0),
            Self::High => PortDrive::Driving(all_ones(width)),
        }
    }

    /// The next position of the click cycle: undriven, high, low, round
    /// again. `tri_state` off skips the undriven position entirely, so a
    /// port declared two-state can't be clicked into a third.
    pub fn next(self, tri_state: bool) -> Self {
        match (self, tri_state) {
            (Self::Undriven, _) => Self::High,
            (Self::High, _) => Self::Low,
            (Self::Low, true) => Self::Undriven,
            (Self::Low, false) => Self::High,
        }
    }
}

/// All bits of a `width`-bit value set, saturating at the 64 a `u64` holds.
pub fn all_ones(width: usize) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

/// What a port is driving **right now**: nothing at all, or these bits,
/// least significant first.
///
/// Runtime state, never saved — the same nature as a button being held. It
/// carries a whole value rather than one level because a port stands for
/// *what a parent will drive*, and a parent drives whatever it likes; the
/// earlier model gave every bit the same level, so a two-bit port could
/// only ever be 0 or 3.
///
/// Kept apart from [`PortSetting`], which is the *resting* value and is a
/// saved property. The same digits, two different natures: one field for
/// both would have to lie about one of them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PortDrive {
    #[default]
    Undriven,
    Driving(u64),
}

impl PortDrive {
    /// The level of bit `index`, given what "not driving" means for this
    /// port — `Unknown` on an input, `HighZ` on a bidirectional one.
    pub fn level(self, index: usize, undriven: Level) -> Level {
        match self {
            Self::Undriven => undriven,
            Self::Driving(bits) if index < 64 && bits & (1 << index) != 0 => Level::High,
            Self::Driving(_) => Level::Low,
        }
    }

    /// The next position of the click cycle: **one step up the value**,
    /// wrapping past the top to undriven. `tri_state` off skips the
    /// undriven position entirely.
    ///
    /// One rule at every width, and at one bit it *is* the switch it has
    /// always been — undriven, low, high, round again. On a bus it counts,
    /// which is usable by hand at four bits and harmless at thirty-two: a
    /// click can never move the value by more than a step.
    ///
    /// It used to rebuild the value out of `all_ones`, so a click on a bus
    /// slammed every bit to the same level and wiped whatever had been
    /// typed. Handy on a plain wire, where the value *is* the position;
    /// destructive on a bus, where a port stands for what a parent will
    /// drive and a parent drives whatever it likes. Setting a whole value
    /// at once is what the value field is for.
    pub fn next(self, tri_state: bool, width: usize) -> Self {
        let top = all_ones(width);
        match (self, tri_state) {
            (Self::Undriven, _) => Self::Driving(0),
            (Self::Driving(bits), _) if bits < top => Self::Driving(bits + 1),
            // Past the top: back to letting go, or round to nothing again.
            (Self::Driving(_), true) => Self::Undriven,
            (Self::Driving(_), false) => Self::Driving(0),
        }
    }
}

/// A port that drives its net, and can be told to stop.
///
/// The two constructors differ only in what "undriven" puts on the wire, and
/// that difference matters more than it looks — see each.
pub struct CircuitPort {
    /// What [`PortSetting::Undriven`] resolves to for this port.
    undriven: Level,
    drive: Rc<Cell<PortDrive>>,
    /// How many bits it drives — the same number the drawing gave the net.
    ///
    /// Shared rather than copied in, for the same reason the level is: the
    /// GUI owns the property and can change it at any moment, and a port
    /// driving a width the net no longer has would fault every bit of it.
    width: Rc<Cell<usize>>,
}

/// The two handles the GUI keeps on a port: what it is set to, and how wide
/// it is. Both are properties it owns and can change without rebuilding.
pub struct PortHandles {
    pub drive: Rc<Cell<PortDrive>>,
    pub width: Rc<Cell<usize>>,
}

impl CircuitPort {
    /// A value entering the circuit. Undriven is [`Level::Unknown`]:
    /// nothing outside is supplying it, so its value genuinely isn't known.
    pub fn input() -> (Self, PortHandles) {
        Self::new(Level::Unknown)
    }

    /// A port carrying values both ways. Undriven is [`Level::HighZ`],
    /// *not* `Unknown`: it has to actually let go so the circuit inside can
    /// drive the net. `Unknown` counts as a driver, so it would put every
    /// net a bidirectional port touches into conflict instead of stepping
    /// aside.
    pub fn bidirectional() -> (Self, PortHandles) {
        Self::new(Level::HighZ)
    }

    fn new(undriven: Level) -> (Self, PortHandles) {
        let drive = Rc::new(Cell::new(PortDrive::default()));
        let width = Rc::new(Cell::new(1));
        (
            Self {
                undriven,
                drive: Rc::clone(&drive),
                width: Rc::clone(&width),
            },
            PortHandles { drive, width },
        )
    }
}

impl Component for CircuitPort {
    /// The first component that is genuinely width-aware: it drives every
    /// bit of its width alike.
    ///
    /// Setting a port to eight bits and driving one would be a contribution
    /// of the wrong width, and the net would fault every bit of itself —
    /// correctly, since a component that says it is eight bits wide and
    /// supplies one is lying about its own contract.
    ///
    /// Bit by bit from whatever value it was set to, so a two-bit port can
    /// be any of the four things a parent might drive into it — not only
    /// all-low and all-high, which is all a single level could say.
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        let drive = self.drive.get();
        let width = self.width.get().max(1);
        vec![Signal::from_levels(
            (0..width)
                .map(|bit| drive.level(bit, self.undriven))
                .collect(),
        )]
    }
}

/// A value leaving the circuit.
///
/// A pure sink, like [`crate::Led`]: it drives nothing, and what it carries
/// is read from the net its pin sits on. Nothing to evaluate — the value is
/// already there. It has no "three-state" choice of its own for the same
/// reason: it reads whatever is on the net, including nothing at all.
#[derive(Debug, Default, Clone, Copy)]
pub struct CircuitOutput;

impl Component for CircuitOutput {
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        scalar_eval(_inputs, |_inputs| Vec::new())
    }
}

/// The pins an instance of a sub-circuit exposes to the circuit containing
/// it — one per port, driving nothing.
///
/// It exists only to give the parent's wires something to attach to, and to
/// be a point the net rebuild can union with the sub-circuit's internals.
/// The sub-circuit's own port components are *not* instantiated: a port's
/// pin is only ever a member of an inner net, so unioning this pin with that
/// net is the whole connection — and an input port that stayed alive would
/// fight whatever the parent drives into it.
#[derive(Debug, Default, Clone, Copy)]
pub struct CircuitAnchor;

impl Component for CircuitAnchor {
    /// **Nothing at all**, not even `HighZ`.
    ///
    /// `HighZ` reads as "deliberately not driving", which is true, but it
    /// is still a *contribution* — and a contribution has a width. One bit
    /// of it against a pin occupying four faults the net on every bit,
    /// which is what a splitter's own pins would have done to the bus they
    /// are supposed to be part of. An anchor is not a driver; the honest
    /// way to say so is to hand back no signal.
    fn eval(&self, _inputs: &[Signal]) -> Vec<Signal> {
        Vec::new()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::eval_levels;

    #[test]
    fn an_input_holds_the_level_it_was_set_to() {
        let (port, handles) = CircuitPort::input();
        // Undriven from the outside is exactly what "not known" means here.
        assert_eq!(eval_levels(&port, &[]), vec![Level::Unknown]);

        handles.drive.set(PortSetting::High.to_drive(1));
        assert_eq!(eval_levels(&port, &[]), vec![Level::High]);
        // Latching, not momentary: nothing releases it.
        assert_eq!(eval_levels(&port, &[]), vec![Level::High]);
    }

    #[test]
    fn an_undriven_bidirectional_port_steps_aside_rather_than_clouding_its_net() {
        let (port, handles) = CircuitPort::bidirectional();
        // The difference from an input, and the reason the two exist:
        // `HighZ` is ignored when a net resolves, so the circuit inside can
        // drive it. `Unknown` would count as a driver and cause a conflict.
        assert_eq!(eval_levels(&port, &[]), vec![Level::HighZ]);

        handles.drive.set(PortSetting::Low.to_drive(1));
        assert_eq!(eval_levels(&port, &[]), vec![Level::Low]);
    }

    #[test]
    fn a_port_drives_the_value_it_was_set_to_bit_by_bit() {
        let (port, handles) = CircuitPort::input();
        handles.width.set(4);

        // The point of a value rather than a level: a two-bit port used to
        // manage only 0 and 3, because every bit got the same level. A port
        // stands for what a *parent* will drive, and a parent drives
        // whatever it likes.
        handles.drive.set(PortDrive::Driving(0b0101));
        assert_eq!(
            port.eval(&[])[0].levels(),
            [Level::High, Level::Low, Level::High, Level::Low],
            "bit 0 is the least significant"
        );

        // All high is still one setting away, and it means all four.
        handles.drive.set(PortSetting::High.to_drive(4));
        assert_eq!(port.eval(&[])[0].levels(), [Level::High; 4]);

        // Undriven is a whole-port state, not a value.
        handles.drive.set(PortDrive::Undriven);
        assert_eq!(port.eval(&[])[0].levels(), [Level::Unknown; 4]);
    }

    #[test]
    fn a_click_on_a_plain_wire_is_still_the_switch_it_has_always_been() {
        // One bit is the case the click cycle was built for, and the rule
        // has to keep meaning the same thing there: off, on, let go.
        let mut drive = PortDrive::Undriven;
        for expected in [
            PortDrive::Driving(0),
            PortDrive::Driving(1),
            PortDrive::Undriven,
        ] {
            drive = drive.next(true, 1);
            assert_eq!(drive, expected);
        }

        // Two-state: it never reaches undriven again once it has left.
        let mut drive = PortDrive::Driving(0);
        for expected in [PortDrive::Driving(1), PortDrive::Driving(0)] {
            drive = drive.next(false, 1);
            assert_eq!(drive, expected);
        }
    }

    #[test]
    fn a_click_on_a_bus_counts_rather_than_wiping_it() {
        // What Romain reported: a click rebuilt the value out of `all_ones`,
        // so it slammed every bit alike and threw away whatever had been
        // typed. A step at a time can never lose more than a step.
        let mut drive = PortDrive::Driving(0b1010);
        for expected in [
            PortDrive::Driving(0b1011),
            PortDrive::Driving(0b1100),
            PortDrive::Driving(0b1101),
        ] {
            drive = drive.next(true, 4);
            assert_eq!(drive, expected);
        }

        // And past the top it lets go rather than wrapping silently, so the
        // cycle still has an end you can reach by clicking.
        assert_eq!(
            PortDrive::Driving(0b1111).next(true, 4),
            PortDrive::Undriven
        );
        assert_eq!(
            PortDrive::Driving(0b1111).next(false, 4),
            PortDrive::Driving(0)
        );
    }

    #[test]
    fn the_click_cycle_skips_undriven_unless_the_port_is_three_state() {
        let mut level = PortSetting::Undriven;
        for expected in [PortSetting::High, PortSetting::Low, PortSetting::Undriven] {
            level = level.next(true);
            assert_eq!(level, expected);
        }

        // Two-state: it never reaches undriven again once it has left.
        let mut level = PortSetting::High;
        for expected in [PortSetting::Low, PortSetting::High, PortSetting::Low] {
            level = level.next(false);
            assert_eq!(level, expected);
        }
    }

    #[test]
    fn a_two_state_port_still_leaves_undriven_when_it_starts_there() {
        // Otherwise a port set to two-state *after* being left undriven
        // would be stuck there with no way to click out of it.
        assert_eq!(PortSetting::Undriven.next(false), PortSetting::High);
    }

    #[test]
    fn an_anchor_contributes_nothing_to_any_of_its_pins() {
        // Nothing at all, not `HighZ` on each. `HighZ` says "deliberately
        // not driving", which is true, but it is still a contribution — and
        // a contribution has a width. One bit of it against a pin occupying
        // four faults the net on every bit, which is what a splitter's own
        // pins would do to the bus they are part of.
        assert!(CircuitAnchor
            .eval(&[
                Signal::bit(Level::High),
                Signal::bit(Level::Low),
                Signal::bit(Level::Unknown),
            ])
            .is_empty());
    }

    #[test]
    fn an_output_drives_nothing() {
        assert!(eval_levels(&CircuitOutput, &[Level::High]).is_empty());
    }
}
