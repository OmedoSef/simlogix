/// The electrical state carried by a wire (`Pin`/`Net`) at a point in simulated time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Signal {
    /// Logic level 1.
    High,
    /// Logic level 0.
    Low,
    /// Indeterminate state (e.g. an unconnected input, or a net not simulated yet) — the "X" of HDL simulators.
    #[default]
    Unknown,
    /// Invalid/conflicting state (e.g. two outputs driving the same net with different values).
    Error,
    /// High impedance: this driver is deliberately not driving the net right now
    /// (e.g. a disabled tri-state buffer), leaving room for another driver to.
    HighZ,
    /// A `High` that a stronger driver overrides — what an NMOS pass
    /// transistor puts out when it passes a high level, which it can only do
    /// through a threshold drop.
    ///
    /// **Only ever appears as a driver's contribution, never as a net's
    /// resolved value**: [`crate::Circuit`] normalises it away, so no
    /// component ever receives one and no truth table has to know it exists.
    /// That containment is what makes modelling strength cheap here.
    WeakHigh,
    /// A `Low` that a stronger driver overrides — a PMOS passing a low
    /// level. See [`Signal::WeakHigh`].
    WeakLow,
}

impl Signal {
    /// Whether this contribution yields to a stronger one on the same net.
    pub fn is_weak(self) -> bool {
        matches!(self, Signal::WeakHigh | Signal::WeakLow)
    }

    /// The full-strength level a weak one stands for; anything else is
    /// already itself.
    pub fn strengthened(self) -> Self {
        match self {
            Signal::WeakHigh => Signal::High,
            Signal::WeakLow => Signal::Low,
            other => other,
        }
    }

    /// The weakened form of a level, as a pass transistor delivers it.
    pub fn weakened(self) -> Self {
        match self {
            Signal::High => Signal::WeakHigh,
            Signal::Low => Signal::WeakLow,
            other => other,
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
    fn signal_defaults_to_unknown() {
        assert_eq!(Signal::default(), Signal::Unknown);
    }

    #[test]
    fn high_z_is_distinct_from_unknown() {
        assert_ne!(Signal::HighZ, Signal::Unknown);
    }
}
