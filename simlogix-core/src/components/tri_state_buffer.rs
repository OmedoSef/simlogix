use crate::component::Component;
use crate::level::Level;

/// A buffer with an enable: it passes its data input through while enabled,
/// and stops driving altogether when it isn't.
///
/// This is the component the whole `HighZ` state was designed for. Every
/// other component drives its output all the time, so a net has exactly one
/// source; disabling this one leaves the net to whatever *else* is on it,
/// which is what makes a shared bus possible — see `Circuit::resolve` for
/// the rule that combines several drivers.
///
/// `HighZ` is deliberately not the same as `Unknown`: "I am choosing not to
/// drive" and "nothing is known here" resolve differently when several
/// drivers meet.
#[derive(Debug, Default, Clone, Copy)]
pub struct TriStateBuffer;

impl Component for TriStateBuffer {
    fn eval(&self, inputs: &[Level]) -> Vec<Level> {
        match inputs {
            [data, enable] => vec![gated(*data, *enable)],
            _ => vec![Level::Unknown],
        }
    }
}

fn gated(data: Level, enable: Level) -> Level {
    match enable {
        Level::High => data,
        Level::Low => Level::HighZ,
        // A faulted enable is a faulted output — the same dominance the
        // gates use, and the alternative would be to quietly pick one of
        // "driving" or "not driving" and hide the fault.
        Level::Error => Level::Error,
        // Neither driven nor known. Answering `HighZ` would claim this
        // buffer is deliberately off, which is a stronger statement than
        // the truth: nobody knows whether it's on.
        _ => Level::Unknown,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_passes_its_input_through_while_enabled() {
        assert_eq!(
            TriStateBuffer.eval(&[Level::High, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            TriStateBuffer.eval(&[Level::Low, Level::High]),
            vec![Level::Low]
        );
    }

    #[test]
    fn it_stops_driving_when_disabled_whatever_its_input_says() {
        for data in [Level::High, Level::Low, Level::Unknown, Level::Error] {
            assert_eq!(
                TriStateBuffer.eval(&[data, Level::Low]),
                vec![Level::HighZ],
                "a disabled buffer drives nothing, even with {data:?} on its input"
            );
        }
    }

    #[test]
    fn an_enable_that_is_merely_unknown_does_not_count_as_switched_off() {
        // `HighZ` here would assert that the buffer is deliberately off,
        // which would let the rest of the bus resolve as if it were absent.
        assert_eq!(
            TriStateBuffer.eval(&[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            TriStateBuffer.eval(&[Level::High, Level::HighZ]),
            vec![Level::Unknown]
        );
    }

    #[test]
    fn a_faulted_enable_dominates() {
        assert_eq!(
            TriStateBuffer.eval(&[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn a_faulted_input_passes_through_while_enabled() {
        assert_eq!(
            TriStateBuffer.eval(&[Level::Error, Level::High]),
            vec![Level::Error]
        );
    }
}
