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
driven on its own. And an engine-state inspector, which exists because every
bug found in this project so far was found by printing exactly that.

**Multi-bit buses are in**, engine and editor alike: a signal is a list of
levels, a net takes its width from the drawing, the ports and gates carry
one, and there is a splitter, a constant, a base to read values in and a
symbol that grows to hold what it shows. What that cost and why it was done
that way is in [CLAUDE.md](CLAUDE.md).

And the **splitter is connectivity rather than a component**: it contributes
no pin of its own, its branches *are* parts of its bus, and a value crossing
one costs nothing because there is nothing in between. What is left of that
is the entry on bit mapping below.

And **sequential logic**: a D latch, a D flip-flop and a T flip-flop, each
rising- or falling-edge and each a register when it is wider than a bit, plus
a synchronous counter with load, enable, direction and carry out. The
flip-flops sample what their data pin held *before* the edge — a setup time,
and what keeps a chain of them on one clock from shifting a value down all of
it at once.

So the list below is no longer "finishing v1". It's what would make SimLogix
better at the thing it's for.

## Next

- [ ] **A file format that stops going out from under you**

  Romain's, and the way he put it is sharper than "we bump too often": the
  number rising costs nobody anything — **it rising in *his* files** does.
  Opening a v10 project with a v16 build and saving rewrites it as v16, and
  the other machine can no longer read it. That is what
  `scripts/set-format-version.py` exists to work around.

  This is the promise a 1.0 rests on, so it comes before more components,
  not after. Three parts, in the order they are worth doing:

  - **Write the lowest version that can express the document**, computed
    from what is in it rather than from the build. "≥16 because something
    is mirrored, else ≥15 because something names its base, else…" — one
    rule per bump, kept honest by a test, which is the bargain `SAVED_NAMES`
    and the downgrade script's own table already make. A pleasant
    consequence: the version can go **down** — delete the mirrored
    component and the file is readable by the old build again.
  - **An unknown component must not cost the file.** Today
    `ComponentKind`'s deserialiser fails on a name it doesn't know, and one
    such component refuses the whole project. Warning is the smaller half of
    the fix: the real one is **keeping it**. Held verbatim — its type
    string, its raw properties, its place in the list — it saves back
    intact and the wires that reference it by index stay valid. Without
    that, opening and saving *destroys* what wasn't understood, which is
    worse than refusing to open.
  - **Two numbers, not one.** The additive-or-structural distinction already
    exists but lives in a table beside the code (`ADDITIVE` in the downgrade
    script); a table can drift, a number cannot. `major.minor`, and a reader
    that says *"I know major 2, I take any minor, and I ignore what I don't
    recognise"*.

    **Two, not three.** A third would separate "ignoring this loses an
    annotation" from "ignoring this draws a circuit that means something
    else" — a mirror being the second kind. But that judgement would be made
    at every bump and getting it wrong is *silent*. Better to hand it to the
    reader: opening says *"3 components carry a `mirrored` setting this
    build does not know"*, and the person decides.

  **What it does not do**, said plainly: it cannot rescue what is already
  written. A build at 0.7.0 or earlier refuses a newer file because *it* is
  the one refusing. This fixes the future — which is the argument for doing
  it before the format changes again, rather than after.

- [ ] **Which bits go to which branch, freely**

  A splitter's branches take bits **in order from 0**, each a contiguous
  run. That covers splitting a bus into halves or into single bits, and
  stops covering it the moment you decode an instruction: `[15:12]` and
  `[3:0]` in one branch is an ordinary thing to want, and today it takes two
  splitters and a merge that is really a third.

  What it needs is for a branch to be *a list of bit positions* rather than
  a width. The cost is the editor: a table of branch against bit is the
  honest control, and a width spinner is not.

  **This is the moment it is cheapest.** The splitter is connectivity now,
  so a branch is already placed at an offset by a weighted union-find — a
  list of positions is that same machinery asked for each bit instead of
  once per branch. Left much longer and the editor is built around
  contiguous runs.

- [ ] **Whether a click should still flip a switch while drawing**

  A port only drives in the simulation view: a click in the schematic
  *selects* it, so picking one to set its width no longer pokes the circuit.
  A switch is the exception — a click flips it in either view — and the
  reason it was left that way is that flipping one while drawing is
  something Romain uses.

  Both are runtime state now, so the argument that split them is gone. What
  is left is a question of habit rather than of principle, which is why it
  is a note and not a decision: if a click on a switch ever changes one that
  was only meant to be selected, this is the answer.

