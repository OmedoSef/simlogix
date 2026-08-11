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
/// This is what a component looks like until it learns what a bus means
/// *for it* — a gate maps over the bits ([`bitwise_eval`]), a rail drives
/// them all alike, a latch on a bus is a register. Until then, `only_level`
/// answering `Error` for a wider signal is the right refusal: a component
/// that has no meaning on a bus should say so on the wire rather than
/// report its first bit.
pub fn scalar_eval(inputs: &[Signal], eval: impl Fn(&[Level]) -> Vec<Level>) -> Vec<Signal> {
    let levels: Vec<Level> = inputs.iter().map(Signal::only_level).collect();
    eval(&levels).into_iter().map(Signal::bit).collect()
}

/// Adapts a component that means the same thing on every bit: `bit` is
/// handed one level per input and answers the one level of the output.
///
/// This is the whole of what a gate on a bus is — an 8-bit AND is eight AND
/// gates side by side — so the truth tables underneath are untouched and
/// still say exactly what they said when a wire was one bit wide.
///
/// **Inputs of differing widths have no bit-by-bit answer**, and the output
/// is `Error` on every bit of the widest. That case is already ringed on the
/// schematic, since a pin whose declared width disagrees with its net is a
/// reported fault; what matters here is that the gate makes the loudest
/// claim it can rather than quietly answering about the bits that happen to
/// line up.
pub fn bitwise_eval(inputs: &[Signal], bit: impl Fn(&[Level]) -> Level) -> Vec<Signal> {
    let buses: Vec<&Signal> = inputs.iter().collect();
    across_bits(&buses, |levels| vec![bit(levels)])
}

/// Adapts a component whose **data** pins are buses while its **control**
/// pins are single wires: `bit` is handed one level per data pin and answers
/// one level per output, the controls having been read and closed over by
/// the caller.
///
/// This is the shape of anything that gates or steers a bus — a tri-state
/// buffer, a transceiver — and [`bitwise_eval`] cannot express it, since it
/// requires every input to be the same width and a control pin is precisely
/// the input that is not. A control read with [`Signal::only_level`] answers
/// `Error` if it is itself a bus, which every truth table here dominates on,
/// so a wide enable needs no rule of its own.
///
/// **Buses of differing widths have no bit-by-bit answer**, and every output
/// is `Error` on every bit of the widest — the same loud refusal
/// [`bitwise_eval`] makes, for the same reason: the schematic already rings
/// a pin whose declared width disagrees with its net, so what matters is
/// that this makes the loudest claim it can rather than quietly answering
/// about the bits that happen to line up. The table is consulted there only
/// for *how many* outputs there are, which is the one thing it knows and
/// this does not.
pub fn across_bits(buses: &[&Signal], bit: impl Fn(&[Level]) -> Vec<Level>) -> Vec<Signal> {
    let width = buses.iter().map(|bus| bus.width()).max().unwrap_or(0);
    if width == 0 || buses.iter().any(|bus| bus.width() != width) {
        let faulted = vec![Level::Error; buses.len()];
        return bit(&faulted)
            .into_iter()
            .map(|_| Signal::splat(Level::Error, width))
            .collect();
    }
    let per_bit: Vec<Vec<Level>> = (0..width)
        .map(|index| {
            let levels: Vec<Level> = buses.iter().map(|bus| bus.levels()[index]).collect();
            bit(&levels)
        })
        .collect();
    let outputs = per_bit.first().map_or(0, Vec::len);
    (0..outputs)
        .map(|output| {
            Signal::from_levels(
                per_bit
                    .iter()
                    .map(|levels| levels.get(output).copied().unwrap_or(Level::Error))
                    .collect(),
            )
        })
        .collect()
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
    fn a_bitwise_component_answers_as_wide_as_it_was_asked() {
        let a = Signal::from_levels(vec![Level::Low, Level::High, Level::Low]);
        let b = Signal::splat(Level::High, 3);
        assert_eq!(
            bitwise_eval(&[a, b], |bits| match bits {
                [a, b] if *a == Level::High && *b == Level::High => Level::High,
                _ => Level::Low,
            }),
            vec![Signal::from_levels(vec![
                Level::Low,
                Level::High,
                Level::Low
            ])]
        );
    }

    #[test]
    fn inputs_of_differing_widths_have_no_bit_by_bit_answer() {
        // Loud rather than quiet: answering about the bits that happen to
        // line up would hide a mismatch the schematic is already ringing.
        let narrow = Signal::bit(Level::High);
        let wide = Signal::splat(Level::High, 4);
        assert_eq!(
            bitwise_eval(&[narrow, wide], |_| Level::High),
            vec![Signal::splat(Level::Error, 4)]
        );
    }

    #[test]
    fn component_propagation_delay_defaults_to_one_tick() {
        assert_eq!(NotGate.propagation_delay(), 1);
    }
}
