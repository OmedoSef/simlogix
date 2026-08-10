# Dev tooling

A few things beyond the [code](code-conventions.md) and [commit](commit-conventions.md) conventions, meant to catch problems locally before they reach a commit or a reviewer.

## `rust-toolchain.toml`

Pins the exact Rust toolchain (currently `1.97.1`, plus the `clippy`/`rustfmt` components) regardless of what the devcontainer's base image (`rust:1-slim-bookworm`) happens to resolve to on a given day. `rustup` picks this file up automatically inside the devcontainer.

## `.editorconfig`

Consistent whitespace/indentation across editors for files `rustfmt` doesn't touch (`.toml`, `.md`, `.yml`, `.json`). Rust files are still governed by `rustfmt`, not this file.

## Pre-commit hook

[.githooks/pre-commit](../../.githooks/pre-commit) runs on every commit (wired up the same way as the [commit-msg hook](commit-conventions.md#enforcement), via `core.hooksPath`):

- `cargo fmt --all -- --check` — **blocks the commit** if code isn't formatted.
- `cargo clippy --workspace --all-targets` — runs and prints warnings, but doesn't block the commit on lints (matches the [current decision](code-conventions.md#formatting-and-linting) not to `deny(warnings)` yet). It does block on compile errors.

## Interface-level tests

`simlogix-gui/src/ui_tests.rs` runs the whole application through a real
egui pass, using [`egui_kittest`](https://docs.rs/egui_kittest). It exists
because a run of bugs — a group drag that came apart, copy/paste that never
fired, gestures a mode was supposed to have removed — were all in the
*wiring* between correct pieces, and every unit test stayed green through
them.

Two things about how they are written:

- **A rule about one function belongs beside that function.** These are for
  behaviour that only exists once the pieces are assembled; repeating a unit
  test here would only mean two places to change it.
- **Each one was checked to fail against the bug it describes**, by putting
  the bug back and running it. A regression test that has never failed proves
  nothing.

They are declared as a child module of `app` with a `#[path]`, so they can
read private fields without any accessor being published for their sake.
Use `Harness::run_steps`, not `Harness::run`: this application asks for a
repaint every frame — which is what keeps a clock ticking — so it never
settles, and `run` gives up after a few tries.

## `scripts/set-format-version.py`

```bash
scripts/set-format-version.py ~/Projects/CPU/base_component.slgx 9
```

Stamps a project file with a different format version, for carrying one to
a machine still running an older release. The version is what a build
checks before reading anything else, and it refuses a document newer than
itself — so a file saved from a working tree cannot be opened by the last
release until that release catches up.

Only the stamp changes. Nothing is stripped, so anything the older build
cannot read is still in the file and comes back when you open it here
again — but that build **writes back only what it understood**, so saving
there drops it for good. Hence the backup, written beside the file, which
is an ordinary project: rename it to open it, or type its full name into
the open dialog, since the format is recognised from the bytes rather than
the extension.

It reads the result back before reporting success, and refuses if any entry
other than `project.json` came out changed. It runs on the only copy of
someone's work, so it does not get to assume the write went as intended.

This exists because the alternative — cutting a release every time the
format gains a field — makes the release the thing that has to wait.

## `cargo-audit`

Installed in the devcontainer image, scans `Cargo.lock` against the [RustSec advisory database](https://rustsec.org/) for known vulnerabilities and unmaintained crates:

```bash
cargo audit
```

Not run automatically on commit — it fetches/updates the advisory database over the network each time, which is too slow for a commit hook. Run it manually now and then, and it'll be added to CI once that's set up.
