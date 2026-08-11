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
    pub members: Vec<Member>,
    /// How many bits, one for a plain wire.
    pub width: usize,
}

/// One pin on a net, and **which of the net's bits it occupies**.
///
/// A plain wire puts every pin at offset zero across the whole width — that
/// is what a conductor means. A pin joins at an *offset* only because
/// something in the drawing said so, which today is a splitter: it is what
/// makes "bit 3 of this net is bit 0 of that one" expressible without a
/// component in between to relay it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub component: ComponentId,
    pub pin: usize,
    /// Which bit of the net this pin's bit zero is.
    pub offset: usize,
    /// How many bits it occupies, or `None` for all of them from `offset`.
    ///
    /// `None` rather than the net's width copied in: a pin that spans the
    /// whole conductor should not have to be told the width twice, and a
    /// copy is a thing that can disagree.
    pub width: Option<usize>,
}

impl Member {
    /// A pin occupying the whole net, which is every pin until a splitter
    /// says otherwise.
    pub fn whole(pin: (ComponentId, usize)) -> Self {
        Self {
            component: pin.0,
            pin: pin.1,
            offset: 0,
            width: None,
        }
    }

    /// A pin occupying `width` bits from `offset`.
    pub fn slice(pin: (ComponentId, usize), offset: usize, width: usize) -> Self {
        Self {
            component: pin.0,
            pin: pin.1,
            offset,
            width: Some(width),
        }
    }

    pub fn key(&self) -> (ComponentId, usize) {
        (self.component, self.pin)
    }
}

impl NetGroup {
    /// A plain one-bit wire, which is every net until something says wider.
    pub fn wire(pins: Vec<(ComponentId, usize)>) -> Self {
        Self::bus(pins, 1)
    }

    /// A bus of `width` bits, every pin spanning all of it.
    pub fn bus(pins: Vec<(ComponentId, usize)>, width: usize) -> Self {
        Self {
            members: pins.into_iter().map(Member::whole).collect(),
            width,
        }
    }

    /// A net whose pins occupy different parts of it — what a splitter
    /// makes of a bus.
    pub fn sliced(members: Vec<Member>, width: usize) -> Self {
        Self { members, width }
    }

    /// Just which pins are on it, for callers that don't care where.
    pub fn pins(&self) -> impl Iterator<Item = (ComponentId, usize)> + '_ {
        self.members.iter().map(Member::key)
    }
}
