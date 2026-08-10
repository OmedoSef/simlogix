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

- [ ] **Single-stepping and simulation speed**

  Time advances continuously at sixty ticks a second, and a clock beats once
  a second. There is no way to advance one tick and look at the result.

  For anything sequential — a register, a counter, a state machine — that is
  *the* debugging tool, and it doesn't exist. It is also the first thing that
  belongs in the simulation view's tool row, which was built with room for
  exactly this.

  Small: `advance(1)` behind a button, plus a multiplier on the tick budget.

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

## Internal

Not user-visible, but they decide how expensive everything above is.

- [ ] **Splitting `app.rs`**

  It is around seven thousand lines, and the canvas is a single closure
  inside it. Locating an interaction point costs several searches, every time
  — which is a tax on every item on this list.

  The seams are visible enough: the canvas interaction, the menu bar, the
  panels, and the appearance editor are four things sharing one file.

- [ ] 🚧 **Tests at the interface level**

  A pattern worth naming: the group drag, copy/paste, the view framing, two
  leaks in the simulation mode, and the paint order of the text layer were
  all bugs in the *wiring* between correct pieces — and the unit tests stayed
  green through all of them. Five occurrences is a category, not bad luck.

  `egui_kittest` drives the real application in `src/ui_tests.rs`. Every
  test there was checked to *fail* against the bug it describes, by putting
  the bug back — a regression test that has never failed proves nothing, and
  that stays the rule for the ones below.

  - [x] A harness running the whole application, `Ui` and all
  - [x] `Delete` removes a selected component, and does nothing in the
        simulation view
  - [x] Copy and paste, which is where egui's `Event::Copy` caught us out
  - [x] Pasting refused in the simulation view
  - [x] A way to reach canvas coordinates from a test — the transform is
        recorded while drawing, since that is the only place it is known
  - [x] Dragging a multi-selection, which once came apart by one frame's
        delta. Asserted **mid-drag**: everything snaps to the grid on release
        and snapping hides an error smaller than a grid step, so a released
        drag came out equal with the bug back in
  - [x] Dragging a component in the simulation view does nothing
  - [x] Dragging a wire's waypoint, and the right-click that cuts a wire —
        cutting a *middle* segment, since cutting the first leaves nothing
        before the cut and correctly gives one piece rather than two
  - [x] The rubber band, including that it catches what it merely touches
  - [x] Placing on the canvas, which broke once when a widget covered the
        scene's own background response
  - [x] The view framing on switching circuit — the logic was right and the
        wiring threw the result away
  - [ ] Paint order: circuit labels behind the floating windows. Needs either
        a snapshot or a way to read the layer order back; worth deciding
        which before writing it — the last one left, and the only one whose
        approach isn't settled

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