- [ ] **Two faults, two messages**

  A splitter loop that contradicts itself — one that would put a bit in two
  places at once — is reported today by joining the width faults, so it is
  ringed in red and counted by a sentence that says *"disagree about width
  with what they are wired to"*. That is not what happened, and the two are
  fixed by different things: a width is fixed by changing a property, a
  contradiction by redrawing.

  This file already records the principle, from when width faults were first
  told apart from `Error`: *two messages, not one*. It was right then and it
  is being broken now, by me, for the convenience of one list.

- [ ] **Which bits a pin occupies, in the inspector**

  The engine-state window says a pin reads "1 bits". Since a splitter became
  connectivity that is half the answer: what you want to know is *which* —
  bits 4 to 4 of an eight-bit conductor, not merely one of them. The
  contributions have the same gap.

  It is the window whose whole reason for existing is answering "why is this
  wrong", and offsets are now the commonest thing to be wrong about.
  `Circuit::pin_slice` already returns both halves; only the row is missing.

- [ ] **Getting to a width fault**

  The status bar says how many pins disagree with their net, and each is
  ringed in red on the schematic. On a drawing bigger than the window that
  is a hunt: the complaint knows exactly where the fault is and does not
  say. Clicking the message should select them, and the view should go
  there — `content_rect` and `refit_view` already exist for opening a
  project.

  Small, and it finishes something already half-delivered: naming the pin
  was the whole point of putting the fault on the pin rather than the net.

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

- [ ] **Does it hold up at the size of a CPU, and how would we know**

  Nothing has ever been measured. The largest circuit anyone has drawn is a
  few dozen components, and the answer for a few thousand is simply unknown
  — which is the honest problem, not slowness that has been observed.

  What is worth knowing before optimising anything, because each of these is
  proportional to the **whole document** rather than to what changed:

  - **`record_edit` clones the entire project**, every edit, and keeps up to
    64 of them. On a CPU-sized drawing that is the document copied on every
    keystroke that counts as one.
  - **`rebuild_nets` re-derives all connectivity** and `Circuit::rewire`
    then throws away and reallocates *every* net — on any change to the
    topology. That is the geometric net model working as designed; the
    question is only what it costs when there are thousands of pins.
  - **Every frame** resolves every wire's route, measures every readout and
    draws everything, and the application asks for a repaint unconditionally
    so that a clock keeps ticking. So the per-frame cost is paid sixty times
    a second whether or not anything moved.

  **The first step is a measurement, not a change**: a generated circuit of a
  few thousand components, and a look at where the time actually goes. Any of
  the three above could turn out to be free at that size, and optimising the
  wrong one is worse than leaving all three alone. Only then is it worth
  asking whether an edit can touch less than everything.

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

- [ ] **A bus says how wide it is on the schematic**, the way a printed one
  does: a short stroke across the wire near the pin, with the number beside
  it.

  Everything that reports a width today has to be *asked* — hover it, or
  select it and read the panel. The thickness says "more than one bit" and
  stops there, so reading a drawing means interrogating it wire by wire,
  which is exactly what a printed schematic does not make you do.

  **It goes on the wire at the pin**, not on the pin itself: a width belongs
  to the conductor, and a component's pin already gets its number from
  whatever it is wired to. Which also settles the splitter case for free —
  `wire_slice` already answers per wire, so a branch marks *its* bits and the
  bus marks all of them, rather than eight wires all claiming eight.

  Three things to decide before drawing anything:

  - **Only buses are marked.** One bit is what every wire is until something
    says otherwise, so marking them all is noise rather than information —
    the same rule the hover hint already follows.
  - **Where exactly**, when a wire has two ends and both are pins. Both ends
    is the printed convention and is probably right; one mark per wire would
    leave you hunting for which end has it.
  - **Whether it can be switched off**, and if so where. It is a standing
    mark on every bus in the drawing, so a dense schematic may want it gone —
    which makes it the same kind of thing as hiding the signal state (`C`,
    Simulation menu): something you flip while working, not a preference. If
    it turns out never to be in the way, it needs no control at all, and that
    is the better answer.

  The text goes in `symbol::TextLayer` like every other label, or it is
  resampled by the zoom.

