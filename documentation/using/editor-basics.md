# Editor basics

The window is split into a menu bar, a toolbar above the canvas, the circuit
tree and component palette down the left side, the canvas itself, and a
status bar along the bottom.

The **status bar is worth watching**: it always describes what the current
selection or mode lets you do, so most of what follows can be discovered
without this page.

## Tools

The toolbar holds the interaction modes — what a click on the canvas *does*:

| Tool | What clicking the canvas does |
|---|---|
| **Select** | Select and move things. The resting state. |
| **Wire** | Start drawing a wire, anywhere — see [Wiring](wiring.md). |

Choosing a component in the palette is a third, temporary mode: the next
click drops that component, then you fall back to Select.

Pins always start a wire when clicked, whichever tool is active — you never
have to switch to the wire tool just to connect two components.

## Placing components

Click a component in the palette, then click the canvas. A translucent
preview follows the pointer at the exact grid position it will land on, so
placing isn't a blind click.

**Hold Shift while clicking to keep placing the same kind** — a row of LEDs
is one trip to the palette rather than one per component. Release Shift on
the last one to drop back to Select.

Clicking the highlighted palette entry again cancels it, as does Escape.

The palette is grouped into **Sources** (Button, Clock, GND, PWR),
**Outputs** (LED, Probe), **Transistors** (NMOS, PMOS) and **Gates**
(AND, OR, NAND, NOR, XOR, XNOR, NOT, Buffer). Each category folds away, and
the panel edge can be dragged to resize it.

## Selecting, moving, rotating, deleting

- **Select** — click a component or a wire. A selected item gets a blue
  outline; hovering shows a fainter one, so you can tell what a click is
  about to take.
- **Deselect** — click empty canvas, or press Escape, or right-click.
- **Move** — drag a component. It follows the pointer freely and snaps to
  the grid when you let go, which keeps dragging smooth instead of jerky.
- **Rotate** — `R` with a component selected. The symbol turns a quarter
  turn clockwise, pins included.
- **Delete** — `Delete` or `Backspace`. A selected wire takes priority over
  a selected component, and the two are never selected at once.

Deleting a component **keeps the wires that were attached to it**, cut loose
where the pin used to be. Swapping one gate for another is then a matter of
dragging those loose ends onto the replacement rather than redrawing them.

## Moving around the canvas

- **Zoom** — mouse wheel, anchored on the pointer: whatever is under the
  cursor stays under it.
- **Pan** — drag the empty background. Dragging a component or a wire point
  moves that instead, since those claim the gesture first.

The view is not part of the circuit: it isn't saved in the project file, and
undo won't move you somewhere else.

## Settings

**Settings → Theme** follows the operating system by default, and can be
forced to light or dark. **Settings → Language** offers English, French and
Italian, defaulting to the system locale. Neither affects what a project
file means — the language you edit in is invisible to the saved circuit.

## Keyboard reference

| Key | Action |
|---|---|
| `R` | Rotate the selected component |
| `Delete` / `Backspace` | Delete the selection |
| `Escape` | Cancel the wire being drawn, otherwise clear the selection |
| `Enter` | Finish the wire being drawn, leaving its end loose |
| `Space` | Run / pause the simulation |
| `Shift` (held) | Keep placing the same component |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Redo |
| `Ctrl+N` / `Ctrl+O` | New / open project |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / save as |

## Component properties

Select a component and the panel on the right shows what can be set on it.
Everything there is optional: leave a property alone and the component
behaves exactly as it always has, and nothing about it is written to the
project file.

| Property | Applies to | What it does |
|---|---|---|
| **Name** | every component | Drawn under the symbol, as your own annotation. |
| **Pressed at rest** | Button | The button rests pressed, so clicking it *releases* it — a normally-closed switch. |
| **Colour** | LED | What it glows when lit. *Reset* puts it back to red. |

The button's setting is its **resting** state, not its current one. Runtime
state is still never saved: opening a project starts every button at rest,
which is now whatever you chose rather than always released.

Setting a property is an ordinary edit, so `Ctrl+Z` undoes it. Typing a name
counts as one step from the moment you click into the field; the colour
picker leaves a step per change while you drag through it.
