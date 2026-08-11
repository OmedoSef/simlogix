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

Its tool row holds **Interact** and a hand for the view, then run/pause,
the three step buttons and, when there is a choice, the clock picker — see
[Stepping](#stepping-one-tick-at-a-time) below. The tools latch and the
rest happen once, which is why a separator sits between them.

## Running and pausing

**Simulation → Run/Pause**, `Space`, or the ▶ / ❚❚ button at the head of the
simulation view's tool row. Editing keeps working while paused; only time
stops, which is what lets you go and fix a circuit that misbehaves.

## Stepping one tick at a time

**Simulation → Step one tick**, `F10`, or the ⏵| button in the simulation
view's tool row. It stops the simulation first, so the step you take is the
only thing that moved.

A **tick** is one propagation delay — what every component takes to answer a
change on its inputs. So one press moves the signal on by one stage, which
is how you watch something ripple through a chain of gates rather than
seeing only where it ended up.

The **tick count** is at the right of the status bar, and it is worth
watching: most ticks change nothing you can see, so without it a step that
did its job looks exactly like a button that didn't.

**Simulation → Skip to the next event**, `Shift+F10`, or the ⏭ button, goes
straight to the next tick where something is scheduled instead. Between two
beats of a clock there are dozens of ticks with nothing in them, and
crossing those one at a time tells you nothing. It is greyed out when
nothing is pending — a settled circuit with no clock in it will never move
again on its own.

## Stepping one clock edge

**Simulation → Step one clock edge**, `Ctrl+F10`, or the ⎍| button, moves
time on until the clock changes level — high to low, or low to high. That
is usually what "the next state" means for anything sequential, and it
saves counting ticks.

The edge is read **off the wire** rather than worked out from the clock's
period: what ends up on the net is what the rest of the circuit sees, and a
clock feeding something through a gate would make the period a lie.

**Which clock** it acts on is yours to say. The picker beside the button
lists every `Clock` in the circuit **and every input or bidirectional
port** — because a circuit drawn to be used inside another has its clock
arriving on a port, and a flip-flop tested on its own has no `Clock` in it
at all. With only one source the picker doesn't appear, since there is
nothing to ask.

On a port there is nothing to advance *to* — you are its clock — so a step
flips it, high to low and back. Undriven is a third position of that
switch, not part of a cycle. This costs nothing and leaves no undo step: a
port's current level is runtime state, like a button press, and only its
*resting* level is saved. A switch is deliberately not offered as a source
for the opposite reason — its position **is** part of the document.

### What a click does while paused

Nothing, until the next step — and that is deliberate: settling a circuit
moves time, which is exactly what a pause refuses. Setting `D` and *then*
clocking it is the whole point of stepping a flip-flop.

But the two things you can click do not look alike while stopped. A
**switch** draws its own lever, so it visibly flips whatever the engine is
doing; a **port** draws what its *net* resolves to, and no net can change
until time moves. So a click on a port used to look like nothing at all.

So a notice says **a change is waiting for the next step** whenever
something is queued for the current tick and has not run — across the top
of the canvas, where your eyes already are, and in the status bar. A
clock's next beat does not count: it is due later, and saying so every time
you paused would make the message meaningless.

The notice is painted rather than laid out and has no widget of its own, so
it can never take a click meant for the canvas underneath.

### Letting a port beat on its own

Stepping a port by hand is fine for watching one edge, and tiring for
watching a counter. When the chosen source is a port, a **free-run** button
appears beside the picker: armed, that port flips every clock period while
the simulation runs, so a circuit whose clock arrives from outside can be
watched rather than only stepped.

Without it the alternative is dropping a `Clock` into the circuit to test
it, which is a change to the **document** — one you then have to remember
to take out before using the circuit inside another. Beating the port
leaves no trace.

It is off until you arm it, and deliberately so: with no source picked, the
first one in the circuit is used, and a lone `RESET` port oscillating by
itself in every circuit that has one — with nothing in the drawing to say
why — is not a surprise worth risking.

It also only beats **in the simulation view**, since that is where the
button lives. A beat you cannot see the control for is a beat you cannot
stop. The rhythm is counted in logical time, so it follows the speed
setting and stops dead with the pause, exactly as a real `Clock` does.

One thing to know: [unstable circuits](#unstable-circuits) are detected
within a single run, by counting how often a net changes. Stepping a tick at
a time never reaches that count, so it will not stop you — which is the
point, since walking an oscillation one tick at a time is how you find out
what it is doing.

## Speed

**Simulation → Speed** offers ¼×, 1× and 4×. It moves the whole circuit
through logical time faster or slower against real time; a clock's period
is unchanged, so everything keeps its proportions and only the wall-clock
rate differs. Slowing down before freezing is the usual way to catch
something.

Three settings rather than a slider, because what you want is "slower so I
can watch", "normal", or "faster so I stop waiting" — none of which has a
numeric answer. Anything but 1× is shown in the status bar, since a speed
you left on and forgot would otherwise make the circuit look wrong.

Like pause, it is not remembered between runs: it is a way of working at a
moment, not something you set once.

## Driving a circuit by hand

**Click a port or a tri-state source to drive it**, in the **Simulation**
view. Each click steps its value by one: undriven, 0, 1, 2… up to the
widest it can carry, then back to undriven. On a one-bit port that is the
switch it has always been; on a bus it counts, so a click can never lose
more than a step of whatever you set.

In the **Schematic** view the same click *selects* instead, so picking a
port to set its width or its name never also pokes the circuit.

To set a whole value at once, use the **Value** panel on the right — type
it in decimal, or `0x…` for hex and `0b…` for binary. That panel stays
usable while the simulation is showing, unlike the properties above it:
what a port is driving *now* is runtime state, never saved and never an
undo step, which is exactly why it is a section of its own.

A **switch** is the exception on purpose: its position is part of the
document, so flipping one is an edit and works in either view.

## When two widths meet

Pins joined **by wire** are the same conductor, so they carry the same bits
and must agree on how many. When they don't, the widest wins and the
narrower one is **ringed in red** on the schematic, with the status bar
saying how many there are.

The fault is put on the **pin**, not on the net: the net is fine, and one
thing attached to it is mistaken about how wide it is. Saying which is the
whole point — blaming the wire would leave you staring at something that is
doing nothing wrong.

Being *narrower than its net* is not itself a mistake. A splitter's branch
is exactly that — two bits of an eight-bit conductor — which is why the
test is agreement with what a pin is **wired** to, rather than with the net
it ends up part of.

It clears the moment the drawing agrees again.

A **probe never disagrees**: it declares no width at all and simply reads
whatever its net carries, so it can be dropped onto a bus of any width. That
is what makes it an instrument rather than a part of the circuit. A **LED**
is the other case on purpose — it is a one-bit device, so wiring one to a
bus is a real mistake and is flagged as one.

## Looking at the engine

`F12`, or **Simulation → Engine state**, opens a window showing the circuit
as the engine sees it: the logical clock, what is scheduled next, and every
net with its width, what it resolved to, and — the part worth having —
**every contribution to it**: which component, which pin, how many bits, and
what that pin is driving.

That last line is what a resolved value cannot tell you. Knowing a net reads
`Error` says nothing; knowing that one port is driving one bit onto a net of
two says everything.

Selecting something narrows it to the nets that thing touches. It reads and
never writes, so it can be left open while you work.

**Copy for a bug report** puts the whole of it on the clipboard, with the
version and the platform, ready to paste into an issue. It deliberately does
not include your circuit — attaching the `.slgx` file yourself is what helps
most, and it should be your choice to send it.

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
