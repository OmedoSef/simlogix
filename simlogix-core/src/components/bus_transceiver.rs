use crate::component::{across_bits, Component};
use crate::level::Level;
use crate::signal::Signal;

/// A bus transceiver: two bus-side pins, and a direction that says which one
/// is listening and which one is talking.
///
/// Pins, in order: `A`, `B` (both `InOut`), `Dir`, and the enable. Disabled,
/// both sides let go of their nets entirely. Enabled, `Dir` high means `A`
/// drives `B` and `Dir` low means `B` drives `A` — the side that is
/// listening drives `HighZ`, so it adds nothing to its own net.
///
/// # Two polarities, on purpose
///
/// The enable comes in both senses, chosen at construction, because both
/// exist in real schematics and neither is a default the other can stand in
/// for:
///
/// - [`BusTransceiver::active_low`] is `OE`, the polarity of the 74x245 it's
///   named after — pulled to ground to switch the outputs on. Its symbol
///   draws the inversion bubble.
/// - [`BusTransceiver::active_high`] is `EN`, asserted high, matching the
///   tri-state buffer's enable and every HDL primitive.
///
/// The electrical reason behind active-low enables doesn't carry over here
/// — an unconnected pin reads `Unknown`, not `High` as a floating TTL input
/// would — but the convention is what a reader recognises, and a schematic
/// that mixes the two marks the difference with a bubble rather than
/// picking one.
#[derive(Debug, Clone, Copy)]
pub struct BusTransceiver {
    active_low_enable: bool,
}

impl BusTransceiver {
    /// `OE`: the transceiver is on while its enable pin is **low**.
    pub fn active_low() -> Self {
        Self {
            active_low_enable: true,
        }
    }

    /// `EN`: the transceiver is on while its enable pin is **high**.
    pub fn active_high() -> Self {
        Self {
            active_low_enable: false,
        }
    }

    /// The enable pin read as plain "is it asserted", whichever way round
    /// the pin is. Only the two definite levels flip: `Unknown`, `HighZ` and
    /// `Error` say nothing about polarity, so inverting them would invent
    /// information.
    fn asserted(&self, enable: Level) -> Level {
        if !self.active_low_enable {
            return enable;
        }
        match enable {
            Level::High => Level::Low,
            Level::Low => Level::High,
            other => other,
        }
    }
}

impl Component for BusTransceiver {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a, b, dir, enable] => {
                // `Dir` and the enable are one wire each whatever the two
                // sides are wide: they pick a direction for the bus, not for
                // one of its bits.
                let dir = dir.only_level();
                let enabled = self.asserted(enable.only_level());
                across_bits(&[a, b], |bits| match bits {
                    [a, b] => drive(*a, *b, dir, enabled),
                    _ => vec![Level::Unknown, Level::Unknown],
                })
            }
            _ => vec![Signal::bit(Level::Unknown), Signal::bit(Level::Unknown)],
        }
    }
}

