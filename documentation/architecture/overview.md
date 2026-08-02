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
- In `simlogix-core`: `Signal` (`High`/`Low`/`Unknown`/`Error`/`HighZ`), `NetId`, `PinDirection` (`Input`/`Output`/`InOut`), `Pin`, the `Component` trait, `Circuit` — a real discrete-event engine (`schedule_now`, `run`, logical clock, `UnstableCircuit` instability detection) plus structural editing after the fact: `merge_nets` (connect), `disconnect_pin` (disconnect one pin without disturbing others on the same net), `remove_component` (delete). Five concrete components: `Button`, `Led`, `Transistor` (NMOS/PMOS, gate-controlled `Source -> Drain` pass, `HighZ` when not conducting), `Rail` (fixed `Ground`/`Power` sources), and `Probe` (reads a net's full signal state as text). Each piece is unit-tested, including a self-looped inverter correctly caught as unstable.
- In `simlogix-gui`: a menu bar (`File` → New/Open Project…/Save Project…/Quit, `?` → About), a left palette (all five component kinds), and a dot-grid canvas. Placed components can be selected (click, blue outline), moved (drag freely, snap to grid on release), rotated (`R` — the box stays axis-aligned, only which edge carries inputs vs. outputs rotates), wired together (drag from one pin's hit target to another — draws a wire colored by the net's signal), and deleted (`Delete`/`Backspace`, for either a selected component or a selected wire).
- Save/load: `simlogix-gui/src/project.rs` defines a versioned **project** file format (ready for multiple named circuits later, though only one — `"main"` — exists today) capturing component kind/position/rotation and which pins are wired together. Loading starts cold (no button-pressed state, no signal values carried over). File I/O goes through a native dialog (`rfd`, default `xdg-portal` backend) — the devcontainer bind-mounts the host's D-Bus session socket so this reaches the host's real desktop portal.

Wires route as a 3-segment orthogonal "Z" path between the two pins (`canvas::orthogonal_path`) instead of a direct diagonal line, and the route's vertical "bend" segment can be dragged to reposition it — a per-wire override stored in `SimLogixApp.wire_bends`, scoped to one movable bend rather than a full arbitrary-waypoint chain.

Not implemented yet: multiple waypoints per wire, logic gates, sub-circuit hierarchy (multiple circuits per project, used as components inside each other).
