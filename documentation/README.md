# SimLogix Documentation

This folder holds the documentation for users and contributors. It's a work in progress and grows alongside the code — sections describing unbuilt features are marked as **planned**.

- **Getting started**
  - [Installation](getting-started/installation.md) — prerequisites and opening the devcontainer.
  - [Building and running](getting-started/building-and-running.md) — building, testing, and running each crate.
- **Using SimLogix**
  - [Editor basics](using/editor-basics.md) — tools, placing components, selecting several, moving, copy/paste, the view, settings, keyboard reference.
  - [Wiring](using/wiring.md) — drawing and reshaping wires, loose ends, junctions.
  - [Running a circuit](using/simulation.md) — run/pause, signal colours, unstable circuits.
  - [Projects, saving and undo](using/files-and-history.md) — undo/redo, save/load, the file format.
- **Architecture**
  - [Overview](architecture/overview.md) — workspace layout, crate responsibilities, GUI toolkit choice.
  - [Simulation engine](architecture/simulation-engine.md) — discrete-event model, signal states, feedback-loop handling.
- **Contributing**
  - [Code conventions](contributing/code-conventions.md) — language, error handling, panics, formatting, tests, module layout.
  - [Commit conventions](contributing/commit-conventions.md) — Conventional Commits format, types, scopes, enforcement hook.
  - [Dev tooling](contributing/dev-tooling.md) — toolchain pinning, editorconfig, pre-commit hook, dependency auditing.
  - [Project scope](contributing/project-scope.md) — what's in and out of scope for v1.

For the project's internal decision log, rationale, and progress tracking, see [CLAUDE.md](../CLAUDE.md) at the repo root (kept for project continuity across sessions/machines, not aimed at outside readers).
