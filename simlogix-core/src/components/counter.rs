use std::cell::Cell;

use crate::component::Component;
use crate::components::storage::{forced, resize};
use crate::level::Level;
use crate::signal::Signal;

/// Which optional pins a [`Counter`] was given.
///
/// Unlike the flip-flops, a counter cannot read its shape back from how many
/// inputs it is handed: three independent options give combinations with the
/// same count — an enable alone and a direction alone are both one extra pin
/// — so the arity would be ambiguous. It is told instead, once, at
/// construction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CounterPins {
    /// `EN`: it counts only while this is high.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    pub enable: bool,
    /// `LD` and `D`: on an edge with `LD` high it takes `D` instead of
    /// counting. This is what makes a counter a program counter — a jump is
    /// a load.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    pub load: bool,
    /// `UP`: high counts up, low counts down. Without the pin it counts up.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    pub direction: bool,
}

/// A synchronous binary counter: on each active clock edge every bit of `Q`
/// takes its new value at once.
///
/// Pins, in order: the clock, `CLR`, then `EN`, then `LD` and `D`, then `UP`
/// — each only if [`CounterPins`] asked for it — and finally `Q` and `RCO`.
///
/// # Synchronous is the whole point of it being a component
///
/// The next value is computed in one step, where a counter drawn as a chain
/// of flip-flops has to let its carry ripple through every stage. At three
/// bits that is a detail; at thirty-two it is thirty-two ticks against one,
/// and the transient values along the way are real. The ripple *counter* is
/// still worth having and is built by wiring flip-flops together, precisely
/// because the skew is the thing that distinguishes it.
///
/// # `CLR` is not optional, for the reason a T flip-flop's inputs are not
///
/// A counter has no data path unless it was given one: it only ever
/// increments what it already holds, and it starts holding nothing.
/// `Unknown + 1` is `Unknown`, so without a way to force a definite value in
/// it could never leave the unknown state. `CLR` is that way in, and it is
/// asynchronous so a reset line works without a clock running.
///
/// # An uncertain bit makes the whole count uncertain
///
/// Incrementing is not a bitwise operation — a carry reaches every bit above
/// it — so a count with any bit not definite gives `Unknown` on all of them,
/// or `Error` if any bit was faulted. Reporting the low bits as though they
/// were sound would be inventing an answer for a number nobody knows.
pub struct Counter {
    rising: bool,
    pins: CounterPins,
    /// The count, one level per bit. Empty until the first evaluation, which
    /// is the absence of a value rather than one.
    state: Cell<Signal>,
    /// The clock level at the previous evaluation: an edge is a transition,
    /// and nothing that was never observed low can be said to have risen.
    previous_clock: Cell<Level>,
    /// `D` as it was at the previous evaluation, for the same reason a
    /// flip-flop samples — see
    /// [`crate::components::storage::EdgeTriggered`]. A load is a capture.
    previous_data: Cell<Signal>,
}

impl Counter {
    /// Counts on the low-to-high edge.
    pub fn rising(pins: CounterPins) -> Self {
        Self::new(true, pins)
    }

    /// Counts on the high-to-low edge.
    pub fn falling(pins: CounterPins) -> Self {
        Self::new(false, pins)
    }

    fn new(rising: bool, pins: CounterPins) -> Self {
        Self {
            rising,
            pins,
            state: Cell::default(),
            previous_clock: Cell::new(Level::Unknown),
            previous_data: Cell::default(),
        }
    }

    /// How many input pins this counter has, the clock and `CLR` included.
    pub fn input_count(&self) -> usize {
        2 + usize::from(self.pins.enable)
            + 2 * usize::from(self.pins.load)
            + usize::from(self.pins.direction)
    }

    fn edge(&self, clock: Level) -> bool {
        let (before, after) = if self.rising {
            (Level::Low, Level::High)
        } else {
            (Level::High, Level::Low)
        };
        self.previous_clock.get() == before && clock == after
    }
}

/// A signal read as a number, or `None` if any bit is not a definite level.
fn to_value(signal: &Signal) -> Option<u64> {
    let mut value = 0u64;
    for (index, level) in signal.levels().iter().enumerate() {
        match level {
            Level::High if index < 64 => value |= 1 << index,
            Level::High | Level::Low => {}
            _ => return None,
        }
    }
    Some(value)
}

/// A number as a signal `width` bits wide, wrapping rather than saturating —
/// which is what a counter of a fixed width does.
fn from_value(value: u64, width: usize) -> Signal {
    Signal::from_levels(
        (0..width)
            .map(|bit| {
                if bit < 64 && value & (1 << bit) != 0 {
                    Level::High
                } else {
                    Level::Low
                }
            })
            .collect(),
    )
}