- [ ] **Hovering a splitter's branch lights the whole net**, bus and
  siblings included, which is the opposite of what hovering is for: following
  *one* wire across a crossing.

  **The answer already exists and simply is not used here.** Colouring a wire
  spreads over `wire_colour_groups` — what the *wires* say, splitters left
  out — precisely because colouring a branch would otherwise repaint the bus
  and its seven siblings. The highlight in
  [canvas_ui.rs](simlogix-gui/src/app/canvas_ui.rs) still compares `NetId`s,
  so it lights everything a splitter joined.

  **The two halves of the same gesture already disagree**, which is the
  sharpest argument for the change: the hover *hint* reports what that wire
  carries — two bits of an eight-bit bus, since `wire_slice` — while the
  hover *highlight* reports the net. One says "this wire", the other says
  "this whole conductor", at the same moment and under the same pointer.

  The comment on `hovered_net` says hovering "has to light up the whole net",
  and it was true when it was written: before splitters, a net *was* one
  conductor. It is the same shape of mistake as `switch_view`'s doc comment,
  which described two views and was inherited unchanged by a third.

- [ ] **Opening a project by double-clicking it.** It starts SimLogix and
  then shows an empty canvas — the file is never read.

  **The packaging side is already right**, which narrows this to one place:
  `packaging/simlogix.desktop` runs `simlogix %f`, so the path *is* handed
  over, and `packaging/simlogix.xml` declares the type the file manager
  matches `.slgx` against. What is missing is that
  [main.rs](simlogix-gui/src/main.rs) never looks at `std::env::args()` at
  all — `SimLogixApp::new(cc)` takes the context and nothing else.

  So the work is a path argument threaded to startup, and then the same
  three things any open does, which is what stops this being a one-liner:

  - it goes through `reopen`, not a bare load, so the circuit tree and the
    camera arrive in the same state as a File → Open;
  - `name_library_after` runs, since a first open is one of the two moments
    a file name is allowed to name the project's library;
  - it joins *Open recent*, which is exactly the project you will want back.

  No unsaved-changes guard is needed, uniquely: nothing has been edited yet.

  **A failure has to be visible.** A path that does not open — moved,
  deleted, an older build's format — must say so rather than leaving an
  empty canvas that looks like a successful start. That is the same reasoning
  as *Open recent* dropping an entry at the moment it fails, and it is worth
  deciding before writing rather than after.

- [ ] **Moving a component that shows a readout** — the ports, the probe and
  the constant. Romain reported it after using the multibit work; the exact
  symptom still has to be pinned down, so the first job is reproducing it and
  saying what "wrong" is.

  Two things about those kinds are unusual and are where I would look first,
  in this order:

  - **Their box is sized from what they display.** `PlacedComponent.readout`
    is refreshed every frame from the width and the base — never stored — so
    `rect()` can change size between one frame and the next, while a drag
    holds a grab anchor from an earlier one. Every other component's box is a
    fixed `BOX_SIZE`.
  - **They keep their body upright** (`symbol::keeps_upright`), so their box
    is deliberately *not* rotated where every other component's is. That
    rule was added when a quarter turn left a wide readout lying across a
    tall narrow box, and a drag reads the box.

  Both are recent and neither is covered by a test that moves one of these
  specifically — the group-drag test uses plain components.

- [ ] **A drawing fault in the palette's symbols.** Reported by Romain, again
  without a symptom yet, so the same first step: reproduce, then say what is
  wrong before changing anything.

  What is worth suspecting, because it is what the palette does differently:
  an icon is drawn into a rect **smaller than a component box**, and the
  lengths a symbol uses have to cope with that. `symbol::fixed(width,
  fraction, at_most)` exists precisely for it — a length proportional below
  an ordinary box and capped above — and it was introduced when fixed lengths
  swallowed the icons whole. Any symbol using a raw constant where it should
  use `fixed` looks right on the canvas and wrong in the palette, which is
  exactly the shape of a bug nobody notices while drawing it.

  The counter's icon is the newest and is deliberately *not* a small copy of
  what lands on the canvas, so it is worth ruling in or out first.

