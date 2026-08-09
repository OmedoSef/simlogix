# Licences

## SimLogix's own terms

SimLogix is offered under the **MIT licence** — the full text is in
[LICENSE](../LICENSE) at the repository root, and in the application under
**? → Licences**.

In short: you may use, copy, change, redistribute and sell it, including as
part of something closed, on one condition — the copyright notice and the
permission notice travel with it. That is the "keep the attribution" bargain,
and it is the whole of it.

MIT was chosen over the Rust ecosystem's usual MIT-or-Apache-2.0 pair for one
reason: it says exactly what was wanted and nothing else. Apache-2.0's extra
provision is an explicit patent grant, which matters when contributors hold
patents that read on the code. Adding it later is always possible — a
copyright holder may offer further terms at any time — while taking a licence
away is not.

## What SimLogix is built on

Every dependency, its version and the terms it is offered under are listed in
[THIRD-PARTY.md](../THIRD-PARTY.md), together with the licence text each one
ships. The same list is in the application, under **? → Licences**, on the
*Third-party* tab.

It is in **both** places on purpose. The obligation these licences carry is
attribution, and attribution has to reach whoever ends up with a copy: a file
beside a binary can be separated from it, so the text is compiled in as well.

Nothing SimLogix depends on is copyleft. Everything is permissive — MIT,
Apache-2.0, BSD, ISC, Zlib, Unicode-3.0 and a couple of public-domain
dedications — so a released binary carries no obligation beyond passing those
notices along, which is what the file and the window do.

## Regenerating the notice

`THIRD-PARTY.md` is generated, not written by hand. After adding, removing or
upgrading a dependency:

```bash
cargo run -p simlogix-gui --bin write-licenses -- THIRD-PARTY.md
```

The tool reads `cargo metadata`, walks the **normal** dependency graph out
from this workspace's crates, and collects each crate's licence files from the
cargo registry. Dev-dependencies and build-dependencies are left out: they
build the tests and run at build time, and neither reaches a released binary.

Identical licence texts are printed once, with every crate that ships them
listed above. Apache-2.0 is byte-identical wherever it appears; MIT differs by
its copyright line, and so genuinely repeats — that line *is* the attribution.

A dependency that declares no licence at all is written into the table as
`NOT DECLARED` rather than guessed at, and a test fails on it. That is a
decision to make, not a line to scroll past.
