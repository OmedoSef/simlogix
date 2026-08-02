# Project scope

## In scope for v1 (minimal simulator)

- Basic gates: AND, OR, NOT, NAND, NOR, XOR, XNOR, buffer.
- Bidirectional/tri-state buffer (bus transceiver) — needs multi-driver net resolution, see [simulation-engine.md](../architecture/simulation-engine.md#data-model).
- Input (switch/button), output (LED), clock.
- Schematic editing: placement, wire routing with grid snapping and orthogonal routing, rotation, multi-select/move.
- Real-time simulation wired into the UI loop.
- Save/load a circuit (via `serde`).

## Out of scope for v1 (future roadmap, not being built now)

- A custom appearance/symbol editor for components — v1 uses an auto-generated appearance (box + named pins). This addresses one of the original pain points with Logisim, but only after the core simulator is solid.
- Multi-bit buses, memory (RAM/ROM), VHDL/FPGA export, collaboration features.

This mirrors the scope decisions in the project's internal [CLAUDE.md](../../CLAUDE.md#v1-scope--out-of-scope) — check there if this page and the code ever seem to disagree, and update both together.
