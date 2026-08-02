# Simulation engine

> **Status: implemented in `simlogix-core::Circuit`** (`schedule_now`/`run`), exercised end to end by `Button`/`Led`. Not yet wired to the GUI — see [overview.md](overview.md#current-status) for what's actually built.

## The problem it solves

Logisim's simulator struggles with feedback loops in combinational logic — for example an SR latch built from cross-coupled NAND gates, or a ring oscillator. Naively re-evaluating a circuit until it "settles" can recurse forever or lock up the UI when the circuit doesn't converge.

## Discrete-event simulation with propagation delay

SimLogix's engine is a discrete-event simulator, the same family of technique used by HDL simulators (e.g. Verilog):

- Every component has a **propagation delay** (defaults to 1 logical tick).
- A change on a component's input doesn't instantly update its output. Instead, it schedules that component's own re-evaluation at `t + delay` — the delay belongs to the component whose input changed (the reader), not the one that drove the change.
- Events are kept in a queue ordered by logical time and processed in that order (`Circuit::run`).

Because outputs only ever change in response to a scheduled future event, a feedback loop can't cause infinite synchronous recursion — an SR-NAND latch settles into a stable state after a few ticks, and a ring oscillator oscillates (as real hardware would) instead of hanging the simulator.

### Oscillation detection

If a net changes state more than a threshold number of times within a single `run()` call (currently 1,000), the engine stops and returns an `UnstableCircuit` error instead of looping forever. Verified with a unit test: a single NOT gate wired back to its own input never settles and is correctly reported as unstable.

## Data model

- **`Signal`** — `High`, `Low`, `Unknown`, `Error`, or `HighZ`. Unknown/error states are modeled from the start, since mishandling X/Z-like states is a common source of subtle simulation bugs. `HighZ` is distinct from `Unknown`: `Unknown` means "not driven / not simulated yet", `HighZ` means "this driver is deliberately not driving right now" (tri-state), needed for bidirectional buffers.
- **`Pin`** — an input, output, or bidirectional (`PinDirection::InOut`) terminal of a component, connected to a `Net`. A bidirectional pin drives the net when active and reads it otherwise (e.g. a bus transceiver).
- **`Component`** — a trait with `eval(&self, inputs) -> outputs` and `propagation_delay()`. Sub-circuits implement the same trait, so hierarchy (a circuit used as a component inside another circuit) is a first-class concept rather than a special case.
- **`Circuit`** — the graph of components and nets, plus the event queue and logical clock. A `Net` can have more than one potential driver (to support tri-state buffers); resolving its `Signal` ignores `HighZ` drivers, then: 0 remaining → `Unknown`, 1 → that value, ≥2 differing → `Error`.
