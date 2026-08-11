use crate::component::{across_bits, Component};
use crate::level::Level;
use crate::signal::Signal;

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
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
        match inputs {
            [data, enable] => {
                // One enable governs the whole bus, however wide it is: the
                // buffer passes each bit through or lets go of all of them.
                let enable = enable.only_level();
                across_bits(&[data], |bits| match bits {
                    [data] => vec![gated(*data, enable)],
                    _ => vec![Level::Unknown],
                })
            }
            _ => vec![Signal::bit(Level::Unknown)],
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
    use crate::component::eval_levels;

    #[test]
    fn it_passes_its_input_through_while_enabled() {
        assert_eq!(
            eval_levels(&TriStateBuffer, &[Level::High, Level::High]),
            vec![Level::High]
        );
        assert_eq!(
            eval_levels(&TriStateBuffer, &[Level::Low, Level::High]),
            vec![Level::Low]
        );
    }

    #[test]
    fn it_stops_driving_when_disabled_whatever_its_input_says() {
        for data in [Level::High, Level::Low, Level::Unknown, Level::Error] {
            assert_eq!(
                eval_levels(&TriStateBuffer, &[data, Level::Low]),
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
            eval_levels(&TriStateBuffer, &[Level::High, Level::Unknown]),
            vec![Level::Unknown]
        );
        assert_eq!(
            eval_levels(&TriStateBuffer, &[Level::High, Level::HighZ]),
            vec![Level::Unknown]
        );
    }

    #[test]
    fn a_faulted_enable_dominates() {
        assert_eq!(
            eval_levels(&TriStateBuffer, &[Level::High, Level::Error]),
            vec![Level::Error]
        );
    }

    #[test]
    fn a_faulted_input_passes_through_while_enabled() {
        assert_eq!(
            eval_levels(&TriStateBuffer, &[Level::Error, Level::High]),
            vec![Level::Error]
        );
    }

    #[test]
    fn it_passes_a_bus_through_bit_by_bit() {
        // Mixed bits on purpose: all-alike values would pass just as well
        // against a buffer that only ever looked at bit 0.
        let data = Signal::from_levels(vec![Level::Low, Level::High, Level::High, Level::Low]);
        assert_eq!(
            TriStateBuffer.eval(&[data.clone(), Signal::bit(Level::High)]),
            vec![data]
        );
    }

    #[test]
    fn a_disabled_buffer_lets_go_of_every_bit() {
        assert_eq!(
            TriStateBuffer.eval(&[Signal::splat(Level::High, 4), Signal::bit(Level::Low)]),
            vec![Signal::splat(Level::HighZ, 4)]
        );
    }

    #[test]
    fn one_enable_governs_the_whole_bus() {
        // The enable is one wire whatever the data is wide, which is the
        // shape `bitwise_eval` cannot express: it wants every input the
        // same width.
        for enable in [Level::Unknown, Level::HighZ] {
            assert_eq!(
                TriStateBuffer.eval(&[Signal::splat(Level::High, 3), Signal::bit(enable)]),
                vec![Signal::splat(Level::Unknown, 3)],
                "an enable that is merely {enable:?} leaves every bit unknown"
            );
        }
        assert_eq!(
            TriStateBuffer.eval(&[Signal::splat(Level::High, 3), Signal::bit(Level::Error)]),
            vec![Signal::splat(Level::Error, 3)]
        );
    }

    #[test]
    fn an_enable_that_is_itself_a_bus_faults_the_whole_output() {
        // Nothing about a wide enable says which bit switches the buffer on,
        // and `only_level` is what refuses to guess.
        assert_eq!(
            TriStateBuffer.eval(&[Signal::splat(Level::High, 3), Signal::splat(Level::High, 3)]),
            vec![Signal::splat(Level::Error, 3)]
        );
    }
}
