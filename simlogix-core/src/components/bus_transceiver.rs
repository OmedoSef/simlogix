use crate::component::Component;
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
    fn asserted(&self, enable: Signal) -> Signal {
        if !self.active_low_enable {
            return enable;
        }
        match enable {
            Signal::High => Signal::Low,
            Signal::Low => Signal::High,
            other => other,
        }
    }
}

impl Component for BusTransceiver {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a, b, dir, enable] => drive(*a, *b, *dir, self.asserted(*enable)),
            _ => vec![Signal::Unknown, Signal::Unknown],
        }
    }
}

/// What to drive on `A` and `B`, in that order. `enabled` has already been
/// normalised to active-high by the caller, so there is one truth table
/// rather than one per polarity.
fn drive(a: Signal, b: Signal, dir: Signal, enabled: Signal) -> Vec<Signal> {
    let both = |signal| vec![signal, signal];
    match (enabled, dir) {
        // Disabled wins over everything, including a faulted direction: a
        // chip that isn't driving can't put a fault on anything.
        (Signal::Low, _) => both(Signal::HighZ),
        (Signal::Error, _) | (Signal::High, Signal::Error) => both(Signal::Error),
        (Signal::High, Signal::High) => vec![Signal::HighZ, a],
        (Signal::High, Signal::Low) => vec![b, Signal::HighZ],
        // Enabled-ness or direction is undriven or not yet known. `HighZ`
        // would claim the chip is deliberately out of the way, which is more
        // than is known.
        _ => both(Signal::Unknown),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `[drive on A, drive on B]`.
    fn eval(
        part: BusTransceiver,
        a: Signal,
        b: Signal,
        dir: Signal,
        enable: Signal,
    ) -> Vec<Signal> {
        part.eval(&[a, b, dir, enable])
    }

    /// The pin level that switches each variant *on*.
    fn on(part: BusTransceiver) -> Signal {
        if part.active_low_enable {
            Signal::Low
        } else {
            Signal::High
        }
    }

    fn both_variants() -> [BusTransceiver; 2] {
        [BusTransceiver::active_low(), BusTransceiver::active_high()]
    }

    #[test]
    fn direction_high_sends_a_to_b() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Signal::High, Signal::Unknown, Signal::High, on(part)),
                vec![Signal::HighZ, Signal::High]
            );
        }
    }

    #[test]
    fn direction_low_sends_b_to_a() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Signal::Unknown, Signal::Low, Signal::Low, on(part)),
                vec![Signal::Low, Signal::HighZ]
            );
        }
    }

    #[test]
    fn the_listening_side_drives_nothing_onto_its_own_net() {
        // The half that makes reading a net you also drive a non-issue: the
        // side being read is always the side driving `HighZ`.
        for part in both_variants() {
            let out = eval(part, Signal::High, Signal::Low, Signal::High, on(part));
            assert_eq!(out[0], Signal::HighZ, "A is listening, so A drives nothing");
        }
    }

    #[test]
    fn the_two_variants_are_switched_on_by_opposite_levels() {
        let oe = BusTransceiver::active_low();
        let en = BusTransceiver::active_high();

        // The same pin level does opposite things — which is the whole
        // reason both exist, and why the symbol has to say which is which.
        assert_eq!(
            eval(oe, Signal::High, Signal::Low, Signal::High, Signal::Low),
            vec![Signal::HighZ, Signal::High],
            "OE low is on"
        );
        assert_eq!(
            eval(en, Signal::High, Signal::Low, Signal::High, Signal::Low),
            vec![Signal::HighZ, Signal::HighZ],
            "EN low is off"
        );
    }

    #[test]
    fn disabled_it_lets_go_of_both_sides() {
        for part in both_variants() {
            let off = if part.active_low_enable {
                Signal::High
            } else {
                Signal::Low
            };
            for dir in [Signal::High, Signal::Low, Signal::Unknown, Signal::Error] {
                assert_eq!(
                    eval(part, Signal::High, Signal::Low, dir, off),
                    vec![Signal::HighZ, Signal::HighZ],
                    "a disabled transceiver drives nothing, whatever Dir says ({dir:?})"
                );
            }
        }
    }

    #[test]
    fn a_faulted_control_pin_is_reported_on_both_sides() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Signal::High, Signal::Low, Signal::Error, on(part)),
                vec![Signal::Error, Signal::Error]
            );
            // A faulted enable is faulted whichever way the pin is read:
            // inverting it would be inventing a level it doesn't have.
            assert_eq!(
                eval(part, Signal::High, Signal::Low, Signal::High, Signal::Error),
                vec![Signal::Error, Signal::Error]
            );
        }
    }

    #[test]
    fn an_undriven_control_is_not_the_same_as_being_switched_off() {
        for part in both_variants() {
            assert_eq!(
                eval(part, Signal::High, Signal::Low, Signal::Unknown, on(part)),
                vec![Signal::Unknown, Signal::Unknown],
                "an undriven direction"
            );
            assert_eq!(
                eval(
                    part,
                    Signal::High,
                    Signal::Low,
                    Signal::High,
                    Signal::Unknown
                ),
                vec![Signal::Unknown, Signal::Unknown],
                "an undriven enable"
            );
        }
    }
}
