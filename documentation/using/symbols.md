# Drawing a circuit's symbol

A circuit placed inside another one shows as a box: a rectangle with a pin
per port, its name written above. That box is generated, and generated is
the wrong word for something a reader has to recognise at a glance. So a
circuit can carry a **symbol of its own** instead.

Nothing forces you to draw one. Until you do, the generated box is what
every instance shows, exactly as before.

## The two views of a circuit

The toolbar above the canvas switches between them:

| View | What you're looking at |
|---|---|
| **Schematic** | What the circuit is made of — components and wires. |
| **Appearance** | What it looks like when used inside another circuit. |

Each view keeps its own camera. A schematic sits wherever you drew it and a
symbol always sits on the origin, so switching doesn't drop you somewhere
unrelated.

The first time you move anything in the appearance view, the generated box
becomes this circuit's own symbol — with the box itself as the starting
point. Nothing is lost and nothing is invented: it's the box that was on
screen a moment before. **Reset symbol** in the toolbar gives it back if a
try goes wrong.

## Pins are placed, not drawn

The circuit's **ports** decide which pins exist. A symbol says only *where*
each one comes out; it can't invent a pin or leave one off. Add a port and a
pin appears, remove one and its pin goes.

Select a pin and the properties panel shows:

| Setting | What it does |
|---|---|
| **Points** | Which way the pin faces. The lead is drawn the other way, and the port's name is written at its inner end. |
| **Position** | Where it sits, in whole grid steps. |
| **Lead length** | How far the lead runs back towards the body. Zero draws none. |
| **Show the port's name** | Whether the name is written beside it. |
| **Nudge the name** | Moves that name, in quarter-grid steps. |

**Nudge the name** exists because the automatic place — a fixed step in
from the lead — is right until your line art is in the way. A name against
a sloped edge, on the side of a multiplexer say, comes out unreadable, and
nothing about the pin can work out where you left room. The nudge is in the
symbol's own coordinates, the same ones every other field is typed in, so
it does not change meaning when you move the pin to another edge. *Reset*
puts it back.

The panel names which port a pin belongs to. On a symbol with four
identical-looking pins that's the only thing telling them apart — which is
what a port's name is for.

**The direction is yours to set, not guessed from where the pin sits.** An
earlier version took it from the nearest edge on every drop; that reads well
on a rectangle and fights you around a curve, or when a pin deliberately
points across the body.

Pins snap to **whole** grid steps, because a pin is what a wire attaches to
and it has to land on a dot.

## Drawing

The tools sit in the toolbar, to the right of the view switch.

| Tool | How |
|---|---|
| **Select** | Click to pick, drag to move, `Delete` to remove. |
| **Line** | Click by click, like a wire. Double-click or `Enter` finishes it, `Escape` drops it. |
| **Rectangle** | Drag one corner to the other. |
| **Circle** | Drag from the centre out to the edge. |
| **Arc** | Click each end, then move to bulge it and click again. |
| **Text** | Click to drop a label, then type it in the panel. |
| **Pan the view** | Drag to move the view. The middle button always pans, whatever the tool. |

Shapes snap to a **quarter** of a grid step. A whole step leaves four cells
across a component box, which isn't enough to shape anything; a quarter is
fine enough to draw a gate and coarse enough that two lines meant to meet
actually do.

A rectangle is stored as a closed four-point line, so you can drag any one
of its corners afterwards and get a shape that is no longer a rectangle.
That's deliberate — only the *gesture* is rectangular.

An arc is stored as the three points it passes through: the two ends and one
point on the curve. The middle one is what decides which way round it goes,
so dragging it flips the bulge from one side to the other. Three points in a
line describe no circle, and the arc simply becomes the straight run through
them.

## Selecting and moving

| Action | How |
|---|---|
| Pick one thing | Click it. |
| Add or remove one | `Shift`-click. |
| Pick several | Drag on empty canvas to sweep a rectangle. |
| Move what's picked | Drag it. |
| Nudge it | Arrow keys. |
| Delete shapes | `Delete` or `Backspace`. |
| Copy and paste shapes | `Ctrl+C`, `Ctrl+V`. |

The sweep catches **anything it touches**, not only what it swallows whole —
otherwise a long line drawn across the symbol would be impossible to catch.

Arrow keys move by one step of whatever the thing snaps to: a quarter of a
grid step for a shape, a whole one for a pin. A nudge lands exactly where a
drag would have, which is the point of having them.

Copy and paste carry **shapes only**. A pin belongs to a port, so a copy of
one would be the pin of nothing.

## Typing exact values

Every shape shows the points it's made of in the properties panel, plus
whatever else it has — a circle's radius, a line's open/closed, an arc's
three points, a label's size and alignment.

**Typed values don't snap.** Dragging is how you sketch a shape; typing is
how you make it exact, and a curve sometimes has to sit off the grid to look
right.

## The circuit's name

By default the circuit's name is written above the symbol. With nothing
selected, the properties panel offers **Show the circuit's name** to turn it
off.

It's on for the generated box, where the rectangle says nothing on its own
and the name is all that identifies it. A symbol you drew often says what it
is by its shape, or carries a label you put where you wanted it — and then a
name floating above is in the way.

## How big the symbol is

The box you hover and click — and the outline you see around a selected
instance — is the extent of what you actually drew, shapes and pins
together. Text is left out of it, because its drawn size changes with the
zoom and the clickable area shouldn't.

There is a floor: a symbol smaller than one component box is treated as that
size, so two short lines are still something you can hit. The floor is a
minimum *size*, applied around the drawing where it sits — a symbol drawn
entirely to one side of its origin claims no space on the other.

## What isn't stored

**Colour.** Symbols take theirs from the theme, so one saved in a light grey
would be invisible on a white background. You draw shapes, not colours.

**Rotation.** A symbol is drawn once, facing one way; rotating an instance
turns the whole drawing, pins included, exactly as it does for a built-in
component.

## Two things to know

- **Built-in components can't be redrawn.** Gates, transistors and the rest
  keep their hand-coded symbols; this is for circuits you write.
- **Editing a symbol updates every instance of it**, the same way editing a
  circuit's contents does.
