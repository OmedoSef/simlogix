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

## Running a build directly on the host

`cargo build --release` (still from inside the devcontainer) produces `target/release/simlogix-gui`. Since the repo is bind-mounted, that binary is already visible at the same path on the host — no copy needed:

```bash
target/release/simlogix-gui
```

This works because the devcontainer's Debian bookworm ships an older glibc than most host Linux distros, and glibc is forward-compatible (a binary built against an older glibc runs fine on a system with a newer one, not the other way around). If your host's glibc turns out to be *older* than the container's, this won't work — check with `ldd --version` on both sides.
