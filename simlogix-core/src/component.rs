use crate::level::Level;
use crate::signal::Signal;

/// A circuit element that computes its output signals from its input signals.
///
/// Both plain gates and sub-circuits implement this trait, so hierarchy
/// (a circuit reused as a component inside another circuit) is not a special case.
pub trait Component {
    /// Compute output signals from the given input signals.
    ///
    /// One `Signal` per pin, each carrying one level per bit. A component
    /// with no meaning on a bus reads its inputs with
    /// [`Signal::only_level`], which answers `Error` for anything but a
    /// plain wire rather than quietly reporting bit 0.
    fn eval(&self, inputs: &[Signal]) -> Vec<Signal>;

    /// Delay, in logical ticks, between an input change and the resulting output change.
    /// Defaults to 1 tick.
    fn propagation_delay(&self) -> u64 {
        1
    }
}

/// Adapts a component written against single levels to the `Signal`
/// boundary: every input read with [`Signal::only_level`], every output a
/// plain one-bit wire.
///
/// Every component is like this today, which is why this exists rather than
/// the conversion being written out eighteen times. It stops being true one
/// component at a time, as each learns what a bus means *for it* — a gate
/// maps over the bits, a rail drives them all alike, a latch on a bus is a
/// register. Until then, `only_level` answering `Error` for a wider signal
/// is the right refusal: a component that has no meaning on a bus should
/// say so on the wire rather than report its first bit.
pub fn scalar_eval(inputs: &[Signal], eval: impl Fn(&[Level]) -> Vec<Level>) -> Vec<Signal> {
    let levels: Vec<Level> = inputs.iter().map(Signal::only_level).collect();
    eval(&levels).into_iter().map(Signal::bit).collect()
}

/// Evaluates a component that is scalar on both sides, in levels.
///
/// The shape every test in this crate is written in, kept so those tests
/// still say exactly what they said before a signal had a width. They are
/// the evidence that nothing changed meaning when it gained one, and
/// rewriting each expectation by hand is precisely how that evidence would
/// have been lost.
#[cfg(test)]
pub(crate) fn eval_levels(component: &dyn Component, inputs: &[Level]) -> Vec<Level> {
    let inputs: Vec<Signal> = inputs.iter().copied().map(Signal::bit).collect();
    component
        .eval(&inputs)
        .iter()
        .map(Signal::only_level)
        .collect()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::eval_levels;

    struct NotGate;

    impl Component for NotGate {
        fn eval(&self, inputs: &[Signal]) -> Vec<Signal> {
            let level = match inputs {
                [only] => match only.only_level() {
                    Level::High => Level::Low,
                    Level::Low => Level::High,
                    _ => Level::Unknown,
                },
                _ => Level::Unknown,
            };
            vec![Signal::bit(level)]
        }
    }

    #[test]
    fn component_eval_computes_outputs_from_inputs() {
        assert_eq!(eval_levels(&NotGate, &[Level::High]), vec![Level::Low]);
        assert_eq!(eval_levels(&NotGate, &[Level::Low]), vec![Level::High]);
    }

    #[test]
    fn component_propagation_delay_defaults_to_one_tick() {
        assert_eq!(NotGate.propagation_delay(), 1);
    }
}
