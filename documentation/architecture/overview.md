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

See [simulation-engine.md](simulation-engine.md) for the discrete-event model that handles feedback loops — this is the direct answer to the "circuits with feedback lock up the simulator" problem from Logisim. **Implemented** in `Circuit`; not yet wired to the GUI or exercised by concrete components.

## Current status

As of now, the working code is:

- The Cargo workspace scaffold (`simlogix-core`, `simlogix-gui`).
- In `simlogix-core`: `Signal` (`High`/`Low`/`Unknown`/`Error`/`HighZ`), `NetId`, `PinDirection` (`Input`/`Output`/`InOut`), `Pin`, the `Component` trait, `Circuit` — net/component registration plus a real discrete-event engine (`schedule_now`, `run`, logical clock, per-net toggle-count instability detection returning `UnstableCircuit`) — and two concrete components, `Button` and `Led`, wired together and tested end to end (press → the shared net goes `High`). Each piece has unit tests, including a self-looped inverter correctly caught as unstable.
- In `simlogix-gui`: a minimal window with a menu bar (`File` → Quit, `?` → About) plus a fixed demo scene — a push button wired to an LED through a real `simlogix-core::Circuit`. Holding the button drives the shared net `High` (LED lights up), releasing drives it back `Low`. The wiring is hardcoded in `main.rs`, not placed/connected by the user yet.

Not implemented yet: the general canvas editor (placing components, drawing wires, snapping, rotation, selection), logic gates, save/load.
