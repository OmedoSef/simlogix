# Running a circuit

The simulation advances continuously in real time, so a `Clock` ticks on its
own — one toggle per second by default.

## The simulation view

The toolbar has three modes on its top row — **Schematic**, **Appearance**
and **Simulation** — with the tools belonging to the current one underneath.

*Simulation* shows the same drawing with **every editing gesture taken
away**. Nothing can be dragged out of place, no pin starts a wire, no
waypoint moves, `Delete` and `R` do nothing, the palette is greyed out and
pasting is refused. Watching a circuit means clicking switches and reading
probes for minutes at a time, and one dropped drag in the middle of that is
a change you didn't see happen and won't think to undo.

The status bar says what this mode is rather than which keys edit, since
those keys no longer do.

The properties panel goes **read-only** rather than empty: what a component
is set to is worth reading while you watch it work, and only changing it is
out of bounds. Selecting still works, so it can be read.

What still works is everything that *isn't* a change to the drawing: click a
switch, a button or a port, hover a wire to light its net, pan and zoom, and
copy — reading something out changes nothing.

Flipping a switch is still recorded as an edit, because a switch's position
is part of the document: it is how the circuit was left, not something the
simulation produced. That is the one thing this mode deliberately lets you
change, since it is the whole reason to be here.

Its tool row holds **Interact** and a hand for the view. It is deliberately
short — the row exists as the place inspection tools will go as they arrive,
not as a shelf of buttons that do nothing yet.

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

## Hiding the signal state

`C`, or **Simulation → Show signal state**, stops colouring wires by what
they carry and draws them as plain structure. Useful while laying out a
dense schematic, where four signal colours changing under you is noise
rather than information.

What it hides is the **state**, not your own wire colours — and with the
levels quiet, a coloured net is drawn *in* its colour rather than ringed by
it. The casing exists to leave the middle for the level; once there is no
level to show, the colour takes the middle and the wire reads at full
width.

Components still report themselves — a lit LED stays lit, a probe still
reads out. Only the wires go plain, and the status bar says so, so grey
wires never look like something broken.

Unlike the theme or the language, this isn't remembered between runs. It's a
way of working at a given moment, like pause.

## Weak levels

A transistor passes one level well and the other badly: an NMOS can only
pull *up* through a threshold drop, and a PMOS only pull *down* through one.
That asymmetry is the whole reason CMOS puts the two in parallel as a
transmission gate, so the simulator models it rather than pretending a lone
transistor is a perfect switch.

A level delivered that way is **weak**: real, but overridden by any
full-strength driver on the same net.

- A **transmission gate** — an NMOS and a PMOS in parallel, gated oppositely
  — passes both levels cleanly, because whichever half pulls well wins.
- A **lone pass transistor** still works in its good direction, and in its
  bad one loses to anything pulling the other way. That is what it does in
  silicon too.

A net held up only by a weak contribution is drawn **faded** — wires, probes
and port readouts alike. The colour stays, because the level is real and the
gate downstream will read it; the fading says the noise margin has gone. It
is the difference between a circuit that works and one that happens to work.
