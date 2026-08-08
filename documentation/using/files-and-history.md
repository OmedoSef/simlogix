# Projects, saving and undo

## Undo and redo

`Ctrl+Z` undoes, `Ctrl+Shift+Z` or `Ctrl+Y` redoes; both are also in the
**Edit** menu, greyed out when there's nothing to step to. Placing, moving,
rotating, wiring, reshaping and deleting are all covered.

One thing to expect: **undo restarts the simulation cold**. A held button is
released, a clock's phase resets. Undo is defined in terms of the saved
document — the circuit's structure — and runtime state was never part of
that.

## Saving and loading

**File → Save** (`Ctrl+S`) writes back to the current file, asking for a
path only the first time. **Save As** (`Ctrl+Shift+S`) always asks.

The window title shows which file you're editing and an asterisk while it
has unsaved changes:

```
SimLogix — half-adder.simlogix*
```

Anything that would discard unsaved work — New, Open, or closing the window
— asks first, offering to save, discard, or cancel. Cancelling the save
dialog cancels the whole operation rather than quietly throwing the work
away.

## What a project file holds

A project file stores **structure only**: which components, where, how they
are rotated, and every wire with its full route and junctions. It does not
store runtime state — button presses, signal values, the clock's phase — so
opening a project starts it cold, like opening a fresh circuit.

The current view (zoom and pan) isn't saved either; it's how you were
looking at the circuit, not part of it.

The format carries a version number, and older files are migrated forward on
open, so projects saved by earlier builds keep working:

| Version | Change |
|---|---|
| 1 | Wires were just groups of pins sharing a net, with no shape. |
| 2 | Wires became explicit, each with its own route and junctions. |
| 3 | A wire's *start* became a full endpoint too, so it can begin loose. |

A project file holds **every** circuit in the project, not just the one
open at the time — see [Circuits](#circuits) below. Saving, undo and redo
all work on the project as a whole.

## Circuits

A project holds one or more circuits, listed in the tree at the top left
with the project itself at the root. The circuit shown in bold is the one
on the canvas.

| Action | How |
|---|---|
| Open a circuit | Click its name. |
| Add one | The **+** button beside the *Circuits* heading. |
| Rename one | Double-click its name, or right-click → *Rename*. |
| Delete one | Right-click → *Delete*. |

Names have to be unique, and a project always keeps at least one circuit —
renaming onto a name already in use is refused rather than silently
altered, and *Delete* is greyed out on the last one.

Creating, renaming and deleting a circuit are ordinary edits, so `Ctrl+Z`
undoes them like anything else.

Two things worth knowing:

- **Circuits are independent.** You can't yet place one inside another as a
  component; that's the next step, and it's what will turn this list into a
  real hierarchy.
- **Only the open circuit runs.** Switching away rebuilds the circuit you
  arrive at from scratch, so it starts cold — a clock in a circuit you left
  is stopped, and begins again from its first tick when you come back. This
  is the same trade undo makes: runtime state was never part of the saved
  document.
