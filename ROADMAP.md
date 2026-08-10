# Roadmap

What isn't built yet, roughly in the order it's worth building.

This is a list of intentions, not a schedule: there are no dates and nothing
here is a promise. Items move up when something makes them urgent — usually a
circuit that turns out to be painful to draw — and out when they're done.

| Mark | Means |
|---|---|
| `- [ ]` | Not started. |
| `- [ ] 🚧` | Under way. |
| `- [x]` | Done — and on its way out of this file. |

For *why* the things that exist are the way they are, see
[CLAUDE.md](CLAUDE.md); this file is only about what's missing. When an item
ships, tick it, then take it out of here and record the reasoning there.

## Where things stand

Everything originally scoped for v1 is built, and a good deal past it: the
full component set including tri-state and a bidirectional transceiver,
hierarchy with sub-circuits nested to any depth, hand-drawn symbols, projects
with folders, undo, translations, and a release pipeline producing installers
for Linux, Windows and macOS.

So the list below is no longer "finishing v1". It's what would make SimLogix
better at the thing it's for.

## Next

- [ ] 🚧 **Single-stepping and simulation speed**

  Time advances continuously at sixty ticks a second, and a clock beats once
  a second. There is no way to advance one tick and look at the result.

  For anything sequential — a register, a counter, a state machine — that is
  *the* debugging tool, and it doesn't exist. It is also what the simulation
  view's tool row was built with room for.

  In this order, each usable on its own, and the first is what makes the rest
  legible:

  - [x] **A tick counter in the status bar.** Without it, pressing *step* at
    an instant where nothing changes is indistinguishable from a broken
    button. Needs a read-only accessor on `Circuit` for the logical clock.
  - [x] **Step one tick.** `advance(1)`. A tick is one gate delay, so each
    press moves the propagation on by one stage — which is exactly what a
    ripple counter has to be watched through. One tick processes *all* the
    events at that instant, not one of them, and that is the right
    granularity.
  - [ ] **Run to the next event.** Between two clock edges there are 59 ticks
    where nothing happens, and crossing them one at a time is tedious. The
    event queue already knows when the next one falls; it needs an accessor.
  - [ ] **Step one clock edge**, with an explicit **source selector** — see
    below.
  - [ ] **A speed multiplier** on the tick budget (¼×, 1×, 4×). The natural
    companion: slowing down before freezing.

  **Stepping implies pausing.** Pressing *step* while it runs stops it first.

  **The clock source is chosen, not guessed.** A step is "advance to the next
  edge", and with several clocks that needs an answer. The selector lists the
  `Clock` components *and the input ports*, because a circuit drawn to be
  used inside another has its clock on a port — a flip-flop tested on its own
  has no `Clock` in it at all, and refusing to step there would refuse the
  very circuit you want to step through. With a single clock it settles
  itself and is never seen. Guessing from a name (`CLK`, `H`) is shorter and
  is not worth it: when it guesses wrong nothing on screen says why.

  Acting on a port costs nothing, which is what makes this honest: a port's
  *current* level is runtime state, like a button press — only its resting
  level is stored. So stepping a port writes nothing to the document and
  leaves no undo step. (A `Switch` is the exception, since its position *is*
  saved.) On a port a step is high ↔ low; undriven is not part of a cycle.

  The selection cannot be a `ComponentId` — those are handed out afresh every
  time the circuit is rebuilt. It goes by position in the circuit, as
  changing a component's variant already does to recover the selection.

  **Known consequence**: `MAX_TOGGLES_PER_NET` counts per `advance` call, so
  stepping one tick at a time never trips it. That is what you want — you are
  watching the oscillation on purpose — but it means stepping cannot warn you
  the way running does.

- [ ] **Multi-bit buses**

  A byte-wide datapath drawn in single-bit wires is eight times everything:
  eight wires, eight pins per port, eight probes to read one value. For a CPU
  this is the difference between feasible and not.

  It reaches into `Signal` and therefore into the engine, which is the
  argument for doing it sooner rather than later — the cost grows with
  everything built on top of the current shape.

- [ ] **A waveform view**

  The engine is discrete-event and already knows *when* everything happened;
  nothing exposes that history. Seeing a handful of nets over time is the
  natural companion to single-stepping, and the usual next question after
  "what is it doing right now".

- [ ] **Importing a circuit from another project**

  The groundwork is done and unused: projects carry a library name,
  components are saved qualified by it, and a reference from outside is meant
  to read `library:folder/name`. What's missing is the gesture and the copy.

  The cheapest item here relative to what it unlocks — a personal library of
  gates reused across projects.

## Later

- [ ] **Memory: RAM and ROM.** Wants buses first; a byte-addressed memory
  with one-bit wires is not worth drawing.
