# SimLogix

> This file is the project's reference guide. It must be **updated on every new decision** (architecture, scope, convention) to stay the source of truth — including when switching machines.

Project name: **SimLogix** (Sim + Logic).

## Project context

Romain wants to replace Logisim with a tool that fixes his main frustrations:

- **Tedious canvas interaction**: placing/wiring gates and wires is slow and clunky.
- **Painful component appearance editing**: Logisim's shape editor is limited.
- **Poorly handled feedback loops**: circuits with combinational feedback (e.g. an SR latch in NAND) cause bugs/hangs in the simulation engine.
- **Dated UI/UX** (Swing/Java).

Goal: a cross-platform logic simulator, written in Rust.

## Architecture decisions

_(current state — nothing implemented yet, see Progress)_

- **2-crate Cargo workspace**:
  - `simlogix-core/` — circuit model + simulation engine, no GUI dependency, testable in isolation.
  - `simlogix-gui/` — schematic editor + rendering + real-time simulation loop.
- **GUI: egui / eframe.** Chosen over iced or Slint for direct control of the `Painter` (smooth custom rendering of gates/wires/grid) and the immediate mode which naturally fits a real-time simulation. Native cross-platform + WASM export possible later.
- **Simulation engine: discrete events with propagation delay.** This is the answer to the feedback-loop problem. Every component has a propagation delay (defaults to 1 logical tick); an input change schedules an output event at `t + delay` instead of propagating instantly. An event queue ordered by logical time processes events in order — like HDL simulators (Verilog). This avoids infinite recursion on a combinational loop by construction (an SR-NAND latch or ring oscillator converges/oscillates instead of crashing).
  - Oscillation detection: if a net changes state more than N times within the same time step, the engine stops and reports "unstable circuit" instead of freezing the UI.
- **Data model**:
  - `Signal::{High, Low, Unknown, Error, HighZ}` — the unknown/error state is planned from the start (one of the "simulation bug" irritants often comes from mishandling X/Z). `HighZ` is distinct from `Unknown`: `Unknown` means "not driven / not simulated yet", `HighZ` means "this specific driver is deliberately not driving right now" (tri-state).
  - `Pin`: input, output, or bidirectional (`PinDirection::InOut`) terminal of a component, connected to a `Net`.
  - `Component`: a trait with `eval(&self, inputs) -> outputs` + `propagation_delay()`. Sub-circuits also implement this trait (hierarchy is a first-class citizen, not a hack).
  - `Circuit`: graph of components + nets, event queue, logical clock.
  - **Multi-driver nets (not implemented yet)**: to support bidirectional/tri-state buffers (e.g. a bus transceiver), a `Net` must be able to have more than one potential driver. `Circuit`'s resolution rule when computing a net's `Signal`: ignore drivers reporting `HighZ`; if 0 remaining drivers → `Unknown`; if 1 → that driver's value; if ≥2 with differing values → `Error`. This needs to be designed when `Circuit`/`Net` are implemented, not before.
