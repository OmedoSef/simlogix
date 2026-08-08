# Running a circuit

The simulation advances continuously in real time, so a `Clock` ticks on its
own — one toggle per second by default.

## Running and pausing

**Simulation → Run/Pause**, or `Space`. Editing keeps working while paused;
only time stops, which is what lets you go and fix a circuit that misbehaves.

## Signal colours

Wires, and the components whose job is to show a state (Clock, Probe), are
drawn in the colour of the signal they carry:

| State | Colour | Meaning |
|---|---|---|
| High | green | driven to 1 |
| Low | amber | driven to 0 |
| Unknown | dark blue | nothing is driving it, or it hasn't been evaluated |
| Error | red | two drivers disagree |
| High-Z | grey | a driver deliberately not driving (tri-state) |

Each colour comes in a variant per theme, so it stays readable on both the
light and dark canvas.

A **Probe** additionally spells the state out (`1`, `0`, `?`, `E`, `Z`) — it
is the one component that shows text on the canvas, since naming the state
is its whole purpose. A **LED** is red when lit, dark otherwise, like the
physical part.

## Unstable circuits

Some circuits never settle — a ring oscillator, or a gate wired back to its
own input. Rather than freezing, the engine gives up after a net has toggled
too many times in one step, **pauses the simulation, and names the offending
net in the status bar**.

The circuit stays on screen so you can inspect and fix it. Pressing Run
again clears the report; if the circuit still can't settle, the very next
tick says so again.

This is deliberate: circuits with feedback are exactly what SimLogix exists
to handle, so a circuit that oscillates has to be reported, not hidden and
not fatal. The underlying model is described in
[the simulation engine](../architecture/simulation-engine.md).
