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

## `cargo-audit`

Installed in the devcontainer image, scans `Cargo.lock` against the [RustSec advisory database](https://rustsec.org/) for known vulnerabilities and unmaintained crates:

```bash
cargo audit
```

Not run automatically on commit — it fetches/updates the advisory database over the network each time, which is too slow for a commit hook. Run it manually now and then, and it'll be added to CI once that's set up.
