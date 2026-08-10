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

- [ ] **Multi-bit buses**

  A byte-wide datapath drawn in single-bit wires is eight times everything:
  eight wires, eight pins per port, eight probes to read one value. For a CPU
  this is the difference between feasible and not.

  It reaches into `Signal` and therefore into the engine, which is the
  argument for doing it sooner rather than later — the cost grows with
  everything built on top of the current shape.

- [ ] **A clock's period, and a component's delay**

  Both are constants today: every clock beats every sixty ticks, and every
  component answers in one. Neither is reachable from the editor.

  The period is the one that bites first — two clocks at different rates is
  the ordinary way to drive anything, and there is no way to ask for it. The
  delay matters for the ripple counter already listed below: staged settling
  is the whole difference between it and a synchronous one, and with every
  stage at one tick there is nothing to see.

  `Component::propagation_delay` already exists per component; what is
  missing is a property and a way to type it.

- [ ] **A waveform view**

  The engine is discrete-event and already knows *when* everything happened;
  nothing exposes that history. Seeing a handful of nets over time is the
  natural companion to single-stepping, and the usual next question after
  "what is it doing right now".

- [ ] **Looking inside an instance while it runs**

  Flattening puts every nested component into the one engine, at any depth,
  so watching a circuit means its sub-circuits' innards are right there,
  live — and invisible. Seeing what a gate you built is doing means opening
  it as its own circuit, which rebuilds it cold and loses the very state you
  were looking at.

  **The expensive half is already done, and thrown away.** `flatten` builds
  `ids: Vec<Option<ComponentId>>` — one entry per saved component of the
  sub-circuit, `None` for a port, which isn't instantiated — uses it to map
  the pin groups, and drops it. Keeping it on the instance, recursively for
  nested ones, *is* the map a live inner view needs.

  **A tree of instances, not of circuits.** Two `nand` on a schematic are
  two copies with two different states, so this cannot reuse the tree on the
  left: that one lists definitions, this lists occurrences.

  **It goes where the properties panel sits**, which in the simulation view
  is greyed from edge to edge and says nothing. The tree on top, and the
  selection's read-only properties underneath — what a component is *set to*
  is still worth reading while you watch it work. Each mode then owns its
  right-hand panel, the way each already owns its tool row.

  Two things known in advance:

  - a resizable panel only keeps the size it is given while its content
    fills it. `set_min_height` was needed for the palette and again for the
    circuit tree; this is the third, and it costs a line rather than a
    discovery;
  - descending is effectively a **third view**, so it needs its own camera
    and a way back — a breadcrumb. Without the camera you arrive on blank
    canvas, which is exactly the bug `switch_view` was fixed for.

  Read-only inside, and structurally so: editing there is editing the
  *definition*, which means a rebuild — and rebuilding cold is the thing
  being avoided. The ids only hold until the next rebuild, and nearly every
  edit rebuilds; the simulation view is the one place they can be trusted,
  which is the argument for it living there and nowhere else.

  Worth building **after** [multi-bit buses](#next): those reach into
  `Signal` and the engine, so their cost grows with everything stacked on
  top, while this one only waits.

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
- [ ] **Saying which drivers disagree** on a net that reads `Error`. The
  engine knows — the resolution is per pin — and the answer is currently
  found by reading the schematic and reasoning, which is what a simulator is
  supposed to save you.
- [ ] **Reading a project written by a newer build** when the difference is
  only fields it doesn't know. The version check is all or nothing, so a
  format that gained an optional field refuses to open at all, and
  `scripts/set-format-version.py` exists to work around exactly that. The
  table that script keeps — which versions only *added* — is the same
  judgement the reader would have to make, so it is already written down.
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

- [ ] **Splitting `canvas_ui.rs`**, if it starts to hurt. `app.rs` was cut
  from 6975 lines to 2623, but this one is still 1400 in a single method. It
  was left whole on purpose — its pieces share the frame's pointer position,
  the resolved wire routes and the click-consumed flag, and handing those
  between a dozen small functions would move the complexity rather than
  reduce it.

- [ ] **Rendered snapshots of symbols**, if the appearance work needs them.
  The interface tests drive the real application and assert on state; what
  they cannot check is what a symbol *looks* like. Deliberately not done:
  CI runs on three platforms and text rasterises differently on each, so a
  pixel reference could not hold across them. It would suit a Linux-only
  job, which is a different decision.

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
