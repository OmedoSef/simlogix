# Code conventions

## Language

Identifiers (types, functions, variables) and comments/doc-comments (`///`, `//!`) are in English. Project-internal docs (`CLAUDE.md`, root `README.md`) stay in French.

## Error handling

Custom errors use an `enum` with [`thiserror`](https://docs.rs/thiserror) — no raw `String` errors, and no `anyhow` inside `simlogix-core`. It's a library; callers need to be able to match on specific error variants.

## Panics

`panic!`, `unwrap()`, and `expect()` are not allowed outside `#[cfg(test)]`. An invalid input or malformed circuit must always surface as a `Result`, never a panic.

## Formatting and linting

Run `cargo fmt` (default configuration, no `rustfmt.toml`) and `cargo clippy` before considering a change done. No custom clippy configuration for now (this may tighten later — e.g. `deny(warnings)`, `pedantic`).

## Tests

Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the file they test (see `simlogix-core/src/lib.rs`).

## Module organization

While `simlogix-core` stays small, everything can live in `lib.rs`. Once a concept (`Signal`, `Pin`, `Component`, `Circuit`, ...) grows past a few dozen lines, it moves into its own file (`src/signal.rs`, etc.) instead of letting `lib.rs` grow indefinitely.
