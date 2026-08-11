use crate::component::{scalar_eval, Component};
use crate::level::Level;
use crate::signal::Signal;

/// Which MOSFET polarity a [`Transistor`] models — determines which gate
/// level makes it conduct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    NType,
    PType,
}

/// A MOSFET transistor, modeled at logic level as a gate-controlled switch:
/// while conducting, it passes its Source signal through to Drain; otherwise
/// it drives `HighZ` (lets some other driver on the net decide).
///
/// # It passes one level well and the other badly
///
/// An NMOS can only pull up through a threshold drop, so the high it
/// delivers is not a full-strength one; a PMOS is the mirror, weak pulling
/// down. That asymmetry is not a detail — it is the entire reason CMOS puts
/// the two in parallel as a transmission gate, and a simulator that let a
/// lone NMOS pass a clean high would make a broken circuit look correct.
///
/// So the weak direction comes out as [`Level::WeakHigh`]/[`Level::WeakLow`],
/// which any full-strength driver on the same net overrides. A transmission
/// gate therefore resolves to a clean level, and a lone pass transistor
/// still works — it just loses to anything else pulling the other way,
/// exactly as in silicon.
///
/// Pins, in the order `Component::eval` expects: `Gate` and `Source` are
/// inputs, `Drain` is the (sole) output.
///
/// # An undriven source is not a level
///
/// A conducting transistor whose source net has no driver reports `HighZ`,
/// not `Unknown`. It used to pass the `Unknown` through, and that broke every
/// structure with transistors in series: in a CMOS NAND with `A` high and `B`
/// low, the lower NMOS is off, so the node between the two is undriven --
/// and the upper NMOS, which *is* conducting, put that `Unknown` onto the
/// output where it met the `High` from the PMOS and resolved to `Error`. A
/// correct gate reported a fault.
///
/// An `Error` on the source still reaches the drain: a fault is a fault, and
/// unlike an absence it is something to carry.
///
/// Simplified from a real transistor: current only ever flows Source -> Drain
/// here, never the other way. That's enough for ordinary switching logic
/// (e.g. a CMOS inverter), but not for a true bidirectional pass-gate — see
/// the separate bidirectional/tri-state buffer already planned in scope for that.
pub struct Transistor {
    polarity: Polarity,
}

impl Transistor {
    /// An N-type (NMOS) transistor: conducts while `Gate` is `High`.
    pub fn n_type() -> Self {
        Self {
            polarity: Polarity::NType,
        }
    }

    /// A P-type (PMOS) transistor: conducts while `Gate` is `Low`.
    pub fn p_type() -> Self {
        Self {
            polarity: Polarity::PType,
        }
    }

    fn conducts(&self, gate: Level) -> bool {
        match self.polarity {
            Polarity::NType => gate == Level::High,
            Polarity::PType => gate == Level::Low,
        }
    }
}

impl Component for Transistor {
    fn eval(&self, inputs: &[Signal], _widths: &[usize]) -> Vec<Signal> {
        scalar_eval(inputs, |inputs| match inputs {
            [gate, source] if self.conducts(*gate) => vec![self.pass(*source)],
            _ => vec![Level::HighZ],
        })
    }
}

impl Transistor {
    /// The level as it arrives at the drain: full strength in the direction
    /// this polarity pulls well, weakened in the other.
    fn pass(&self, source: Level) -> Level {
        // A switch connected to nothing conducts nothing. `Unknown` here
        // means precisely that: it is what a net with no driver resolves to.
        // So the honest answer at the drain is `HighZ` -- "I am not driving
        // either" -- rather than passing the uncertainty on as though it
        // were a level for the net to weigh against a real one.
        if matches!(source, Level::Unknown | Level::HighZ) {
            return Level::HighZ;
        }
        let weak_direction = match self.polarity {
            Polarity::NType => Level::High,
            Polarity::PType => Level::Low,
        };
        if source == weak_direction {
            source.weakened()
        } else {
            source
        }
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
    fn an_n_type_pulls_down_at_full_strength_and_up_only_weakly() {
        let transistor = Transistor::n_type();
        assert_eq!(
            eval_levels(&transistor, &[Level::High, Level::Low]),
            vec![Level::Low],
            "the direction an NMOS pulls well"
        );
        // The threshold drop, and the reason a lone NMOS is not a pass gate.
        assert_eq!(
            eval_levels(&transistor, &[Level::High, Level::High]),
            vec![Level::WeakHigh]
        );
    }

    #[test]
    fn a_p_type_is_the_mirror() {
        let transistor = Transistor::p_type();
        assert_eq!(
            eval_levels(&transistor, &[Level::Low, Level::High]),
            vec![Level::High],
            "the direction a PMOS pulls well"
        );
        assert_eq!(
            eval_levels(&transistor, &[Level::Low, Level::Low]),
            vec![Level::WeakLow]
        );
    }

    #[test]
    fn an_undriven_source_leaves_the_drain_undriven_too() {
        // A switch connected to nothing conducts nothing. Passing the
        // `Unknown` through instead made the transistor claim to drive its
        // drain, and that claim then fought the real driver on the net --
        // which is how a correct CMOS NAND came to report `Error`.
        for source in [Level::Unknown, Level::HighZ] {
            assert_eq!(
                eval_levels(&Transistor::n_type(), &[Level::High, source]),
                vec![Level::HighZ],
                "an n-type conducting from {source:?}"
            );
            assert_eq!(
                eval_levels(&Transistor::p_type(), &[Level::Low, source]),
                vec![Level::HighZ],
                "a p-type conducting from {source:?}"
            );
        }
    }

    #[test]
    fn a_fault_on_the_source_still_reaches_the_drain() {
        // Unlike an absence, a fault is something to carry: weakening
        // applies to a *level*, and there is no such thing as a weak
        // "something is wrong".
        assert_eq!(
            eval_levels(&Transistor::n_type(), &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn n_type_drives_high_z_when_gate_is_low() {
        let transistor = Transistor::n_type();
        assert_eq!(
            eval_levels(&transistor, &[Level::Low, Level::High]),
            vec![Level::HighZ]
        );
    }

    #[test]
    fn p_type_drives_high_z_when_gate_is_high() {
        let transistor = Transistor::p_type();
        assert_eq!(
            eval_levels(&transistor, &[Level::High, Level::Low]),
            vec![Level::HighZ]
        );
    }
}
