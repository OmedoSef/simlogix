# Code conventions

## Language

Identifiers (types, functions, variables) and comments/doc-comments (`///`, `//!`) are in English. Root `README.md` stays in French; `CLAUDE.md` is in English.

## Error handling

Custom errors use an `enum` with [`thiserror`](https://docs.rs/thiserror) — no raw `String` errors, and no `anyhow` inside `simlogix-core`. It's a library; callers need to be able to match on specific error variants.

## Panics

`panic!`, `unwrap()`, and `expect()` are not allowed outside `#[cfg(test)]`. An invalid input or malformed circuit must always surface as a `Result`, never a panic.

## Formatting and linting

Run `cargo fmt` (default configuration, no `rustfmt.toml`) and `cargo clippy` before considering a change done. No custom clippy configuration for now (this may tighten later — e.g. `deny(warnings)`, `pedantic`).

## Tests

Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the file they test, preceded by a banner comment (see `simlogix-core/src/signal.rs`):

```rust
// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
```

## Module organization

One file per concept: `signal.rs`, `pin.rs` (`Pin`, `PinDirection`), `net.rs` (`NetId`), `component.rs` (the `Component` trait only), `circuit.rs`. `lib.rs` stays crate doc + `mod`/`pub use` declarations, nothing else. Once there are several concrete components (gates, `Button`, `Led`, ...), they go under a `components/` subfolder rather than piling into `component.rs`.