impl Component for Counter {
    fn eval(&self, inputs: &[Signal], widths: &[usize]) -> Vec<Signal> {
        // How wide it is comes from `Q`, since the count is what the width
        // is *about* — and a counter with no data pin has nothing else to
        // read it from.
        let width = widths.first().copied().unwrap_or(1).max(1);
        if inputs.len() != self.input_count() {
            return vec![
                Signal::splat(Level::Unknown, width),
                Signal::bit(Level::Unknown),
            ];
        }

        let clock = inputs[0].only_level();
        let clear = inputs[1].only_level();
        let mut next_input = 2;
        let mut take = |wide: bool| {
            let signal = &inputs[next_input];
            next_input += 1;
            if wide {
                signal.clone()
            } else {
                Signal::bit(signal.only_level())
            }
        };
        let enable = self.pins.enable.then(|| take(false).only_level());
        let load = self.pins.load.then(|| take(false).only_level());
        let data = self.pins.load.then(|| take(true));
        let up = self.pins.direction.then(|| take(false).only_level());

        let held = resize(self.state.take(), width);
        let sampled = resize(
            self.previous_data.take(),
            data.as_ref().map_or(width, Signal::width),
        );

        let next = next_count(
            clock,
            clear,
            enable,
            load,
            &sampled,
            up,
            &held,
            self.edge(clock),
            width,
        );

        self.previous_clock.set(clock);
        if let Some(data) = data {
            self.previous_data.set(data);
        }
        self.state.set(next.clone());

        // `RCO` is high at the value the counter is about to roll over from:
        // the top going up, zero going down. That is what a carry out into
        // the next counter along has to mean.
        let terminal = match (to_value(&next), up) {
            (Some(value), Some(Level::Low)) => value == 0,
            (Some(value), _) => value == all_ones(width),
            (None, _) => return vec![next, Signal::bit(Level::Unknown)],
        };
        let carry = if terminal { Level::High } else { Level::Low };
        vec![next, Signal::bit(carry)]
    }
}

fn all_ones(width: usize) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1 << width) - 1
    }
}

