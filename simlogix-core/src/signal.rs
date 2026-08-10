//! What a wire carries: one [`Level`] per bit.
//!
//! A plain wire is a signal of width one, which is what every wire in every
//! project is until something says otherwise. A bus is the same thing wider,
//! so nothing in the engine has a scalar case and a vector case — there is
//! one shape, and the truth tables underneath it still work on a single
//! level at a time.
//!
//! **Bit 0 is the least significant.** Fixed here, once, because a splitter
//! has to say which bits go where and that convention is expensive to leave
//! implicit.

use crate::level::Level;

/// The state of a wire: one level per bit, least significant first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal(Vec<Level>);

impl Signal {
    /// A one-bit signal — what a plain wire carries.
    pub fn bit(level: Level) -> Self {
        Self(vec![level])
    }

    /// A signal of the given levels, least significant first.
    pub fn from_levels(levels: Vec<Level>) -> Self {
        Self(levels)
    }

    /// `width` bits, all the same.
    pub fn splat(level: Level, width: usize) -> Self {
        Self(vec![level; width])
    }

    /// How many bits. Zero is possible and means "nothing has said yet",
    /// which is what an undriven net starts as.
    pub fn width(&self) -> usize {
        self.0.len()
    }

    pub fn levels(&self) -> &[Level] {
        &self.0
    }

    /// The level of a **one-bit** signal, and [`Level::Error`] for any other
    /// width.
    ///
    /// For the components that have no meaning on a bus. Answering about the
    /// first bit of a wider signal would be a quiet lie, where `Error` is
    /// exactly the right claim — this is wrong, and it shows on the wire.
    pub fn only_level(&self) -> Level {
        match self.0.as_slice() {
            [level] => *level,
            _ => Level::Error,
        }
    }

    /// The same signal with `f` applied to every bit — a gate on a bus is
    /// the same gate applied bit by bit.
    pub fn map(&self, f: impl Fn(Level) -> Level) -> Self {
        Self(self.0.iter().copied().map(f).collect())
    }

    /// Whether any bit is held up only by a weak contribution.
    pub fn is_weak(&self) -> bool {
        self.0.iter().any(|level| level.is_weak())
    }
}

impl From<Level> for Signal {
    fn from(level: Level) -> Self {
        Self::bit(level)
    }
}

impl Default for Signal {
    /// One bit, undriven — what a wire carries before anything has said
    /// anything about it.
    fn default() -> Self {
        Self::bit(Level::Unknown)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_wire_is_one_bit() {
        let signal = Signal::bit(Level::High);
        assert_eq!(signal.width(), 1);
        assert_eq!(signal.only_level(), Level::High);
    }

    #[test]
    fn asking_a_bus_for_one_level_is_an_error_rather_than_its_first_bit() {
        // The component asking has no meaning on a bus, so answering about
        // bit 0 would be a quiet lie. `Error` says it out loud, and shows
        // on the wire.
        let bus = Signal::from_levels(vec![Level::High, Level::Low]);
        assert_eq!(bus.only_level(), Level::Error);

        // And a signal of no bits at all is not a level either.
        assert_eq!(Signal::from_levels(Vec::new()).only_level(), Level::Error);
    }

    #[test]
    fn mapping_applies_bit_by_bit() {
        let bus = Signal::from_levels(vec![Level::High, Level::Low]);
        assert_eq!(
            bus.map(|level| match level {
                Level::High => Level::Low,
                Level::Low => Level::High,
                other => other,
            })
            .levels(),
            [Level::Low, Level::High]
        );
    }
}
