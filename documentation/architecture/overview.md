# Architecture overview

SimLogix is a cross-platform digital logic simulator, written in Rust, meant to replace [Logisim](http://www.cburch.com/logisim/) while fixing its main pain points: a sluggish canvas editor, a limited custom-shape editor, poor handling of feedback loops in combinational circuits (e.g. an SR latch built from NAND gates), and a dated Swing-based UI.

## Workspace layout

The repository is a two-crate Cargo workspace:

- **`simlogix-core`** — the circuit model and simulation engine. No GUI dependency, so it can be built and tested in isolation.
- **`simlogix-gui`** — the schematic editor: canvas rendering, wiring, and the real-time simulation loop.

Keeping the engine free of GUI dependencies means the core logic can eventually be reused (e.g. headless simulation, other frontends) without dragging in a windowing stack.

## GUI toolkit: egui / eframe

[egui](https://github.com/emilk/egui) (via the [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) framework) was chosen over alternatives like iced or Slint for two reasons:

- Direct access to an immediate-mode `Painter`, which suits custom rendering of gates, wires, and the grid.
- Immediate-mode UI fits naturally with a real-time simulation loop (redraw every frame from current state, no separate retained widget tree to keep in sync).

It's also natively cross-platform, with a WASM export path available later if a browser-based version is ever wanted.

## Simulation engine

See [simulation-engine.md](simulation-engine.md) for the discrete-event model that handles feedback loops — this is the direct answer to the "circuits with feedback lock up the simulator" problem from Logisim. Implemented in `Circuit` and driven by the GUI's frame loop, with an unstable circuit pausing the simulation and reporting the offending net rather than hanging.

## Current status

**`simlogix-core`** — `Signal` (`High`/`Low`/`Unknown`/`Error`/`HighZ`), `NetId`, `PinDirection` (`Input`/`Output`/`InOut`), `Pin`, the `Component` trait, and `Circuit`: a discrete-event engine (`schedule_now`, `schedule_periodic`, `advance`, `run`, logical clock, `UnstableCircuit` detection) plus structural editing after the fact — `rewire` (replace the whole pin-to-net mapping from groups of pins the caller says are connected) and `remove_component`. Connectivity is *derived*, not accumulated: the GUI owns the drawing and hands over the resulting groups after each edit, so a net always states what the schematic currently shows. Sixteen concrete components:

- sources — `Button`, `Clock` (periodic), `Rail` (fixed `Ground`/`Power`);
- outputs — `Led`, `Probe`;
- `Transistor` (NMOS/PMOS, gate-controlled `Source -> Drain` pass, `HighZ` when not conducting);
- gates — `And`, `Or`, `Nand`, `Nor`, `Xor`, `Xnor`, `Not`, `Buffer`, all combinational, resolving uncertain inputs by dominance (a definite `Low` forces an AND's output `Low` whatever the other input is; `Xor`/`Xnor` have no dominant input, so both must be definite).

All unit-tested, including a self-looped inverter correctly reported as unstable.

**`simlogix-gui`** — modules mirror the concepts: `app.rs` (state and the `eframe` loop), `canvas.rs` (grid, hit-testing, theme colours), `symbol.rs` (a hand-drawn vector symbol per component kind), `placed_component.rs` (one placed instance), `palette.rs`, `toolbar.rs`, `i18n.rs`, `project.rs`.

What it does today:

- **Editing** — a categorised palette, a Select/Wire toolbar, placement with a live preview (Shift to place several), selection, drag-to-move with grid snapping, rotation, deletion.
- **Wiring** — wires are explicit objects (`Wire`) with two endpoints and a route. An endpoint is a pin, a junction onto another wire, or a loose point; wires are drawn click by click with as many waypoints as wanted, can be split, rejoined, tapped, and reattached by dragging an end. Deleting a component keeps its wires, cut loose where the pin was.
- **View** — zoom and pan via `egui::Scene`; everything is stored in scene coordinates, so nothing else has to know about the transform.
- **Simulation** — advanced every frame by elapsed wall-clock time (60 logical ticks/second) so a `Clock` runs in real time; run/pause, and an unstable circuit pauses with the offending net named in the status bar.
- **History and files** — undo/redo over a stack of document snapshots, a versioned project format (v3, with migrations from v1 and v2), Save vs Save As, an unsaved-changes guard on New/Open/Quit, and a window title showing the file and its modified state.
- **Presentation** — light/dark theme following the OS, English/French/Italian following the locale, and signal colours defined once per theme so they stay legible on both backgrounds.

Not implemented yet: multi-select, copy/paste, a custom symbol editor, and sub-circuit hierarchy (multiple circuits per project, used as components inside each other). Connectivity is also tracked by merging nets as wires are drawn rather than recomputed from the drawing, which has one visible consequence — see the limitation noted in [Wiring](../using/wiring.md).
