# Simulation engine

> **Status: planned, not yet implemented.** This document describes the design decision as agreed, not existing code — see [overview.md](overview.md#current-status) for what's actually built.

## The problem it solves

Logisim's simulator struggles with feedback loops in combinational logic — for example an SR latch built from cross-coupled NAND gates, or a ring oscillator. Naively re-evaluating a circuit until it "settles" can recurse forever or lock up the UI when the circuit doesn't converge.

## Discrete-event simulation with propagation delay

SimLogix's engine is a discrete-event simulator, the same family of technique used by HDL simulators (e.g. Verilog):

- Every component has a **propagation delay** (defaults to 1 logical tick).
- A change on a component's input doesn't instantly update its output. Instead, it schedules an output-change event at `t + delay`.
- Events are kept in a queue ordered by logical time and processed in that order.

Because outputs only ever change in response to a scheduled future event, a feedback loop can't cause infinite synchronous recursion — an SR-NAND latch settles into a stable state after a few ticks, and a ring oscillator oscillates (as real hardware would) instead of hanging the simulator.

### Oscillation detection

If a net changes state more than a threshold number of times within the same logical time step, the engine stops and reports "unstable circuit" instead of freezing the UI.

## Data model

- **`Signal`** — `High`, `Low`, `Unknown`, or `Error`. Unknown/error states are modeled from the start, since mishandling X/Z-like states is a common source of subtle simulation bugs.
- **`Pin`** — an input or output of a component, connected to a `Net`.
- **`Component`** — a trait with `eval(&self, inputs) -> outputs` and `propagation_delay()`. Sub-circuits implement the same trait, so hierarchy (a circuit used as a component inside another circuit) is a first-class concept rather than a special case.
- **`Circuit`** — the graph of components and nets, plus the event queue and logical clock.
