# Building and running

Run these commands from inside the devcontainer (a VS Code integrated terminal opened after "Reopen in Container" is already inside it).

## The workspace

The repository is a Cargo workspace with two crates:

- `simlogix-core` — the circuit model and simulation engine, no GUI dependency.
- `simlogix-gui` — the schematic editor and native window, built with `eframe`/`egui`.

## Common commands

Build everything:

```bash
cargo build
```

Run the tests for `simlogix-core`:

```bash
cargo test -p simlogix-core
```

Run the GUI (opens a native window, forwarded to the host display via X11):

```bash
cargo run -p simlogix-gui
```

Format and lint before considering a change done (see [code conventions](../contributing/code-conventions.md)):

```bash
cargo fmt
cargo clippy
```
