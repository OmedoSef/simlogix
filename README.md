# SimLogix

A cross-platform digital logic simulator written in Rust, built to fix the
frustrations of working with Logisim: clumsy canvas interaction, feedback
loops the engine mishandles, and a dated interface.

## What it does

- **Schematic editing** — place components, wire them click by click,
  reshape and colour the wires, select several things at once, copy and
  paste, undo everything.
- **Real-time simulation** — a discrete-event engine with propagation
  delays, so a combinational feedback loop (an SR latch built from NAND
  gates, a ring oscillator) settles or oscillates instead of hanging. An
  oscillation that never settles is reported rather than freezing the UI.
- **Stepping** — one tick, straight to the next event, or one clock edge at
  a time, plus a speed control. A circuit whose clock arrives on a port can
  be stepped too: you are its clock.
- **Signals that admit what they don't know** — `High`, `Low`, `Unknown`,
  `Error`, `HighZ`, plus the weak levels a single transistor really passes.
- **A full v1 component set** — the eight logic gates, transistors, rails,
  buttons, switches, clocks, a three-position source, LEDs, probes, an SR
  latch, a tri-state buffer and a bidirectional bus transceiver in both
  enable polarities.
- **Hierarchy** — a project holds many circuits, filed in folders, and any
  of them can be placed inside another. Give a circuit a symbol of your own
  or let one be generated.
- **Projects** — a single `.slgx` file, readable with any zip tool.

## Requirements

- Docker, plus VS Code with the
  [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)
  extension. The project is developed inside a devcontainer that carries the
  Rust toolchain.
- On the host, before opening the devcontainer, allow local X11 connections
  so the application window can appear from inside the container:

  ```bash
  xhost +local:docker
  ```

## Getting started

1. Open the folder in VS Code.
2. Command palette → *Dev Containers: Reopen in Container*.
3. Run it:

   ```bash
   cargo run -p simlogix-gui
   ```

A release build produced in the container also runs on the host directly —
the binary lands in `target/release/simlogix-gui`, which is bind-mounted:

```bash
cargo build --release
```

## Licence

SimLogix is offered under the [MIT licence](LICENSE): reuse it, change it,
ship it in something of your own — keep the copyright notice with it.

It is built on a good deal of other people's work, listed with the terms
each part is offered under in [THIRD-PARTY.md](THIRD-PARTY.md). The same
list is in the application, under **? → Licences**, searchable. Both are
generated; after changing a dependency, run:

```bash
cargo run -p simlogix-gui --bin write-licenses -- THIRD-PARTY.md assets/third-party.json
```

## Installing

Each release carries a portable archive and a `.deb` for Debian and Ubuntu
— see the [releases page](https://github.com/OmedoSef/simlogix/releases).

**Linux only, for now.** The Windows and macOS builds are switched off
rather than removed: nobody here has either machine, so nothing they
produced could be *tried* before it reached you, and an artefact nobody has
run is a promise made on the strength of it having compiled. They come back
the day there is a way to test what comes out. Building from source works on
all three in the meantime.

## Documentation

- Using the editor, architecture, and contributor conventions: the
  [documentation/](documentation/README.md) folder.
- What isn't built yet, and in roughly what order: [ROADMAP.md](ROADMAP.md).
- Project context, decisions and progress (internal notes):
  [CLAUDE.md](CLAUDE.md).
