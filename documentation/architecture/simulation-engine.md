# Simulation engine

> **Status: implemented in `simlogix-core::Circuit`** (`schedule_now`/`schedule_periodic`/`advance`/`run`) and wired into the GUI's real-time frame loop — see [overview.md](overview.md#current-status) for what's actually built.

## The problem it solves

Logisim's simulator struggles with feedback loops in combinational logic — for example an SR latch built from cross-coupled NAND gates, or a ring oscillator. Naively re-evaluating a circuit until it "settles" can recurse forever or lock up the UI when the circuit doesn't converge.

## Discrete-event simulation with propagation delay

SimLogix's engine is a discrete-event simulator, the same family of technique used by HDL simulators (e.g. Verilog):

- Every component has a **propagation delay** (defaults to 1 logical tick).
- A change on a component's input doesn't instantly update its output. Instead, it schedules that component's own re-evaluation at `t + delay` — the delay belongs to the component whose input changed (the reader), not the one that drove the change.
- Events are kept in a queue ordered by logical time and processed in that order (`Circuit::advance`/`run`).

Because outputs only ever change in response to a scheduled future event, a feedback loop can't cause infinite synchronous recursion — an SR-NAND latch settles into a stable state after a few ticks, and a ring oscillator oscillates (as real hardware would) instead of hanging the simulator.

### Oscillation detection

If a net changes state more than a threshold number of times within a single `advance()`/`run()` call (currently 1,000), the engine stops and returns an `UnstableCircuit` error instead of looping forever. Verified with a unit test: a single NOT gate wired back to its own input never settles and is correctly reported as unstable.

### Periodic components (`Clock`)

Not every component settles — a `Clock` is meant to oscillate forever. `Circuit::schedule_periodic(component, period)` marks it as self-rescheduling: after every evaluation, it reschedules itself `period` ticks later automatically, regardless of whether its output changed. Because of this, `Circuit::run()` (drain the queue until empty) can never truly finish once a periodic component exists — it's redefined as `Circuit::advance` bounded to a large-but-finite tick count (1,000,000) purely so it returns instead of hanging, not as a real way to "settle" a clocked circuit.

The actual way to drive a `Clock`: `Circuit::advance(ticks)` processes events up to a given point and *stops*, leaving anything further out (including the clock's next self-reschedule) queued for later. The GUI calls this once per frame with a tick count derived from real elapsed time, so a `Clock` ticks at a genuine, wall-clock-tied rate rather than free-running as fast as the CPU allows.

### One evaluation per component per tick

`Circuit` refuses to queue the same component twice at the same tick. Asking
twice means "make sure this is evaluated then", which it is — not "evaluate
it a second time in no time at all".

The rule is not an optimisation. For anything combinational a repeat is
merely wasted work, since the same inputs give the same outputs; for a
component carrying state it is wrong. A `Clock` toggles on *every*
evaluation, so a duplicate made it toggle twice per period and appear never
to move at all — and it stayed doubled, because each evaluation reschedules
itself.

### Reading the clock, and stepping

`Circuit::now()` is the logical clock: how many ticks the circuit has been
advanced by. `Circuit::next_event_tick()` says when the next thing is due,
or `None` when nothing is — skipping events that name a component since
removed, because `advance` drops those without evaluating anything.

Both exist for single-stepping in the GUI. A caller advancing one tick at a
time has no other way to say where it got to, and a step that changes
nothing visible is otherwise indistinguishable from a step that never
happened.

One consequence worth knowing: oscillation detection counts *within* a
single `advance` call, so stepping a tick at a time never reaches the
threshold. That is what you want when you are deliberately walking an
oscillation — but it means stepping cannot warn the way running does.

## Data model

- **`Signal`** — `High`, `Low`, `Unknown`, `Error`, or `HighZ`. Unknown/error states are modeled from the start, since mishandling X/Z-like states is a common source of subtle simulation bugs. `HighZ` is distinct from `Unknown`: `Unknown` means "not driven / not simulated yet", `HighZ` means "this driver is deliberately not driving right now" (tri-state), needed for bidirectional buffers.
- **`Pin`** — an input, output, or bidirectional (`PinDirection::InOut`) terminal of a component, connected to a `Net`. A bidirectional pin drives the net when active and reads it otherwise (e.g. a bus transceiver).
- **`Component`** — a trait with `eval(&self, inputs) -> outputs` and `propagation_delay()`. Sub-circuits implement the same trait, so hierarchy (a circuit used as a component inside another circuit) is a first-class concept rather than a special case.
- **`Circuit`** — the graph of components and nets, plus the event queue and logical clock. A `Net` can have more than one potential driver (to support tri-state buffers); resolving its `Signal` ignores `HighZ` drivers, then: 0 remaining → `Unknown`, 1 → that value, ≥2 differing → `Error`.
