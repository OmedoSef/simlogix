use crate::component::Component;
use crate::level::Level;
use crate::signal::Signal;

/// A bus taken apart into narrower branches, and put back together.
///
/// Pin 0 is the **bus**; every pin after it is a **branch**, taking the next
/// bits from bit 0 upward — the first branch the low bits, the next the ones
/// above them, and so on. Nothing says which way a value travels: a branch
/// driven from outside reaches the bus, and a bus driven from outside reaches
/// the branches. Which happens falls out of what is connected, exactly as
/// multi-driver resolution already decides everywhere else, so there is no
/// separate merger to build.
///
/// **It holds no state and is told nothing.** The bus's width and each
/// branch's width arrive in the *shape* of the signals it is handed, since a
/// net is as wide as the drawing said. So the widths cannot come to disagree
/// with the drawing: there is no second copy of them here to drift.
///
/// **It is a relay, and that is why it does not hear itself**
/// ([`Component::reads_own_contribution`]). Repeating what it hears while
/// hearing its own repetition would leave it holding a value long after
/// whatever really drove it had let go.
///
/// A bit nobody drives from the far side is answered with `HighZ` rather
/// than `Unknown`: the splitter genuinely is not driving it, and saying
/// `Unknown` would make it a driver in conflict with everyone else.
///
/// **This is a relay, and a relay is not what a splitter is.** A splitter is
/// wire — bit 3 of one net *is* bit 0 of another — and wire takes no time
/// and cannot echo. Modelled as a component it costs a tick, so bits that
/// cross one arrive after bits that do not. It is here because it is usable
/// today; the destination is for connectivity itself to carry bit offsets,
/// at which point this file goes away.
#[derive(Debug, Default, Clone, Copy)]
pub struct Splitter;

impl Component for Splitter {
    fn reads_own_contribution(&self) -> bool {
        false
    }

    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        let Some((bus, branches)) = inputs.split_first() else {
            return Vec::new();
        };

        // The bus side, assembled from the branches in order. A branch bit
        // past the end of the bus has nowhere to go and is dropped, and a
        // bus bit no branch reaches is left undriven — both are things a
        // drawing may legitimately say.
        let mut assembled: Vec<Level> = branches
            .iter()
            .flat_map(|branch| branch.levels().iter().copied().map(relay))
            .collect();
        assembled.resize(bus.width(), Level::HighZ);

        let mut outputs = vec![Signal::from_levels(assembled)];

        // And each branch, from its own slice of the bus.
        let mut offset = 0;
        for branch in branches {
            outputs.push(Signal::from_levels(
                (offset..offset + branch.width())
                    .map(|bit| bus.levels().get(bit).copied().map_or(Level::HighZ, relay))
                    .collect(),
            ));
            offset += branch.width();
        }
        outputs
    }
}

/// One bit on its way across.
///
/// Nothing to relay means letting go, not driving an unknown: `Unknown` is a
/// contribution and would fight everything else on the net, where `HighZ`
/// steps aside. A fault travels — unlike an absence, `Error` is something to
/// carry rather than something missing.
fn relay(level: Level) -> Level {
    match level {
        Level::Unknown | Level::HighZ => Level::HighZ,
        other => other,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4-bit bus split into two 2-bit branches.
    fn split(bus: Signal, low: Signal, high: Signal) -> Vec<Signal> {
        Splitter.eval(&[bus, low, high])
    }

    fn undriven(width: usize) -> Signal {
        Signal::splat(Level::Unknown, width)
    }

    #[test]
    fn a_driven_bus_reaches_the_branches_lowest_bits_first() {
        // 0b1001: the low branch takes bits 0-1, the high one bits 2-3.
        let bus = Signal::from_levels(vec![Level::High, Level::Low, Level::Low, Level::High]);
        let out = split(bus, undriven(2), undriven(2));

        assert_eq!(out[1].levels(), [Level::High, Level::Low], "bits 0-1");
        assert_eq!(out[2].levels(), [Level::Low, Level::High], "bits 2-3");
        // And it lets the bus alone, since no branch is driving anything
        // back at it.
        assert_eq!(out[0], Signal::splat(Level::HighZ, 4));
    }

    #[test]
    fn driven_branches_reach_the_bus_in_the_same_order() {
        let out = split(
            undriven(4),
            Signal::from_levels(vec![Level::High, Level::Low]),
            Signal::from_levels(vec![Level::Low, Level::High]),
        );

        assert_eq!(
            out[0].levels(),
            [Level::High, Level::Low, Level::Low, Level::High],
        );
        // Nothing on the bus to send back, so it lets the branches alone.
        assert_eq!(out[1], Signal::splat(Level::HighZ, 2));
        assert_eq!(out[2], Signal::splat(Level::HighZ, 2));
    }

    #[test]
    fn a_bit_nobody_drives_is_let_go_of_rather_than_driven_unknown() {
        // The distinction the whole design rests on: `Unknown` is a
        // contribution and would fight whatever else is on the net, where
        // `HighZ` steps aside and lets it through.
        let out = split(undriven(4), undriven(2), undriven(2));
        for signal in &out {
            assert!(
                signal.levels().iter().all(|level| *level == Level::HighZ),
                "expected the splitter to be driving nothing at all: {signal:?}",
            );
        }
    }

    #[test]
    fn one_side_driven_and_the_other_returning_it_is_not_a_conflict() {
        // What the far side reads back is the same value, so the two agree
        // — which is exactly what stops a splitter faulting its own bus
        // once a value has gone round.
        let bus = Signal::from_levels(vec![Level::High, Level::Low, Level::Low, Level::High]);
        let out = split(
            bus.clone(),
            Signal::from_levels(vec![Level::High, Level::Low]),
            Signal::from_levels(vec![Level::Low, Level::High]),
        );
        assert_eq!(out[0], bus);
    }

    #[test]
    fn a_fault_travels_but_an_absence_does_not() {
        let out = split(
            Signal::from_levels(vec![Level::Error, Level::Unknown, Level::Low, Level::High]),
            undriven(2),
            undriven(2),
        );
        assert_eq!(out[1].levels(), [Level::Error, Level::HighZ]);
    }

    #[test]
    fn branches_wider_than_the_bus_lose_the_bits_that_are_not_there() {
        // Three 2-bit branches against a 4-bit bus: the drawing says
        // something that does not fit, and the answer is the four bits that
        // do rather than a panic or a silent widening.
        let out = Splitter.eval(&[
            undriven(4),
            Signal::splat(Level::High, 2),
            Signal::splat(Level::Low, 2),
            Signal::splat(Level::High, 2),
        ]);
        assert_eq!(out[0].width(), 4);
        assert_eq!(
            out[0].levels(),
            [Level::High, Level::High, Level::Low, Level::Low],
        );
        // And the branch hanging off the end reads nothing at all.
        assert_eq!(out[3], Signal::splat(Level::HighZ, 2));
    }

    #[test]
    fn a_splitter_does_not_hear_itself() {
        assert!(!Splitter.reads_own_contribution());
    }
}
