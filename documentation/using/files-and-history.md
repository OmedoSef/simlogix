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

**File → Open Recent** lists the last eight projects you opened or saved
under a new name, most recent first, with the full path in the tooltip for
the days two of them share a file name. It goes through the same
unsaved-changes question as *Open* — a file already being named is no reason
to throw work away without asking. A project that no longer opens drops off
the list at the moment it fails, and *Clear the list* empties it.

The list is a preference rather than part of any project, so it survives
opening one — and *Reset settings* deliberately leaves it alone: it is a
history, and clearing it has its own entry where you would look for it.

The window title shows which file you're editing and an asterisk while it
has unsaved changes:

```
SimLogix — half-adder.slgx*
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

A project file holds **every** circuit in the project, not just the one
open at the time — see [Circuits](#circuits) below. Saving, undo and redo
all work on the project as a whole.

## The file itself

A project is a single `.slgx` file — one file to copy onto a USB stick or
sync to another machine, not a folder to keep together.

Inside, it's a zip archive laid out like this:

```
project.json              the format version, the library name, the folders
circuits/main.json        one file per circuit
circuits/alu/adder.json   folders are mirrored as real directories
circuits/scratch/         an empty folder still gets an entry
```

The folders you see in the editor are the directories you see in the
archive, so a project is browsable with an ordinary zip tool and looks like
what the editor shows.

You can open it with any zip tool and read the JSON inside; nothing is
compressed. Splitting the circuits into separate files isn't for speed —
the whole project is read at once either way. It's so the format has
somewhere to put things that aren't JSON later on, such as component
symbols you've drawn yourself.

Older files still open. The format is recognised from the file's contents
rather than its name, so a project saved by an earlier build — including
the previous `.simlogix` single-document format — opens without you having
to do anything, and is written back as `.slgx` next time you save.

| Version | Change |
|---|---|
| 1 | Wires were just groups of pins sharing a net, with no shape. |
| 2 | Wires became explicit, each with its own route and junctions. |
| 3 | A wire's *start* became a full endpoint too, so it can begin loose. |
| 4 | The document became a zip container, one file per circuit. |
| 5 | Projects carry a library name; components are saved qualified by it. |
| 6 | Circuits can be filed in folders. |
| 7 | Components can carry properties. |
| 8 | Wires can carry a colour. |
| 9 | A circuit can carry a symbol of its own. |
| 10 | A pin's name can be nudged clear of the line art. |
| 11 | A pin can carry an inversion bubble. |
| 12 | A component can declare how many bits its pins carry. |
| 13 | A constant carries the value it puts on its wire. |
| 14 | A splitter carries the width of each of its branches. |
| 15 | A component can name the base it shows its value in. |
| 16 | A component can be mirrored, beside the rotation it is placed at. |

## Circuits

A project holds one or more circuits, listed in the tree at the top left
with the project itself at the root. The circuit shown in bold is the one
on the canvas.

| Action | How |
|---|---|
| Place one in the circuit you're editing | Click its name. |
| Open a circuit | Double-click its name. |
| Add one | The **+** button beside the *Circuits* heading. It opens the name for editing straight away. |
| Rename one | `F2`, or right-click → *Rename*. |
| Delete one | Right-click → *Delete*. |

The tree is a palette of your own circuits: a single click arms one for
placing, exactly as a click in the component palette does. The circuit you
are *in* is the one exception — it can't contain itself, so clicking it does
nothing.

Circuits can be filed in folders, nested as deeply as you like:

| Action | How |
|---|---|
| Add a folder | The **🗀** button beside the heading, or right-click a folder → *New folder here*. It opens the name for editing straight away. |
| Fold one away | Click its ▸. It reopens on its own if something inside it needs to be seen. |
| File a circuit | Drag it onto a folder, or right-click it → *Move to*. |
| Rename a folder | Right-click → *Rename*. Only its own name changes; everything inside comes along. |
| Delete a folder | Right-click → *Delete folder*. **What's inside moves up** to the folder above rather than being deleted with it. |

A folder is **part of a circuit's address**, not just where it's filed: a
circuit is referred to as `library:folder/name`. So a name only has to be
unique **within its own folder** — `alu/adder` and `fpu/adder` are two
different circuits, and both are allowed.

The other side of that: renaming a circuit, moving it, or renaming a folder
changes the address of everything concerned. **Within one project this is
repaired for you** — every place that circuit has been used follows it, in
circuits you don't have open as well as the one you do. A reference from a
project that *imported* these circuits still has to be pointed at the new
address by hand.

Deleting is the one case that can't be repaired, because there is no new
address to point at. So **deleting a circuit that is still used somewhere
is refused**, and the message names the circuits using it. Delete those
instances first if you meant it.

One case is resolved rather than refused. Deleting a folder moves its
contents up, which can bring two circuits of the same name into one place —
the second is given a free name instead of the deletion being blocked.

Names have to be unique, and a project always keeps at least one circuit —
renaming onto a name already in use is refused rather than silently
altered, and *Delete* is greyed out on the last one.

Creating, renaming and deleting a circuit are ordinary edits, so `Ctrl+Z`
undoes them like anything else.

Two things worth knowing:

- **A circuit can be placed inside another.** Click it in the tree — or
  right-click → *Place in this circuit* — then click the canvas as you would
  for any component. What you get is a box with one pin per port of that circuit —
  so give your ports names, because those are the labels on the box.
- **Only the open circuit runs.** Switching away rebuilds the circuit you
  arrive at from scratch, so it starts cold — a clock in a circuit you left
  is stopped, and begins again from its first tick when you come back. This
  is the same trade undo makes: runtime state was never part of the saved
  document.

## The project's library name

The root of the circuit tree shows the project's **library name** — what
other projects will use to refer to its circuits once a circuit can be
imported into another project. Right-click → *Rename* to change it.

It starts out as the file name, the first time the project is saved or
opened, and stops following it from then on. That's deliberate: rename
`cpu.slgx` to `cpu-v2.slgx` and every reference another project makes to
these circuits would otherwise break. Two projects *can* end up with the
same library name — if that happens, rename one.

Components are saved qualified by library, so a saved circuit reads
`simlogix:And` for the built-in AND gate. A circuit you write yourself and
call `And` will therefore never be mistaken for it.
