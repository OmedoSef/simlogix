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

See [simulation-engine.md](simulation-engine.md) for the discrete-event model that handles feedback loops — this is the direct answer to the "circuits with feedback lock up the simulator" problem from Logisim. **Not implemented yet**; only the design decision exists so far.

## Current status

As of now, the only working code is:

- The Cargo workspace scaffold (`simlogix-core`, `simlogix-gui`).
- A `hello()` function with a unit test in `simlogix-core`, exercising the crate/test setup.
- A minimal `simlogix-gui` window ("Hello, SimLogix!") confirming the GUI toolchain (including X11 forwarding from the devcontainer) works end to end.

None of the actual circuit model (`Signal`, `Pin`, `Component`, `Circuit`) or the event-driven engine exists in code yet.
