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
