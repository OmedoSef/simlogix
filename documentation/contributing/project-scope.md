# Project scope

## In scope for v1 (minimal simulator)

- **Done** — basic gates: AND, OR, NOT, NAND, NOR, XOR, XNOR, buffer.
- **Done** — input (switch/button), output (LED), clock.
- **Done** — schematic editing: placement, rotation, and wire routing with grid snapping (see [Wiring](../using/wiring.md)). Multi-select is still outstanding.
- **Done** — real-time simulation wired into the UI loop.
- **Done** — save/load a circuit (via `serde`), as a versioned project file.
- **Not yet** — bidirectional/tri-state buffer (bus transceiver). The groundwork is there — `Signal::HighZ`, `PinDirection::InOut`, and multi-driver net resolution (see [simulation-engine.md](../architecture/simulation-engine.md#data-model)) — and `Transistor` already drives `HighZ`, but no component exposes a true bidirectional pass yet.

## Out of scope for v1 (future roadmap, not being built now)

- A custom appearance/symbol *editor* for components — v1 ships a hand-drawn vector symbol per component kind instead (see `simlogix-gui/src/symbol.rs`), not a UI for drawing your own. Letting users draw their own addresses one of the original pain points with Logisim, but comes after the core simulator is solid.
- Multi-bit buses, memory (RAM/ROM), VHDL/FPGA export, collaboration features.
- Sub-circuit hierarchy — several circuits in one project, used as components inside each other. The project file format already allows for it; the editor only ever reads and writes one circuit today.

This mirrors the scope decisions in the project's internal [CLAUDE.md](../../CLAUDE.md#v1-scope--out-of-scope) — check there if this page and the code ever seem to disagree, and update both together.
