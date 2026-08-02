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
            [gate, source] if self.conducts(*gate) => vec![*source],
            [_, _] => vec![Signal::HighZ],
            _ => vec![Signal::HighZ],
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
    fn n_type_passes_source_through_when_gate_is_high() {
        let transistor = Transistor::n_type();
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::Low]),
            vec![Signal::Low]
        );
        assert_eq!(
            transistor.eval(&[Signal::High, Signal::High]),
            vec![Signal::High]
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
    fn p_type_passes_source_through_when_gate_is_low() {
        let transistor = Transistor::p_type();
        assert_eq!(
            transistor.eval(&[Signal::Low, Signal::High]),
            vec![Signal::High]
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
