use crate::net::NetId;

/// Whether a `Pin` is an input, an output, or bidirectional (drives the net when
/// active, reads it otherwise — e.g. a tri-state bus transceiver).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output,
    InOut,
}

/// An input or output terminal of a component, connected to a `Net`.
///
/// A `Pin` doesn't carry a `Signal` itself — the `Net` it's connected to does,
/// so that every pin sharing that net observes the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub direction: PinDirection,
    pub net: NetId,
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_on_the_same_net_are_equal_when_same_direction() {
        let a = Pin {
            direction: PinDirection::Output,
            net: NetId(0),
        };
        let b = Pin {
            direction: PinDirection::Output,
            net: NetId(0),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn pin_can_be_bidirectional() {
        let pin = Pin {
            direction: PinDirection::InOut,
            net: NetId(0),
        };
        assert_eq!(pin.direction, PinDirection::InOut);
    }
}
