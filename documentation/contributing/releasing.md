# Continuous integration and releases

## What runs, and when

| Workflow | When | What it does |
|---|---|---|
| [CI](../../.github/workflows/ci.yml) | Every push to `main`, every pull request | `fmt --check` and `clippy -D warnings`, then `cargo test` and a full build. Linux only — see below. |
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

## The release notes

Written by [`scripts/changelog.sh`](../../scripts/changelog.sh) from the
commit subjects between the previous tag and this one, grouped into
**Breaking changes**, **Features**, **Fixes**, **Performance**,
**Documentation** and **Other changes**, with the Full Changelog link at the
end. Preview what a release would say before tagging it:

```bash
scripts/changelog.sh            # from the last tag to HEAD
scripts/changelog.sh v0.2.3     # what that release said
```

Only the **subject line** is used, with its Conventional-Commits prefix
stripped — so `fix(gui): a symbol's box no longer reaches past the drawing`
becomes one bullet under *Fixes*. The body is where the reasoning lives, and
release notes are not the place to re-read it.

`chore`, `ci` and `test` are left out. They describe work on the project
rather than changes to the thing being released, and a list padded with them
is one nobody finishes reading. A type the script doesn't recognise lands
under *Other changes* rather than being dropped — an unfamiliar prefix in the
wrong section is better than a change that silently isn't there, which also
means `build`, `style` and `refactor` still show up there if you'd rather
they didn't.

A commit marked breaking with `!` appears at the top **and** in its own
section: it is still a feature or a fix, and it is also the one thing a
reader must not miss.

This replaced `gh --generate-notes`, which lists every commit verbatim with
no way to group or filter them. The script writes the Full Changelog link
itself, so the notes are one thing decided in one place.

## What a release contains

| Artefact | Platform |
|---|---|
| `simlogix-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | Linux, portable |
| `simlogix.deb` | Debian, Ubuntu |

### Windows and macOS are switched off

They were shipped until v0.6.0 — a portable archive for Windows, one for
each macOS architecture, and an `.msi`. Nobody here has either machine, so
nothing they produced could be *tried* before it reached someone, and an
artefact nobody has run is a promise made on the strength of it having
compiled. Every release spent runner minutes keeping that promise standing.

**They are commented out, not deleted.** The matrix entries in
[`release.yml`](../../.github/workflows/release.yml) and the whole
`windows-installer` job are there, and so is everything they need:
`wix/main.wxs`, the archive step's Windows branch, both Apple targets. To
bring them back, uncomment the three matrix entries and that job, and put
`windows-installer` back in `publish`'s `needs` — it was taken out because a
release would otherwise wait forever on a job that never runs.

**CI is Linux-only too**, switched off the same way and in the same commit.
It was a separate question and worth asking on its own: building on three
platforms is a check that the code still *compiles* everywhere, which is not
the same as promising an artefact. What settled it is that nothing is being
shipped for those platforms any more, so the check was guarding a promise
that is no longer made — and it was three runners on every push, where a
release is rare.

The cost is real and worth naming: code that stops compiling off Linux will
now surface on the day those releases come back, rather than on the push that
caused it. Put the two entries back on the `os:` line and the check returns
exactly as it was.

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

Two things to know when editing that file.

**A double hyphen is illegal inside an XML comment**, so a comment mentioning
something like `cargo run --bin` makes the whole document malformed.

**Every source path in it is resolved against the current directory** — the
process's, not the package cargo-wix was told to build. So the job runs from
`simlogix-gui/` *and* passes `--package simlogix-gui`: the flag because
cargo-wix refuses to guess a package once it sees a workspace, and the
directory because otherwise even `wix\License.rtf` — the path the generated
template writes for itself — cannot be found.

## Known gaps

The first four are about Windows and macOS, which are **switched off** — so
they are the shape of the work waiting the day those come back, rather than
something anyone downloading a release meets today.

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

**Nobody has ever run what the Windows or macOS paths produced.** They built
on every release up to v0.6.0 — publishing waited on them, so a failure
would have stopped it — but building is not running, and there is no machine
here to try the result on. That is the whole reason they are switched off:
the artefacts were a promise made on the strength of having compiled.

The Linux ones were exercised properly: the Debian package was built,
installed, listed and removed, and its desktop entry validated.
