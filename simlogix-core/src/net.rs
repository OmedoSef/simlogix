//! A net: what the drawing says is one conductor.

use crate::circuit::ComponentId;

/// Identifies a `Net` within a `Circuit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetId(pub usize);

/// One net the drawing says exists: the pins it joins, and how many bits it
/// carries.
///
/// Both are *derived from the drawing* and handed over together, because
/// they are answered by the same question — what does this conductor touch.
/// Width could have lived on the pin instead, but a pin's width is only ever
/// what its component's properties say, and the caller reading those already
/// owns the drawing. Keeping the two together also means a net can never be
/// re-grouped without its width being restated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetGroup {
    pub pins: Vec<(ComponentId, usize)>,
    /// How many bits, one for a plain wire.
    pub width: usize,
}

impl NetGroup {
    /// A plain one-bit wire, which is every net until something says wider.
    pub fn wire(pins: Vec<(ComponentId, usize)>) -> Self {
        Self { pins, width: 1 }
    }

    /// A bus of `width` bits.
    pub fn bus(pins: Vec<(ComponentId, usize)>, width: usize) -> Self {
        Self { pins, width }
    }
}
