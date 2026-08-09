use crate::component::Component;
use crate::signal::Signal;

/// A bus transceiver: two bus-side pins, and a direction that says which one
/// is listening and which one is talking.
///
/// Pins, in order: `A`, `B` (both `InOut`), `Dir`, `Enable`. With `Enable`
/// low both sides let go of their nets entirely. With it high, `Dir` high
/// means `A` drives `B`, and `Dir` low means `B` drives `A` — the side that
/// is listening drives `HighZ`, so it adds nothing to its own net.
///
/// # Reading a net you also drive
///
/// This is the first component with `InOut` pins, and it raises a question
/// the engine had never had to answer: `eval` is handed the *resolved* value
/// of every pin that reads, and an `InOut` pin reads the very net it drives,
/// so the component sees its own contribution folded in.
///
/// That turns out not to need fixing, for two reasons.
///
/// Within one evaluation it can't bite: `Dir` picks read-`A`-drive-`B` or
/// read-`B`-drive-`A`, never both on the same pin, and the pin being read is
/// always the one driving `HighZ`. There is no echo to see.
///
/// Across a *change* of direction there is one tick where the side newly
/// being read still carries this component's own contribution from the tick
/// before, because that contribution is only withdrawn when the new outputs
/// land. That's bus turnaround, and it is real — a physical transceiver
/// glitches in exactly the same way if the direction is flipped while both
/// sides are driving, which is why bus protocols insert turnaround cycles.
///
/// Having the engine hide a component's own contribution from it was the
/// alternative, and it would be wrong in general: an open-drain pin has to
/// see its own pull-down on the wire to do arbitration at all. Reading the
/// true resolved net is the accurate model, not a compromise.
#[derive(Debug, Default, Clone, Copy)]
pub struct BusTransceiver;

impl Component for BusTransceiver {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [a, b, dir, enable] => drive(*a, *b, *dir, *enable),
            _ => vec![Signal::Unknown, Signal::Unknown],
        }
    }
}

/// What to drive on `A` and `B`, in that order.
fn drive(a: Signal, b: Signal, dir: Signal, enable: Signal) -> Vec<Signal> {
    let both = |signal| vec![signal, signal];
    match (enable, dir) {
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
    fn eval(a: Signal, b: Signal, dir: Signal, enable: Signal) -> Vec<Signal> {
        BusTransceiver.eval(&[a, b, dir, enable])
    }

    #[test]
    fn direction_high_sends_a_to_b() {
        assert_eq!(
            eval(Signal::High, Signal::Unknown, Signal::High, Signal::High),
            vec![Signal::HighZ, Signal::High]
        );
    }

    #[test]
    fn direction_low_sends_b_to_a() {
        assert_eq!(
            eval(Signal::Unknown, Signal::Low, Signal::Low, Signal::High),
            vec![Signal::Low, Signal::HighZ]
        );
    }

    #[test]
    fn the_listening_side_drives_nothing_onto_its_own_net() {
        // The half that makes reading a net you also drive a non-issue: the
        // side being read is always the side driving `HighZ`.
        let out = eval(Signal::High, Signal::Low, Signal::High, Signal::High);
        assert_eq!(out[0], Signal::HighZ, "A is listening, so A drives nothing");
    }

    #[test]
    fn disabled_it_lets_go_of_both_sides() {
        for dir in [Signal::High, Signal::Low, Signal::Unknown, Signal::Error] {
            assert_eq!(
                eval(Signal::High, Signal::Low, dir, Signal::Low),
                vec![Signal::HighZ, Signal::HighZ],
                "a disabled transceiver drives nothing, whatever Dir says ({dir:?})"
            );
        }
    }

    #[test]
    fn a_faulted_control_pin_is_reported_on_both_sides() {
        assert_eq!(
            eval(Signal::High, Signal::Low, Signal::Error, Signal::High),
            vec![Signal::Error, Signal::Error]
        );
        assert_eq!(
            eval(Signal::High, Signal::Low, Signal::High, Signal::Error),
            vec![Signal::Error, Signal::Error]
        );
    }

    #[test]
    fn an_undriven_direction_is_not_the_same_as_being_switched_off() {
        assert_eq!(
            eval(Signal::High, Signal::Low, Signal::Unknown, Signal::High),
            vec![Signal::Unknown, Signal::Unknown]
        );
    }
}
