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

A project can hold several named circuits. Only one (`"main"`) is used
today — the format is ready for sub-circuit hierarchy, which isn't built
yet.
