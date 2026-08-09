use crate::component::Component;
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
/// So the weak direction comes out as [`Signal::WeakHigh`]/[`Signal::WeakLow`],
/// which any full-strength driver on the same net overrides. A transmission
/// gate therefore resolves to a clean level, and a lone pass transistor
/// still works — it just loses to anything else pulling the other way,
/// exactly as in silicon.
///
/// Pins, in the order `Component::eval` expects: `Gate` and `Source` are
/// inputs, `Drain` is the (sole) output.
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

    fn conducts(&self, gate: Signal) -> bool {
        match self.polarity {
            Polarity::NType => gate == Signal::High,
            Polarity::PType => gate == Signal::Low,
        }
    }
}

impl Component for Transistor {
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [gate, source] if self.conducts(*gate) => vec![self.pass(*source)],
            _ => vec![Signal::HighZ],
        }
    }
}

impl Transistor {
    /// The level as it arrives at the drain: full strength in the direction
    /// this polarity pulls well, weakened in the other.
    fn pass(&self, source: Signal) -> Signal {
        let weak_direction = match self.polarity {
            Polarity::NType => Signal::High,
            Polarity::PType => Signal::Low,
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

    #[test]
    fn an_n_type_pulls_down_at_full_strength_and_up_only_weakly() {
        let transistor = Transistor::n_type();
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::Low]),
            vec![Signal::Low],
            "the direction an NMOS pulls well"
        );
        // The threshold drop, and the reason a lone NMOS is not a pass gate.
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::High]),
            vec![Signal::WeakHigh]
        );
    }

    #[test]
    fn a_p_type_is_the_mirror() {
        let transistor = Transistor::p_type();
        assert_eq!(
            transistor.eval(&[Signal::Low, Signal::High]),
            vec![Signal::High],
            "the direction a PMOS pulls well"
        );
        assert_eq!(
            transistor.eval(&[Signal::Low, Signal::Low]),
            vec![Signal::WeakLow]
        );
    }

    #[test]
    fn uncertainty_passes_through_unweakened() {
        // Weakening applies to a *level*; there is no such thing as a weak
        // "don't know".
        let transistor = Transistor::n_type();
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::Unknown]),
            vec![Signal::Unknown]
        );
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::Error]),
            vec![Signal::Error]
        );
    }

    #[test]
    fn n_type_drives_high_z_when_gate_is_low() {
        let transistor = Transistor::n_type();
        assert_eq!(
            transistor.eval(&[Signal::Low, Signal::High]),
            vec![Signal::HighZ]
        );
    }

    #[test]
    fn p_type_drives_high_z_when_gate_is_high() {
        let transistor = Transistor::p_type();
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::Low]),
            vec![Signal::HighZ]
        );
    }
}