/// What to drive on `A` and `B`, in that order. `enabled` has already been
/// normalised to active-high by the caller, so there is one truth table
/// rather than one per polarity.
fn drive(a: Level, b: Level, dir: Level, enabled: Level) -> Vec<Level> {
    let both = |signal| vec![signal, signal];
    match (enabled, dir) {
        // Disabled wins over everything, including a faulted direction: a
        // chip that isn't driving can't put a fault on anything.
        (Level::Low, _) => both(Level::HighZ),
        (Level::Error, _) | (Level::High, Level::Error) => both(Level::Error),
        (Level::High, Level::High) => vec![Level::HighZ, a],
        (Level::High, Level::Low) => vec![b, Level::HighZ],
        // Enabled-ness or direction is undriven or not yet known. `HighZ`
        // would claim the chip is deliberately out of the way, which is more
        // than is known.
        _ => both(Level::Unknown),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::eval_levels;

    /// `[drive on A, drive on B]`.
    fn eval(part: BusTransceiver, a: Level, b: Level, dir: Level, enable: Level) -> Vec<Level> {
        eval_levels(&part, &[a, b, dir, enable])
    }

    /// The pin level that switches each variant *on*.
    fn on(part: BusTransceiver) -> Level {
        if part.active_low_enable {
            Level::Low
        } else {
            Level::High
        }
    }

    fn both_variants() -> [BusTransceiver; 2] {
        [BusTransceiver::active_low(), BusTransceiver::active_high()]
    }

    #[test]
    fn direction_high_sends_a_to_b() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Level::High, Level::Unknown, Level::High, on(part)),
                vec![Level::HighZ, Level::High]
            );
        }
    }

    #[test]
    fn direction_low_sends_b_to_a() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Level::Unknown, Level::Low, Level::Low, on(part)),
                vec![Level::Low, Level::HighZ]
            );
        }
    }

    #[test]
    fn the_listening_side_drives_nothing_onto_its_own_net() {
        // The half that makes reading a net you also drive a non-issue: the
        // side being read is always the side driving `HighZ`.
        for part in both_variants() {
            let out = eval(part, Level::High, Level::Low, Level::High, on(part));
            assert_eq!(out[0], Level::HighZ, "A is listening, so A drives nothing");
        }
    }

    #[test]
    fn the_two_variants_are_switched_on_by_opposite_levels() {
        let oe = BusTransceiver::active_low();
        let en = BusTransceiver::active_high();

        // The same pin level does opposite things — which is the whole
        // reason both exist, and why the symbol has to say which is which.
        assert_eq!(
            eval(oe, Level::High, Level::Low, Level::High, Level::Low),
            vec![Level::HighZ, Level::High],
            "OE low is on"
        );
        assert_eq!(
            eval(en, Level::High, Level::Low, Level::High, Level::Low),
            vec![Level::HighZ, Level::HighZ],
            "EN low is off"
        );
    }

    #[test]
    fn disabled_it_lets_go_of_both_sides() {
        for part in both_variants() {
            let off = if part.active_low_enable {
                Level::High
            } else {
                Level::Low
            };
            for dir in [Level::High, Level::Low, Level::Unknown, Level::Error] {
                assert_eq!(
                    eval(part, Level::High, Level::Low, dir, off),
                    vec![Level::HighZ, Level::HighZ],
                    "a disabled transceiver drives nothing, whatever Dir says ({dir:?})"
                );
            }
        }
    }

    #[test]
    fn a_faulted_control_pin_is_reported_on_both_sides() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Level::High, Level::Low, Level::Error, on(part)),
                vec![Level::Error, Level::Error]
            );
            // A faulted enable is faulted whichever way the pin is read:
            // inverting it would be inventing a level it doesn't have.
            assert_eq!(
                eval(part, Level::High, Level::Low, Level::High, Level::Error),
                vec![Level::Error, Level::Error]
            );
        }
    }

    #[test]
    fn an_undriven_control_is_not_the_same_as_being_switched_off() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Level::High, Level::Low, Level::Unknown, on(part)),
                vec![Level::Unknown, Level::Unknown],
                "an undriven direction"
            );
            assert_eq!(
                eval(part, Level::High, Level::Low, Level::High, Level::Unknown),
                vec![Level::Unknown, Level::Unknown],
                "an undriven enable"
            );
        }
    }

    #[test]
    fn it_carries_a_bus_across_bit_by_bit() {
        // Mixed bits, so a transceiver that only ever looked at bit 0 would
        // fail rather than pass by luck.
        let value = Signal::from_levels(vec![Level::High, Level::Low, Level::Low, Level::High]);
        for part in both_variants() {
            assert_eq!(
                part.eval(&[
                    value.clone(),
                    Signal::splat(Level::Unknown, 4),
                    Signal::bit(Level::High),
                    Signal::bit(on(part)),
                ]),
                vec![Signal::splat(Level::HighZ, 4), value.clone()],
                "A drives B, and the listening side lets go of every bit"
            );
        }
    }

    #[test]
    fn the_direction_and_the_enable_stay_one_wire_whatever_the_bus() {
        let part = BusTransceiver::active_high();
        let wide = || Signal::splat(Level::High, 4);
        assert_eq!(
            part.eval(&[
                wide(),
                wide(),
                Signal::bit(Level::Low),
                Signal::bit(Level::Low)
            ]),
            vec![
                Signal::splat(Level::HighZ, 4),
                Signal::splat(Level::HighZ, 4)
            ],
            "disabled, it lets go of every bit of both sides"
        );
    }

    #[test]
    fn two_sides_of_different_widths_have_no_bit_by_bit_answer() {
        // The same loud refusal a gate makes: the drawing already rings a pin
        // that disagrees with its net, so the claim here is the loudest one
        // available rather than an answer about the bits that line up.
        let part = BusTransceiver::active_high();
        assert_eq!(
            part.eval(&[
                Signal::splat(Level::High, 2),
                Signal::splat(Level::High, 4),
                Signal::bit(Level::High),
                Signal::bit(Level::High),
            ]),
            vec![
                Signal::splat(Level::Error, 4),
                Signal::splat(Level::Error, 4)
            ]
        );
    }
}
