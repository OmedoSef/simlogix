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
  - `Signal::{High, Low, Unknown, Error}` — the unknown/error state is planned from the start (one of the "simulation bug" irritants often comes from mishandling X/Z).
  - `Pin`: input/output of a component, connected to a `Net`.
  - `Component`: a trait with `eval(&self, inputs) -> outputs` + `propagation_delay()`. Sub-circuits also implement this trait (hierarchy is a first-class citizen, not a hack).
  - `Circuit`: graph of components + nets, event queue, logical clock.
- **Dev container** (`.devcontainer/`): `rust:1-slim-bookworm` image + X11/GL/GTK libs needed by eframe (`libx11-dev`, `libxkbcommon-dev`, `libgl1-mesa-dev`, `libgtk-3-dev` for future native file dialogs via `rfd`, etc.), `clippy`/`rustfmt` installed. The host's X11 socket (`/tmp/.X11-unix`) is mounted and `DISPLAY` propagated, so `cargo run` from the container opens the window directly on the host desktop (X11 forwarding). `remoteUser: vscode` to stay consistent with the pattern already used on `file-checker`. The container is only for the build/dev toolchain (source code is bind-mounted by VS Code, not copied into the image).
  - Practical setup instructions (`xhost` prerequisite, how to open the devcontainer): see [README.md](README.md), not duplicated here.

## v1 scope / Out-of-scope

**In v1 (minimal simulator):**
- Basic gates: AND, OR, NOT, NAND, NOR, XOR, XNOR, buffer.
- Input (switch/button), output (LED), clock.
- Schematic editing: placement, wire routing with grid snapping and orthogonal routing, rotation, multi-select/move.
- Real-time simulation wired into the UI loop.
- Save/load a circuit (serde).

**Out of scope for v1 (future roadmap, not to be built now):**
- Custom appearance/symbol editor for a component — v1 uses an auto-generated appearance (box + named pins). This is Romain's pain point #2, addressed after the core is solid.
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
- **Tests**: unit tests co-located in a `#[cfg(test)] mod tests` module at the bottom of the file they test (pattern already used in `simlogix-core/src/lib.rs`).
- **Module organization**: while `simlogix-core` stays small, everything can live in `lib.rs`. Once a concept (Signal, Pin, Component, Circuit...) exceeds ~a few dozen lines, it moves into its own file (`src/signal.rs`, etc.) instead of letting `lib.rs` grow indefinitely.

## Progress

- [x] Project and architecture framing (this document).
- [x] Git + devcontainer scaffold (Rust toolchain, X11 GUI passthrough).
- [x] Rename folder/repo `new-logisim` → `simlogix`.
- [x] README.md (practical setup) split from CLAUDE.md (decisions/context).
- [x] Cargo workspace scaffold (`simlogix-core`, `simlogix-gui`).
- [x] Minimal eframe/egui GUI shell ("Hello, SimLogix!" displayed, X11 forwarding validated — required adding `libxkbcommon-x11-dev` to the devcontainer).
- [x] `documentation/` folder started (getting-started, architecture, contributing).
- [ ] `simlogix-core` engine (data model + discrete events) — for now just a `hello()` + test, no real model yet.
- [ ] Feedback-loop tests (SR-NAND latch, ring oscillator).
- [ ] Editor interaction (placement, wire routing, snapping, rotation, selection).
- [ ] Real-time simulation ↔ UI integration.
- [ ] Save/load a circuit.
