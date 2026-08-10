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
full component set including tri-state, a bidirectional transceiver and a
three-position source, hierarchy with sub-circuits nested to any depth,
hand-drawn symbols, projects with folders, undo, translations, and a release
pipeline producing installers for Linux, Windows and macOS.

Since then, the tools for finding out what a circuit is *doing*: stepping by
a tick, to the next event or by a clock edge, a speed control, and a clock
source that can be a port — so a circuit meant to sit inside another can be
driven on its own.

So the list below is no longer "finishing v1". It's what would make SimLogix
better at the thing it's for.

## Next

- [ ] 🚧 **Multi-bit buses**

  A byte-wide datapath drawn in single-bit wires is eight times everything:
  eight wires, eight pins per port, eight probes to read one value. For a CPU
  this is the difference between feasible and not.

  It reaches into `Signal` and therefore into the engine, which is the
  argument for doing it sooner rather than later — the cost grows with
  everything built on top of the current shape.

  ### How a signal is represented — **done**

  The seven-state scalar is now **`Level`**, and **`Signal` is a list of
  `Level`**, least significant bit first, a plain wire being a list of one.
  Every truth table still works on `Level` untouched.

  The alternative, recorded because it was considered and refused: pack the
  whole bus into fixed-size masks — `value`, `known`, `driving` over 64 bits
  — which stays `Copy`, allocates nothing, and does a 32-bit operation in one
  instruction. Seven states do not fit in two masks, so it needs three and an
  encoding to remember, and every truth table stops being a readable `match`
  and becomes mask algebra. The engine's core is what gets re-read most —
  it is where the transistor bug and the CMOS NAND bug were both found — and
  that is not worth trading for speed nobody has needed yet.

  Two things made the change cheap, and are worth knowing before the next
  one:

  - **`component::scalar_eval`** wraps a component whose body is written in
    levels. Every one of them is, today, so the conversion is named once
    rather than written out twenty times — and the name says the truth:
    *this component has no meaning on a bus yet*. It stops being true one
    component at a time, as each learns what a bus means for it.
  - **`component::eval_levels`** does the same for the tests, so all 108
    call sites still say exactly what they said before. They are the
    evidence that nothing changed meaning; rewriting their expectations by
    hand is how that evidence would have been lost.

  `resolve` already works **bit by bit**, by the rule a plain wire has always
  used, and contributions of differing widths already come out `Error` on
  every bit. `Signal::only_level` answers `Error` for anything but width one,
  so a component with no meaning on a bus says so on the wire rather than
  reporting its first bit.

  Everything is still one bit wide: nothing yet *makes* a signal wider. That
  is the next step, and it is where this starts to show.

  ### Width belongs to the net, not to the wire

  A wire does not know what it carries; it takes it from what it joins. So
  width is derived in `rebuild_nets`, alongside connectivity, by the same
  pass and from the same drawing. **One bit unless something on the net says
  otherwise**, which is why every project that exists stays exactly as it is,
  with no migration.

  Two pins of different widths on one net is a **fault to report, not to
  guess at** — and a different fault from `Error`. "Two drivers disagree" is
  fixed by unplugging one; "four bits against eight" is fixed by changing a
  property. Two messages, not one.

  Half of that works: the net takes the widest declared, so a narrower pin
  that *drives* contributes the wrong width and the engine faults every bit
  — visible, if not yet named. A pin that only **reads** contributes
  nothing, so wiring a one-bit output to an eight-bit bus is still silent.
  Saying so needs the declared widths compared where they are known, which
  is `rebuild_nets`, and a channel to report it on.

  ### What has to exist

  - [ ] **A splitter**, one component and bidirectional: its pins are
    `InOut`, and which side drives falls out of what is connected, which is
    what multi-driver resolution already does. So there is no separate
    merger. One thing to check when building it — not before — is the
    question the transceiver already answered: an `InOut` pin reads the net
    it drives, so it must not send back to the bus what it has just read.
  - [ ] **A constant**, whose value is typed in a base of your choosing. It
    works on a one-bit wire too, where it is simply 0 or 1.
  - [ ] **A value a port can be set to**, rather than every bit alike. A
    port drives all its bits the same today, so a two-bit one can only be 0
    or 3 — and that was the wrong justification: a port does not stand for a
    switch, it stands for *what a parent will drive*, and a parent drives
    whatever it likes.

    It goes in a **panel of its own** beside the properties, because there
    are two things here and they must not be confused: the **value**, which
    is what the port sends *now* — runtime state, no undo step, never saved
    — and the **resting value**, which is where it sits when the project
    opens and is a property like any other. The same digits, two different
    natures; one field for both would have to lie about one of them.

    Typed in a base, and by the same widget the constant uses — the two ask
    the identical question and building it twice is how the two answers
    drift. The undriven position stays alongside: a value and *not driving*
    are different claims, and a three-state port needs both. On a one-bit
    port it degenerates to the 0/1/undriven cycle that exists, so nothing is
    lost.
  - [ ] 🚧 **A width property** on the components that are built in rather
    than drawn. **The ports have it**, and `rebuild_nets` reads it — the same
    pass that says what a net joins now says how wide it is. The widest pin
    on a net wins, so a narrower one contributes the wrong width and the
    engine faults every bit: taking the maximum rather than refusing is what
    makes a mismatch *visible* instead of quietly dropped. The gates are
    next.
  - [ ] **Reading a bus.** The `Probe` gains a **base** — binary, hex,
    decimal — because eight letters in a row is not a reading. Its *width*
    stays derived from its net: that is a fact it can already look up, and a
    probe told the wrong number would quietly show something false.
  - [ ] 🚧 **Seeing a bus.** **Drawn thicker**, and a selected wire says how
    many bits it carries. Not proportional to the width — what matters is
    one bit against several, and a 32-bit wire as thick as a component is a
    schematic nobody can read. Still missing: the width readable *without*
    selecting, which probably means on hover.

  **Bit 0 is the least significant**, fixed once and written down, because
  the splitter has to say which bits go where and that convention is
  expensive to leave implicit.

  ### Order

  The engine first, with width defaulting to one and the existing tests as
  the net: if all of them still pass, the semantics that exist are intact.
  Then the drawing, then the components.

- [ ] **A clock's period and phase, and a component's delay**

  All three are constants today: every clock beats every sixty ticks, every
  clock's phase is whenever it happened to enter the engine, and every
  component answers in one tick. None is reachable from the editor.

  - **Period.** Two clocks at different rates is the ordinary way to drive
    anything, and there is no way to ask for it.
  - **Phase**, and with it **putting the clocks back in step**. This is the
    one nobody chose: a clock's phase is decided by *when it was placed*, so
    two of them are aligned or not by accident. Both halves are wanted —
    deliberate skew, because two non-overlapping phases is how a real CMOS
    datapath is clocked, and an *align them all* action for when the skew is
    not the point.
  - **Delay.** It decides whether the ripple counter listed below is worth
    building at all: staged settling is the whole difference between it and
    a synchronous one, and with every stage at one tick there is nothing to
    watch.

  Cheaper together than apart. `Component::propagation_delay` already exists
  per component, and period and phase are both arguments to the scheduling a
  `Clock` already does — first fire *here*, then every *this many* ticks.
  What is missing in all three cases is a property and a way to type it.

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

Two are done and out: `app.rs` (6975 lines to 2623) and `canvas_ui.rs` (1457
to 1248) — the second stopping where splitting further would have moved the
complexity rather than reduced it, which is recorded in
[CLAUDE.md](CLAUDE.md).

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
