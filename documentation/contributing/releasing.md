# Continuous integration and releases

## What runs, and when

| Workflow | When | What it does |
|---|---|---|
| [CI](../../.github/workflows/ci.yml) | Every push to `main`, every pull request | `fmt --check` and `clippy -D warnings` on Linux; `cargo test` and a full build on Linux, Windows and macOS. |
| [Audit](../../.github/workflows/audit.yml) | Weekly, and whenever a manifest or the lockfile changes | `cargo audit` against the RustSec advisory database. |
| [Release](../../.github/workflows/release.yml) | Pushing a `v*` tag | Builds every artefact and publishes a GitHub release. Can also be run by hand, which builds without publishing. |

There was deliberately no CI until the project was ready to be released. While
Romain was the only person pushing and the local hooks ran on every commit, a
pipeline would have been guarding against a contributor who didn't exist. The
trigger recorded for adding one was *deciding to ship a version to the world*,
and that is what happened.

### Two deliberate differences from the local hooks

**Clippy is blocking here, and informational in the pre-commit hook.** A hook
that refuses a commit interrupts a train of thought; a pull request is exactly
where an unaddressed lint should be caught. The toolchain is pinned in
`rust-toolchain.toml`, which is what makes `-D warnings` safe — a new compiler
release cannot turn the build red on its own.

**The audit is not part of CI.** The advisory database changes daily and the
code does not, so an advisory published overnight would redden a pull request
that changed nothing — and a red mark that means "not your fault" is a red
mark people learn to ignore. On a schedule it arrives as information.

## Cutting a release

```bash
scripts/release.sh 0.2.0
```

Run it inside the devcontainer — the checks need cargo, and so does the
commit hook. It sets the version, refreshes the lockfile, runs everything CI
will run, shows you what it is about to do, asks, and then commits, tags and
pushes.

| Option | What it does |
|---|---|
| `--dry-run` | Says what would happen and changes nothing. |
| `--no-push` | Commits and tags locally; prints the push command. |
| `--no-verify` | Skips the checks. They are the point, so this is for a re-run after they have already passed. |

**The version lives once**, as `version` under `[workspace.package]` in the
root `Cargo.toml`. Both crates inherit it with `version.workspace = true`,
cargo turns it into `CARGO_PKG_VERSION`, and that is what the About window
shows — so setting it there is what changes the number a user sees.

The script refuses rather than surprises: it will not run off `main`, with a
dirty tree, onto a tag that already exists locally or on the remote, or with
a version that is not semantic. It also compares `THIRD-PARTY.md` against a
freshly generated one and stops if a dependency moved without the notices
being regenerated — shipping stale attribution is shipping the wrong
attribution.

Everything before the confirmation is undone if a check fails, so a failed
run leaves the tree exactly as it found it.

The tag is the decision; the workflow only carries it out. Nothing in
`.github/` has to be edited to cut a release.

## What a release contains

| Artefact | Platform |
|---|---|
| `simlogix-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | Linux, portable |
| `simlogix-vX.Y.Z-x86_64-pc-windows-msvc.zip` | Windows, portable |
| `simlogix-vX.Y.Z-aarch64-apple-darwin.tar.gz` | macOS, Apple silicon |
| `simlogix-vX.Y.Z-x86_64-apple-darwin.tar.gz` | macOS, Intel |
| `simlogix.deb` | Debian, Ubuntu |
| `simlogix.msi` | Windows, installer |

Both macOS builds are shipped because an Intel Mac cannot run the arm64
binary, and Rosetta is not something a user should discover from a crash.

Every archive carries `LICENSE` and `THIRD-PARTY.md` alongside the binary.
The notices are compiled into the application as well, but an archive is a
copy being distributed, and attribution has to travel with a copy.

## The Debian package

Built by [`cargo-deb`](https://github.com/kornelski/cargo-deb) from
`[package.metadata.deb]` in `simlogix-gui/Cargo.toml`. It installs the binary
as `/usr/bin/simlogix`, a desktop entry, an icon, and a MIME type so that
double-clicking a `.slgx` file opens it.

Two things there are worth knowing before changing them.

**The dependency list is written out by hand, not left to `$auto`.** Reading a
binary's `NEEDED` entries finds only what it links against, and this one links
against almost nothing: X11, Wayland, EGL and xkbcommon are all opened with
`dlopen` at run time. `$auto` alone produced a package depending on libc and
nothing else — one that installs cleanly and then won't start, which is worse
than one that refuses to install. The list comes from the `.so` names in the
binary itself:

```bash
strings target/release/simlogix-gui | grep -oE 'lib[A-Za-z0-9_.+-]*\.so(\.[0-9]+)?' | sort -u
```

**The assets are listed explicitly**, which also settles what must *not* ship.
With no list, `cargo-deb` installs every binary the crate builds — and
`write-icon` and `write-licenses` are build tools with no business on
anyone's `PATH`.

Vulkan and the desktop portal are `Recommends`, not `Depends`: there is a GL
path behind Vulkan, and without a portal the application runs perfectly well
with only its file dialog missing.

To build and check one locally:

```bash
cargo deb -p simlogix-gui
sudo apt install ./target/debian/simlogix_*.deb
desktop-file-validate /usr/share/applications/simlogix.desktop
```

## The Windows installer

Built by [`cargo-wix`](https://github.com/volks73/cargo-wix) from
`simlogix-gui/wix/main.wxs`, which is **checked in and edited**, not
generated during the build. `cargo wix init` writes a template that suits a
command-line tool: it installs every binary the crate builds, adds the install
directory to `PATH`, and has no shortcut. The version here installs one
binary as `SimLogix.exe`, adds a Start Menu entry with an icon, ships
`THIRD-PARTY.md` beside it, and leaves `PATH` alone. Running `cargo wix init`
again would overwrite all of that.

The icon comes from `assets/icon.ico`, written by the same `write-icon` tool
as the PNG — an `.ico` is a twenty-two byte header around a PNG, so there is
no second rasteriser and no second encoder.

One thing to know when editing that file: **a double hyphen is illegal inside
an XML comment**, so a comment mentioning something like `cargo run --bin`
makes the whole document malformed.

## Known gaps

**Neither macOS artefact is signed or notarised.** An unsigned application
downloaded on a recent macOS is refused by Gatekeeper until the user clears it
by hand. Signing needs a paid Apple developer account, so this is a decision
to take rather than an oversight; until then, the macOS archives are for
people willing to work around it.

**The Windows MSI is unsigned too**, so SmartScreen will warn on first run.

**macOS gets a bare binary, not an `.app` bundle**, so there is no icon in the
Dock and no Finder integration. A bundle is the right thing and is a separate
piece of work.

**The Windows binary carries no embedded icon**, so Explorer and the taskbar
show a generic one — the Start Menu shortcut has the right icon because the
installer sets it there. Embedding one into the executable needs a build
script and a build-dependency, which is a decision to take on its own.

**None of the Windows or macOS paths have been run.** They were written on a
Linux machine, and the first real evidence will be the first workflow run.
The Linux ones were exercised: the Debian package was built, installed,
listed and removed, and its desktop entry validated.