- [ ] **Autosave and crash recovery.** Nothing is written until you press
  `Ctrl+S`, so a crash costs everything since the last save. Worth doing
  before anyone but Romain relies on it.
- [ ] **Search in the circuit tree**, once a project holds more circuits than
  fit on screen.
- [ ] **Reading a net's identity** on hover — today, telling two nets apart
  means following wires by eye.
- [ ] **VHDL or FPGA export.** A long way off, and only worth it if the
  intent is to run these circuits on real hardware.
- [ ] **Collaboration.** Named here only so it's clear it isn't forgotten; it
  would change the file format and the undo model, and nothing yet calls for
  it.

## Components to add

A running list, kept here so nothing gets lost between sessions.

**All of these are primitives** — settled, so it isn't reopened per entry.
Romain draws these circuits himself from gates, so the drawn form already
exists and doesn't need shipping; what a built-in adds over it is evaluating
in one step instead of rippling through a carry chain. Reusing a drawn one
across projects is a different need with a different answer:
[importing a circuit from another project](#next), which is worked on when
that is what's wanted.

- [ ] Half adder
- [ ] Adder
- [ ] Multiplexer
- [ ] Demultiplexer
- [ ] D latch
- [ ] D flip-flop, triggered on the rising or the falling edge
- [ ] T flip-flop
- [ ] Counter

The **multiplexer** and **demultiplexer** carry a question of their own:
how many ways. A selector of *n* bits picks between 2ⁿ, so either the
width is a property and the symbol grows pins with it, or each width is
its own entry in the palette.
Worth settling when they are built rather than now — and worth building
after [multi-bit buses](#next), since a mux is the component that changes
most if a wire can carry more than one bit.

The flip-flop's edge is a **variant**, and the shape of that answer is
already settled: it lives in the `ComponentKind`, as the transistor's
channel and the transceiver's enable polarity do, with a selector in the
properties panel. From your side it is a property; it just isn't *stored*
as one, because the symbol has to follow — a falling-edge clock input
carries a bubble, and that mark is what tells the two apart on a printed
schematic. Two palette entries, one component.

The counter's **number** needs saying which number before it is built:
how many bits it counts *in*, or the value it counts *to* — a 4-bit
counter and a modulo-10 counter are different components, and a divider
wants the second. It also wants [multi-bit buses](#next) first, or its
output is one pin per bit.

It carries **two** synchronous-or-not settings, which is what a real part
names too — they are independent and mean different things:

- the **counter**: every stage on one clock, against a ripple counter where
  each stage clocks the next. Invisible at rest; it shows only at a
  transition, as the stages settle one after another. This engine can model
  that honestly, since every component already carries a delay — one of the
  few places the discrete-event model would be visible on screen rather than
  only correct.
- the **reset**: taken at the next edge, or the moment it is asserted.

Which of the two is a *variant* rather than a stored property is the same
question the flip-flop's edge answers, and probably gets the same answer for
the reset — an asynchronous clear is drawn differently — while the counter's
own kind may be better as two palette entries outright.

## Internal

Not user-visible, but they decide how expensive everything above is.

- [x] **Splitting `app.rs`** — done: 6975 lines became 2623, with the
  canvas, the menu bar, the appearance editor, wires, circuits and both test
  suites in `src/app/`. `draw` went from 2300 lines to 609.

  What would go further, if it starts to hurt again: `canvas_ui.rs` is still
  1400 lines in one method. It was left whole on purpose — the pieces share
  the frame's pointer position, the resolved wire routes and the
  click-consumed flag, and handing those between a dozen small functions
  would move the complexity rather than reduce it.

- [x] **Tests at the interface level** — done; see `simlogix-gui/src/ui_tests.rs`
  and the note in [CLAUDE.md](CLAUDE.md). Fourteen tests drive the real
  application through `egui_kittest`, each checked to fail against the bug it
  describes.

  What would extend it, if the need arises: rendered snapshots for the
  *appearance* of symbols. Deliberately not done here — CI runs on three
  platforms and text rasterises differently on each, so a pixel reference
  could not hold across them. It would suit a Linux-only job, which is a
  different decision.
## Known gaps in what ships

- [ ] **Signing** — the macOS and Windows artefacts are unsigned, so both
  systems warn on first run. Needs a paid Apple developer account, so it is a
  decision to take rather than an oversight.
- [ ] **A macOS `.app` bundle** — today it is a bare binary, so no Dock icon
  and no Finder integration.
- [ ] **An icon embedded in the Windows executable** — the installer sets one
  on the Start Menu shortcut, but Explorer and the taskbar show a generic
  one. Needs a build script and a build-dependency.

Details on all three in
[documentation/contributing/releasing.md](documentation/contributing/releasing.md).