- [ ] 🚧 **Libraries: importing another project to place its circuits**

  A personal library of gates, reused across projects. The groundwork has
  been sitting unused since the namespace work: projects carry a library
  name, components are saved qualified by it, and a reference from outside
  was always meant to read `library:folder/name`.

  Settled with Romain, and his framing is what made it small. My own was
  "import a circuit and its dependencies", which needed the reference graph
  rewritten on the way in, a transitive closure computed, and an answer for
  what "update an import" means. Each of those **disappears** below rather
  than being solved.

  ### A library is a project, stored exactly like one

  `libraries/<name>/project.json` and `libraries/<name>/circuits/`, which is
  the layout a project already has. One format, one reader, and importing is
  "drop the other `.slgx`'s contents under `libraries/<name>/`".

  **Copied in, never linked.** The container exists so that *one file*
  travels; a link to another project destroys exactly that.

  **Nested, not flattened**: a library carries its own libraries, at
  `libraries/foo/libraries/bar/`. That looks redundant and is the point —
  the **diamond disappears**. Two libraries each needing `bar` carry their
  own, so there is never a version to choose between, which is the hardest
  question in package management and not one worth having here. The price is
  duplication, measured in kilobytes of uncompressed JSON.

  And a consequence better than the mechanism: **a library's own libraries
  are private by construction.** The host sees `foo`; `foo` sees `bar`. A
  public circuit of `foo` instantiating `bar:adder` resolves *inside* `foo`,
  so `bar` never reaches the host's palette.

  ### `circuit:` resolves relative to the library it is written in

  This is what removes the rewriting. A local reference is stored bare
  (`circuit:alu/adder`) so it survives its project being renamed — which
  means it says *"in my project"*, and after an import "my project" would be
  the **host**: an imported `alu` would silently find the host's own
  `adder`. So it is read relative to the library of the circuit referring,
  not the document. A rule, not a transformation — and re-importing stays
  trivial because nothing was altered on the way in.

  ### In the palette, not in the tree

  What you *do* with a library circuit is what you do with a built-in: click,
  place. So it belongs where the built-ins are, as one more folding section
  headed with the library's name. Nothing marks the individual entries: the
  provenance is the heading, and an entry already draws itself with its own
  symbol.

  It also keeps the circuit tree meaning exactly one thing — **yours, the
  ones you can open and edit** — so the rule is one sentence: the palette is
  what you place, the tree is what you edit.

  - **Folders nest inside the section.** Romain's projects use them, so a
    library will have them; the palette already uses folding headers.
  - **"Look inside"** on a context menu, opening it read-only without it
    entering the tree. Wanting to know what an imported adder is made of is
    legitimate, and the palette has no notion of opening.
  - **Read-only, with an explicit way out**: "copy this into my project",
    which makes it yours and forks it. Better than forbidding, and better
    than letting an edit happen quietly.
  - **No update, no merge.** Re-importing *replaces*. Between two edited
    copies there is no right answer, and promising one would be a lie.

  ### Circuits that stay out of the library

  A project has plumbing — a `nand` made of transistors, a test bench — and
  what it offers should be its interface, not its workings. So a circuit can
  be marked as not part of it.

  **The subtlety that decides the design: it is still copied.** If a public
  `adder` instantiates a hidden `nand`, the library must carry it or the
  adder arrives broken. Hidden means **absent from the listing**, not absent
  from the file — the module rule, where a private item serves a public one
  and simply isn't in the interface. That makes the flag purely
  presentational, so it cannot break a circuit whatever is marked.

  **Not called "private".** The circuit stays in the `.slgx`, readable by
  anyone who opens it, and *has* to for dependencies to work. "Private"
  promises a protection this does not give; the label should say the effect
  — hidden from the library, or internal.

  Navigation still reaches it: opening a public circuit that instantiates a
  hidden one and then wanting to see that one is not a dead end.

  **Per circuit to begin with.** Marking a whole folder would be convenient,
  and is a second mechanism; add it if the use asks.

  ### Two things to get right rather than clever

  Names are **sanitised per level**, as `unique_file_name` already does for
  circuits: a library called `../..` must not be a way out of the container.

  And the flag and the `libraries` list are both **additive** — absent means
  public, absent means none — so no project changes behaviour. Which is also
  the argument for doing [the format work](#next) first: an older build
  opening a project with libraries would drop them on save, silently, and
  that item is precisely what stops it.

## Later

- [ ] **Memory: RAM and ROM.** Wanted buses first, and they are here now —
  so what was blocking this is gone. A byte-addressed memory with one-bit
  wires was not worth drawing; with a bus it is.
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

**But that argument does not hold for every one of them**, and the T
flip-flop is where it showed. Drawn — a D flip-flop with `Q` and `T` into an
XOR into `D` — it costs one extra tick and is otherwise identical. What the
primitive adds there is one symbol instead of three objects and a wire
running backwards: legibility, not speed. Worth checking per component
rather than assuming.

- [ ] Half adder
- [ ] Adder
- [ ] Multiplexer
- [ ] Demultiplexer
- [x] D latch
- [x] D flip-flop, triggered on the rising or the falling edge
- [x] T flip-flop
- [x] Counter, synchronous
- [ ] 🚧 Counter, asynchronous (ripple)

### The ripple counter, and why it is composed rather than written

**It cannot honestly be a primitive.** The only difference between a ripple
counter and a synchronous one is *time*: its bits settle one after another,
so a transition shows real intermediate values (3 → 2 → 0 → 4) and its
maximum frequency falls with the number of stages. A `Component` has one
propagation delay and returns every output at once, so a "ripple counter"
written that way would be indistinguishable from the synchronous one — a
label that lies rather than an approximation.

So it is **built** at place time out of real T flip-flops, the way a
sub-circuit is flattened: one palette entry and one symbol for the user, N
genuine flip-flops in the engine, and the skew is real because the
flip-flops are. This is the one component where the drawn form is *better*
than a written primitive, and the composition is how it stays that way.

**The enabling change is done**: an instance's internal wiring carries a bit
offset (`InnerMember`), so a stage can drive one bit of a shared bus. The
union-find has carried offsets since the splitter work; they simply did not
reach an instance's innards. Everything is at zero today — a sub-circuit's
port occupies its whole conductor — so nothing changed behaviour.

What is left:

1. `Shape::RippleCounter` carrying its wiring, reported by `instance_wiring`
   alongside a real instance's.
2. The construction: a `CircuitAnchor` with `CLK`, `CLR` and `Q`, N
   falling-edge T flip-flops, a power rail for every `T` and a ground rail
   for every `S`; then the groups — the clock on stage 0, each `Q` onto the
   next stage's clock, `CLR` onto every `R`, and **stage *i*'s `Q` at bit
   *i*** of the output bus.
3. The width becomes the number of stages, so changing it has to rebuild the
   document — one more condition beside the synchronous counter's.
4. Palette, translations, `SAVED_NAMES`, and a test that counts 0→7 **and
   asserts the transients**, since those are the only thing distinguishing it
   from the counter that already exists.

Its pins are deliberately fewer — clock, `CLR`, `Q` — and the real parts
agree: a 74x93 has a clock and a clear where a 74x161 has load, enable and
carry. A ripple counter *with* load is not a ripple counter, because its
load path is synchronous.

### What the built ones settled, for the ones still to come

**A component that can only transform its own state needs a way in, and that
way in cannot be a setting.** The T flip-flop showed it and the counter
repeated it: both start holding nothing, and `Unknown` toggled — or
incremented — is still `Unknown`, for ever. So a T flip-flop's asynchronous
inputs are not optional and a counter's `CLR` is not optional, where a D
flip-flop's are, because `D` puts a value in.

**A variant is a `ComponentKind`, not a stored flag**, when the symbol has to
follow — a falling-edge clock carries a bubble, and that mark is what tells
two apart on a printed schematic. Two palette entries, one component, and a
selector in the properties panel. Settled for the transistor's channel, the
transceiver's polarity, and now all three flip-flops and the counter.

**A pin count that depends on a property goes through the document.** A built
component's pins are fixed, so asking a flip-flop for `S`/`R`, or a counter
for `EN`/`LD`/`UP`, rewrites the saved form and reopens — the route a
splitter's branch count already takes.

The **multiplexer** and **demultiplexer** carry a question of their own:
how many ways. A selector of *n* bits picks between 2ⁿ, so either the
width is a property and the symbol grows pins with it, or each width is
its own entry in the palette. Worth settling when they are built rather than
now. `Appearance::generated` is the answer to the drawing half — the
synchronous counter uses it for its own eight named pins, which is the same
shape of problem.

The **adder** is where the one-step argument is strongest of all: a 32-bit
ripple-carry adder drawn from gates settles in 32 ticks, a primitive in one.
The half adder is a different case — two gates, no carry chain — so it earns
its place by legibility rather than by speed, if at all.

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