/// The counter's whole truth table, in one place so the reasons sit together.
#[allow(clippy::too_many_arguments)]
fn next_count(
    clock: Level,
    clear: Level,
    enable: Option<Level>,
    load: Option<Level>,
    sampled: &Signal,
    up: Option<Level>,
    held: &Signal,
    edge: bool,
    width: usize,
) -> Signal {
    // Asynchronous, so it wins over everything including a faulted clock: a
    // part being held clear is held clear whatever its clock is doing.
    match clear {
        Level::High => return Signal::splat(Level::Low, width),
        Level::Error => return Signal::splat(Level::Error, width),
        Level::Low => {}
        // Undriven, or not known yet: whether it is being held clear is
        // unknown, and so is what it holds.
        _ => return Signal::splat(Level::Unknown, width),
    }
    if let Some(level) = forced(clock, None, None) {
        return Signal::splat(level, width);
    }
    if !edge {
        return held.clone();
    }
    // A control that is not definite says nothing about what happens on this
    // edge, so nothing definite can be said about the count afterwards.
    for control in [enable, load, up].into_iter().flatten() {
        if !matches!(control, Level::High | Level::Low) {
            let level = if control == Level::Error {
                Level::Error
            } else {
                Level::Unknown
            };
            return Signal::splat(level, width);
        }
    }
    if load == Some(Level::High) {
        // A load is a capture, so it takes `D` as it stood *before* the edge
        // — the same setup time a flip-flop has, and for the same reason.
        return resize(sampled.clone(), width);
    }
    if enable == Some(Level::Low) {
        return held.clone();
    }
    let Some(value) = to_value(held) else {
        // Incrementing is not bitwise: a carry reaches every bit above it, so
        // one uncertain bit makes the whole count uncertain.
        let faulted = held.levels().contains(&Level::Error);
        let level = if faulted {
            Level::Error
        } else {
            Level::Unknown
        };
        return Signal::splat(level, width);
    };
    let step = if up == Some(Level::Low) {
        value.wrapping_sub(1)
    } else {
        value.wrapping_add(1)
    };
    from_value(step & all_ones(width), width)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 3;

    struct Bench {
        counter: Counter,
        pins: CounterPins,
    }

    impl Bench {
        fn new(pins: CounterPins) -> Self {
            Self {
                counter: Counter::rising(pins),
                pins,
            }
        }

        /// One evaluation. `controls` is `(clear, enable, load, data, up)`,
        /// each ignored when the counter has no such pin.
        fn eval(&self, clock: Level, controls: Controls) -> Vec<Signal> {
            let mut inputs = vec![Signal::bit(clock), Signal::bit(controls.clear)];
            if self.pins.enable {
                inputs.push(Signal::bit(controls.enable));
            }
            if self.pins.load {
                inputs.push(Signal::bit(controls.load));
                inputs.push(from_value(controls.data, WIDTH));
            }
            if self.pins.direction {
                inputs.push(Signal::bit(controls.up));
            }
            self.counter.eval(&inputs, &[WIDTH, 1])
        }

        /// The count after a whole clock cycle.
        fn pulse(&self, controls: Controls) -> Option<u64> {
            self.eval(Level::High, controls);
            self.eval(Level::Low, controls);
            to_value(&self.eval(Level::Low, controls)[0])
        }

        fn clear(&self) {
            self.eval(
                Level::Low,
                Controls {
                    clear: Level::High,
                    ..Controls::default()
                },
            );
            self.eval(Level::Low, Controls::default());
        }
    }

    #[derive(Clone, Copy)]
    struct Controls {
        clear: Level,
        enable: Level,
        load: Level,
        data: u64,
        up: Level,
    }

    impl Default for Controls {
        fn default() -> Self {
            Self {
                clear: Level::Low,
                enable: Level::High,
                load: Level::Low,
                data: 0,
                up: Level::High,
            }
        }
    }

    #[test]
    fn it_counts_up_and_wraps() {
        let bench = Bench::new(CounterPins::default());
        bench.clear();
        for expected in [1, 2, 3, 4, 5, 6, 7, 0, 1] {
            assert_eq!(bench.pulse(Controls::default()), Some(expected));
        }
    }

    #[test]
    fn it_holds_nothing_until_it_is_cleared() {
        // No data path, so it starts unable to say anything — the same
        // reason a T flip-flop's asynchronous inputs are not optional.
        let bench = Bench::new(CounterPins::default());
        assert_eq!(bench.pulse(Controls::default()), None);
        bench.clear();
        assert_eq!(bench.pulse(Controls::default()), Some(1));
    }

    #[test]
    fn the_clear_needs_no_clock() {
        let bench = Bench::new(CounterPins::default());
        bench.clear();
        for _ in 0..5 {
            bench.pulse(Controls::default());
        }
        let cleared = bench.eval(
            Level::Low,
            Controls {
                clear: Level::High,
                ..Controls::default()
            },
        );
        assert_eq!(to_value(&cleared[0]), Some(0), "no edge anywhere in that");
    }

    #[test]
    fn the_enable_freezes_it_without_losing_the_count() {
        let bench = Bench::new(CounterPins {
            enable: true,
            ..CounterPins::default()
        });
        bench.clear();
        bench.pulse(Controls::default());
        bench.pulse(Controls::default());
        let frozen = Controls {
            enable: Level::Low,
            ..Controls::default()
        };
        for _ in 0..3 {
            assert_eq!(bench.pulse(frozen), Some(2), "edges, and no counting");
        }
        assert_eq!(bench.pulse(Controls::default()), Some(3), "and it resumes");
    }

    #[test]
    fn a_load_takes_the_value_instead_of_counting() {
        let bench = Bench::new(CounterPins {
            load: true,
            ..CounterPins::default()
        });
        bench.clear();
        let jump = Controls {
            load: Level::High,
            data: 5,
            ..Controls::default()
        };
        // Twice: `D` has to have settled before the edge, which is the same
        // setup time a flip-flop has.
        bench.eval(Level::Low, jump);
        assert_eq!(bench.pulse(jump), Some(5));
        assert_eq!(bench.pulse(Controls::default()), Some(6), "then counts on");
    }

    #[test]
    fn it_counts_down_when_told_to() {
        let bench = Bench::new(CounterPins {
            direction: true,
            ..CounterPins::default()
        });
        bench.clear();
        let down = Controls {
            up: Level::Low,
            ..Controls::default()
        };
        // Zero going down wraps to the top, which is what a fixed width does.
        for expected in [7, 6, 5] {
            assert_eq!(bench.pulse(down), Some(expected));
        }
        assert_eq!(bench.pulse(Controls::default()), Some(6), "and back up");
    }

    #[test]
    fn the_carry_out_marks_the_value_it_rolls_over_from() {
        let bench = Bench::new(CounterPins::default());
        bench.clear();
        for expected in 1..=6 {
            let out = bench.pulse(Controls::default());
            assert_eq!(out, Some(expected));
            assert_eq!(
                bench.eval(Level::Low, Controls::default())[1].only_level(),
                Level::Low
            );
        }
        assert_eq!(bench.pulse(Controls::default()), Some(7));
        assert_eq!(
            bench.eval(Level::Low, Controls::default())[1].only_level(),
            Level::High,
            "at the top, so the next counter along may step"
        );
    }

    #[test]
    fn an_uncertain_control_leaves_the_count_uncertain() {
        let bench = Bench::new(CounterPins {
            enable: true,
            ..CounterPins::default()
        });
        bench.clear();
        let vague = Controls {
            enable: Level::Unknown,
            ..Controls::default()
        };
        assert_eq!(
            bench.pulse(vague),
            None,
            "nobody knows whether it counted, so nobody knows what it holds"
        );
    }

    #[test]
    fn nothing_happens_between_edges() {
        let bench = Bench::new(CounterPins::default());
        bench.clear();
        for _ in 0..4 {
            assert_eq!(
                to_value(&bench.eval(Level::Low, Controls::default())[0]),
                Some(0)
            );
        }
    }
}