- **Dev container** (`.devcontainer/`): `rust:1-slim-bookworm` image + X11/GL/GTK libs needed by eframe (`libx11-dev`, `libxkbcommon-dev`, `libgl1-mesa-dev`, `libgtk-3-dev`, etc.), `clippy`/`rustfmt` installed. The host's X11 socket (`/tmp/.X11-unix`) is mounted and `DISPLAY` propagated, so `cargo run` from the container opens the window directly on the host desktop (X11 forwarding). The host's D-Bus session socket (`${localEnv:XDG_RUNTIME_DIR}/bus`) is mounted the same way, with `DBUS_SESSION_BUS_ADDRESS` propagated — this lets `rfd`'s native file dialog (used for save/load) reach the host's real `xdg-desktop-portal` instead of needing a portal running inside the container (there isn't one). Tried forcing `rfd` onto its `gtk3` feature first as a workaround; reverted once the D-Bus forward worked, since `gtk3`'s gtk-rs bindings are unmaintained (`cargo audit` flagged them) and the portal is the more correct approach anyway. `remoteUser: vscode` to stay consistent with the pattern already used on `file-checker`. The container is only for the build/dev toolchain (source code is bind-mounted by VS Code, not copied into the image).
  - `cargo install cargo-audit`/`rustup component add` in the Dockerfile must run **after** switching to `USER vscode`, not before — running them as root first leaves root-owned files in the shared cargo registry cache that later block `vscode` from writing new entries (`Permission denied`). Learned this the hard way after a devcontainer rebuild.
  - `cargo build --release` inside the devcontainer produces a binary at `target/release/simlogix-gui` that also runs directly on the host (bind-mounted, so no copy needed) — the container's Debian bookworm has an older glibc (2.36) than a typical host, and glibc is forward-compatible, so this direction works. Confirmed working on the host's real Ubuntu 24.04 desktop, including the native save/load dialog (no forwarding needed there, since it's not sandboxed in a container).
  - Practical setup instructions (`xhost` prerequisite, how to open the devcontainer): see [README.md](README.md), not duplicated here.

## v1 scope / Out-of-scope

**In v1 (minimal simulator):**
- Basic gates: AND, OR, NOT, NAND, NOR, XOR, XNOR, buffer.
- Bidirectional/tri-state buffer (bus transceiver) — added to scope because Romain will need it; requires the multi-driver net resolution described above.
- Input (switch/button), output (LED), clock.
- Schematic editing: placement, wire routing with grid snapping and orthogonal routing, rotation, multi-select/move.
- Real-time simulation wired into the UI loop.
- Save/load a circuit (serde).

**Out of scope for v1 (future roadmap, not to be built now):**
- A full custom appearance *editor* (user-drawn symbols) — v1 ships hand-coded vector symbols per component kind instead (see the "Component appearance" convention below), not a UI for drawing your own. This is Romain's pain point #2; a real editor is addressed after the core is solid.
- Multi-bit buses, memory (RAM/ROM), VHDL/FPGA export, collaboration.

## Working conventions

- **We move forward together, step by step.** Don't scaffold or write large chunks of code at once without validation — propose a step, discuss, then implement.
- This file must be updated as soon as a new structuring decision is made (architecture, scope, convention) — not only at the end of a session.
- Always check that this file reflects the actual state of the code before fully relying on it (the code is authoritative in case of divergence).
- **Dev tooling**: `rust-toolchain.toml` (pins Rust 1.97.1 + clippy/rustfmt), `.editorconfig`, `pre-commit` hook (`fmt --check` blocking + `clippy` informational, consistent with the choice not to `deny(warnings)` yet), `cargo-audit` installed in the image (RustSec scan, run manually for now — no CI yet). Details: [documentation/contributing/dev-tooling.md](documentation/contributing/dev-tooling.md).
  - `cargo audit` had flagged 2 transitive vulnerabilities (`quick-xml` 0.30) and 2 unmaintained crates (`paste`, `ttf-parser`) via the eframe/accesskit dependency chain. Fixed by upgrading `eframe`/`egui` 0.29 → 0.35 (this required adapting `simlogix-gui`: `eframe::App::update(&Context, ...)` became `ui(&mut Ui, ...)` in 0.35). Only remaining flag is `ttf-parser` unmaintained (warning, not a vulnerability) — `cargo audit` now exits clean.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, etc., scope = name of the crate/area touched). Enforced locally by a `commit-msg` hook versioned in `.githooks/`, wired up automatically in the devcontainer via `postCreateCommand`. Details: [documentation/contributing/commit-conventions.md](documentation/contributing/commit-conventions.md).
- **User/contributor documentation**: [documentation/](documentation/README.md) folder, in English, split into subfolders (`getting-started/`, `architecture/`, `contributing/`) to avoid overloading the README. Grows along with features. CLAUDE.md remains the internal decision log ("why" + progress); documentation/ is the "what/how" for an external reader.

## Code conventions

- **Language**: identifiers (types/functions/variables) in English (standard Rust convention); comments and doc-comments (`///`, `//!`) also in English. `README.md` stays in French; `CLAUDE.md` is in English (switched from French on request, no functional reason — just consistency with the rest of the written material).
- **Error handling**: custom errors via an `enum` + [`thiserror`](https://docs.rs/thiserror) (no raw `String` errors, no `anyhow` in `simlogix-core` — it's a lib, callers need to be able to match on precise variants).
- **Panics**: `panic!`/`unwrap()`/`expect()` forbidden outside `#[cfg(test)]`. An invalid input or malformed circuit must always surface as a `Result`, never panic.
- **Formatting/linting**: `cargo fmt` (default config, no `rustfmt.toml`) and `cargo clippy` before considering a step done. No custom clippy config for now (may tighten later — `deny(warnings)`, `pedantic` — if needed).
- **Tests**: unit tests co-located in a `#[cfg(test)] mod tests` module at the bottom of the file they test, preceded by a banner comment separating it from the real code:
  ```rust
  // -----------------------------------------------------------------------------
  // Tests
  // -----------------------------------------------------------------------------

  #[cfg(test)]
  mod tests {
  ```
- **Module organization**: one file per concept — `signal.rs`, `pin.rs` (`Pin`, `PinDirection`), `net.rs` (`NetId`, later `Net`), `component.rs` (the `Component` trait only), `circuit.rs`. `lib.rs` stays just crate doc + `mod`/`pub use` declarations, nothing else. Once there are several concrete components (gates, `Button`, `Led`...), they go under a `components/` subfolder rather than piling into `component.rs`. Each file keeps its own `#[cfg(test)] mod tests` at the bottom.
  - Same principle applies to `simlogix-gui`: `main.rs` stays just `mod` declarations + `fn main()`. `app.rs` (`SimLogixApp`, the `eframe::App` loop), `canvas.rs` (grid + generic box/pins rendering, `BOX_SIZE`/`snap_to_grid`), `palette.rs` (`ComponentKind`, the palette panel), `placed_component.rs` (`PlacedComponent`: draw + interact for one placed instance).

## Progress

- [x] Project and architecture framing (this document).
- [x] Git + devcontainer scaffold (Rust toolchain, X11 GUI passthrough).
- [x] Rename folder/repo `new-logisim` → `simlogix`.
- [x] README.md (practical setup) split from CLAUDE.md (decisions/context).
- [x] Cargo workspace scaffold (`simlogix-core`, `simlogix-gui`).
- [x] Minimal eframe/egui GUI shell ("Hello, SimLogix!" displayed, X11 forwarding validated — required adding `libxkbcommon-x11-dev` to the devcontainer).
- [x] `documentation/` folder started (getting-started, architecture, contributing).
- [x] `simlogix-core` engine, enough for `Button`/wire/`Led`: `Signal`, `NetId`, `PinDirection`, `Pin`, `Component` trait, `Circuit` (discrete-event engine — `add_component`, `schedule_now`, `run`, logical clock, `UnstableCircuit` instability detection), and concrete components `Button` (`components/button.rs`, pressed state shared via `Rc<Cell<bool>>` so the GUI can toggle it without a `&mut` back from `Circuit`) and `Led` (`components/led.rs`, a pure sink — read its state via `Circuit::signal_at` on its input net). All unit-tested, including an end-to-end button→net→LED test. Not implemented: gates (AND/OR/NOT/...), the bidirectional/tri-state buffer, save/load.
- [ ] Feedback-loop tests (SR-NAND latch, ring oscillator).
- [x] First real-time simulation ↔ UI integration: a fixed (not yet placeable/wireable) demo scene in `simlogix-gui` — a push button wired to an LED through `simlogix-core::Circuit`, holding the button drives the net `High` and lights the LED, releasing drives it back `Low`. Proves the engine runs correctly inside the `eframe` loop before building the general editor.
- [x] Editor interaction (placement, wire drawing, snapping, rotation, selection) — the general canvas editor, built step by step (custom wire *routing* remains as a known follow-up, see step 4):
  - [x] Step 1 — canvas + grid background, and a generic "box with named pins" renderer (`simlogix-gui/src/canvas.rs`: `draw_grid`, `draw_component`) used for both `Button` and `Led` instead of ad-hoc widgets. This is the auto-generated appearance planned for v1. Positions are still hardcoded — not yet placed/dragged by the user.
  - [x] Step 2 — placement: a left palette (`Button`/`LED`) queues a kind, clicking the canvas drops it there (snapped to `canvas::GRID_SPACING`). Each placed component gets its own, still-unconnected net — wiring them together is step 4. `SimLogixApp` now holds `Vec<PlacedComponent>` instead of a fixed demo scene.
  - [x] Step 3 — selection (click a placed component for a blue highlight outline) + moving (drag freely, snap to grid only on release — snapping every frame during the drag made it feel jerky, fixed). `PlacedComponent::draw_and_interact` now takes `&mut self` and returns the clicked id (if any) so `SimLogixApp` can track `selected: Option<ComponentId>`.
  - [x] Step 4 — wire drawing: drag from one pin's small hit target to another to connect them. Required a new Core primitive, `Circuit::merge_nets(from, to)` (rewires every pin on `from` to `to`, carries over drivers, recomputes/reschedules like a normal output change) — tested. Wires render as a 3-segment orthogonal "Z" route between pin positions (`canvas::orthogonal_path`/`orthogonal_path_with_bend`/`draw_path`/`distance_to_path`), colored red/gray by the net's signal, redrawn every frame from `handles_by_net` (grouped `PinHandle`s).
    - **Editable bend**: the route's vertical segment ("bend") is draggable — `SimLogixApp.wire_bends: HashMap<(NetId, usize), f32>` stores a user override for its x-position, keyed the same way as `selected_wire`; absent means "use the automatic midpoint". A scoped middle ground short of full arbitrary waypoints (one bend, not an editable point chain) — deliberately kept simple; revisit if one bend per wire turns out not to be enough. **Known limitation, accepted for now**: the `(NetId, usize)` key isn't stable across edits to other components on the same net (deleting/adding a different member can shift the index), same caveat as `selected_wire`.
  - [x] Step 5 — rotation: pressing `R` with a component selected rotates it a quarter-turn clockwise. The box itself stays axis-aligned (label stays horizontal); only which edge (`canvas::Edge` — left/right/top/bottom) carries inputs vs. outputs rotates, via `canvas::Rotation` on each `PlacedComponent`.
- [x] Wire/component deletion: click a wire (line between two pins) or a component to select it, `Delete`/`Backspace` removes it. Required two new Core primitives: `Circuit::disconnect_pin` (inverse of `merge_nets`, gives one pin back a fresh net — other pins sharing the old net stay connected to each other) and `Circuit::remove_component` (drops a component and its driver contributions; `run()` now silently skips a stale event for a since-removed component instead of panicking).
- [x] Two more concrete components: `Transistor` (`components/transistor.rs`, NMOS/PMOS — gate-controlled `Source -> Drain` pass, `HighZ` when not conducting; first real use of the `HighZ`/`InOut` groundwork, though simplified to one-way pass rather than a true bidirectional pass-gate) and `Rail` (`components/rail.rs`, fixed `Ground`/`Power` sources) — added so a transistor can be tested with one hand instead of needing two buttons held at once. Also added `Probe` (`components/probe.rs`, reads out a net's full signal state as text, unlike `Led`'s on/off).
- [x] Save/load a circuit: a **project file** format (`simlogix-gui/src/project.rs`), not just a single circuit — `SavedProject { version, circuits: Vec<SavedCircuit> }`. Only ever one circuit today (named `"main"`); the shape is ready for a sub-circuit hierarchy later. `version` (currently `1`) exists so a future format change can ship a migration instead of breaking old files. Saves structure only (component kind/position/rotation, which pins share a net) — never runtime state (button presses, signal values); loading starts cold, like opening a fresh Logisim file. File I/O uses a native dialog (`rfd`, File → Save/Open Project…, default `xdg-portal` backend — see devcontainer note below for why this needed a config change to actually work).
- [x] Wire routing: wires draw as an automatic 3-segment orthogonal "Z" route (`canvas::orthogonal_path`) instead of a direct diagonal line, and the route's bend is user-draggable (`SimLogixApp.wire_bends`, snapped to grid on release) — see step 4 above.
- [x] `Clock` component (`components/clock.rs`) — a periodic source, alternating `Low`/`High` every evaluation. Required real engine changes, not just a new `Component` impl:
  - **`Circuit::schedule_periodic(component, period)`**: marks a component as self-rescheduling forever after every evaluation (`evaluate()` reschedules it at `clock + period` unconditionally, on top of the normal reader-rescheduling logic). Without this, a `Clock` would tick exactly once and go silent — it has no inputs, so nothing would naturally re-trigger it.
  - **`Circuit::advance(ticks)`**: processes events up through `clock + ticks`, then *stops* — unlike the old `run()` (drain until empty), which would never return once a periodic component exists, since the queue is never truly empty. `run()` still exists, now defined as `advance(MAX_RUN_TICKS)` (a large but finite bound: 1,000,000) purely so it can never hang — existing tests/call sites didn't need to change, but **`run()` is the wrong tool for a clocked circuit**: it can legitimately trip `MAX_TOGGLES_PER_NET` (a fast clock ticking that many times looks indistinguishable from genuine instability) and, worse from the GUI's side, it would let any placed clock burn through a huge chunk of its future ticks in one synchronous burst.
  - **GUI**: `SimLogixApp::ui()` now advances the circuit by real elapsed time *every frame* (`TICKS_PER_SECOND = 60`, accumulated via a fractional `tick_budget`) and calls `ctx().request_repaint()` unconditionally — this is what makes a placed `Clock` visibly tick at a real, wall-clock-tied rate (`CLOCK_PERIOD_TICKS = 60` → 1 toggle/second) instead of free-running as fast as the CPU allows. Tradeoff: the app now redraws continuously (higher idle CPU use) regardless of whether a `Clock` is present, not just on input, to keep this working. All the interactive-settle call sites (button press, wire connect/disconnect, project load) were switched from `run()` to `advance(SETTLE_TICKS)` (a small constant, 32) for the same reason — calling `run()` from *any* interaction would also burst-advance any clock elsewhere in the circuit.
- [x] Fixed a real pin-alignment bug found while testing the clock: multi-pin edges (e.g. a `Transistor`'s `G`/`S`) were positioned by fractional interpolation across the box's height, which doesn't land on the grid except by coincidence for exactly 1 pin. `canvas`'s pin-spacing helper now places each pin a whole multiple of `GRID_SPACING` from the box's center instead.
- [x] Settings menu: light/dark theme (defaults to OS theme via egui's `ThemePreference`/`Theme`), and a language picker (English/French/Italian, defaults to OS locale) — see the i18n convention below.
- [x] Palette reorganized by category (Sources/Outputs/Transistors), each entry showing an icon + name; the canvas now shows only the icon (see "Component appearance" convention below) — the previous auto-generated "box + named-pin-text" appearance is gone.
- [x] **Component appearance convention (standing decision)**: every component kind gets a hand-drawn vector symbol in `simlogix-gui/src/symbol.rs`, not a generic box. No enclosing box, no text labels (not even pin names) — a pin's exact position is wherever that symbol's own lead line ends, not an independently-computed generic offset; the symbol owns its `PinPositions`, returned from `symbol::draw`. A symbol is defined once in its canonical `Rotation::Deg0` layout; `symbol::rotate` rotates every point of it (geometry and pins together) for the other 3 rotations, so the icon and its pins always agree at every rotation instead of only the pins moving. `BOX_SIZE` (`canvas.rs`) is deliberately sized so its half-extents are whole multiples of `GRID_SPACING` — since a symbol's pins sit at the box's own edges/corners, this is what keeps every pin landing exactly on a grid dot. **Going forward, every new component gets a real symbol here by default** — falling back to placeholder/no-symbol rendering only if Romain explicitly asks for it, or if a sensible symbol genuinely doesn't exist yet for that kind.
- [x] **All 8 v1 logic gates**: `And`/`Or`/`Nand`/`Nor`/`Xor`/`Xnor`/`Not`/`Buffer` (`simlogix-core/src/components/`), each combinational (no internal state). 2-input gates resolve uncertain inputs by dominance (e.g. `And`: a definite `Low` on either input forces `Low` regardless of the other; short of that, `Error` dominates; short of that, anything but both definite is `Unknown`) — mirrored for `Or`/`Nand`/`Nor`. `Xor`/`Xnor` have no dominant input (either input alone still leaves the output dependent on the other), so both inputs must be definite to resolve; `Error` still dominates over plain uncertainty. `Not` inverts (`HighZ` and `Unknown` both resolve to `Unknown`); `Buffer` passes every signal state through unchanged. New "Gates" palette category. GUI-side, this is fully generic — adding a gate of the same shape needs no new `PlacedComponent` variant:
  - **2-input gates** (`PlacedComponent::TwoInputGate`, `placed_component.rs`): two inputs at pin index 0/1, one output at pin index 2 — same shape for `And`/`Or`/`Nand`/`Nor`/`Xor`/`Xnor`, added via `PlacedComponent::two_input_gate(id, center, kind)`.
  - **1-input gates** (`PlacedComponent::OneInputGate`): one input at pin index 0, one output at pin index 1 — `Not`/`Buffer`, via `PlacedComponent::one_input_gate(id, center, kind)`.
  - Symbols (`symbol.rs`) share drawing code within each shape family via an `invert: bool` (and `xor: bool` for the OR family) parameter, plus a shared `bubble_end` helper that draws the small inversion "bubble" and returns where the lead past it continues — `draw_and_gate(..., invert)` covers And/Nand, `draw_or_gate(..., xor, invert)` covers Or/Nor/Xor/Xnor, `draw_triangle_gate(..., invert)` covers Buffer/Not. Mirrors the existing `draw_transistor(..., is_n_type)` pattern.
  - Adding a genuinely new shape (a gate that doesn't fit either family) still follows the checklist: `ComponentKind` variant in `palette.rs` (+ its `category_gates` entry), a `component_xxx` label in `i18n.rs` (3 translations), a `draw_xxx` in `symbol.rs`, and a `place()` arm in `app.rs`.
  - `Not`/`Buffer` (single input) aren't started — same overall recipe, but they don't fit `TwoInputGate`'s shape, so they'll need their own `PlacedComponent` variant (or a generalized one covering both arities, if that turns out cleaner once it exists).
