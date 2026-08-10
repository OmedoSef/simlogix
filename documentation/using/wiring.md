# Wiring

A wire is a drawn object in its own right: two ends and a route between
them. Either end may be a **component pin**, a **junction** onto another
wire, or **loose** — attached to nothing yet. That's what lets you draw
wiring before deciding what it connects, and what lets a wire outlive the
component it was drawn to.

## Drawing a wire

Click a pin to start one — no need to pick the wire tool first. With the
**Wire** tool you can also start anywhere: on empty canvas, or on an
existing wire (which taps into it straight away).

Then:

- **Click** to drop a corner point.
- **Click a pin, or another wire**, to finish and connect there.
- **`Enter`** to finish where you last clicked, leaving that end loose.
- **`Escape` or right-click** to throw the wire away.

Finishing on a wire doesn't mean aiming at one of its dots: clicking
anywhere along it inserts a contact point right there. Landing on an
existing point reuses that point instead of stacking a second one on top.

## Reshaping a wire

Each corner point has a handle:

- **Drag** it to move it. It snaps to the grid when released.
- **Double-click a wire** to insert a new point at that spot.
- **Right-click a point** to remove it.
- **Right-click a segment** to cut it out — the wire splits into the piece
  before and the piece after, each ending loose where the cut was.

A wire with no points is simply a straight run between its two ends.

## Loose ends

A loose end is drawn as a **hollow circle**, so it reads as "attached to
nothing" against the filled dots of real corner points. Drag one onto:

- a **pin** — it connects there;
- **another wire** (anywhere along it, or on one of its points) — it taps
  into it;
- **another loose end** — the two wires are joined into one, the meeting
  point becoming an ordinary corner. This is the exact inverse of cutting a
  segment, so cut-then-rejoin gives the original wire back.

It also works the other way round: **drop a component so one of its pins
lands on a loose end** and it picks the wire up. That, plus the fact that
deleting a component leaves its wires behind, is what makes swapping a
component cheap.

## Junctions

A junction is one wire tapping another at one of its points, which is how
more than two pins end up on the same net. The tap **follows the point it
is attached to**: move that point and the tap comes along.

Consequences worth knowing:

- Deleting a wire doesn't delete what tapped it. Those taps are cut loose
  where they were, staying visible and editable rather than silently
  disappearing.
- Cutting a segment next to a junction keeps the connection: the tap is
  joined onto the piece it now meets end to end.
- Removing a point that carries a tap leaves that tap loose at the same
  spot, since it no longer has a point to hold on to.

## What counts as connected

What's connected is worked out from the drawing itself, after every edit:
the wires on screen are the record, and the simulator's nets are derived
from them. So the rule is simply what it looks like — pins that a chain of
wires links are on the same net, and pins nothing links are not.

That includes the awkward cases. Draw **two entirely separate wires**
between the same two components and cut one: they stay connected, because
the other one plainly still joins them. There's nothing to remember or
undo — the drawing is re-read, and it still says the same thing.

## Buses

A wire carrying more than one bit is **drawn thicker**. The thickness isn't
proportional to the width — what matters at a glance is one bit against
several, and a 32-bit wire as thick as a component would be a schematic
nobody can read.

To know *how many*, **hover it**: a bus says its width beside the pointer,
and, while signal state is showing, its value with it. Hovering already
lights the whole net, so the answer is the net's width, not the segment's.
A one-bit wire says nothing — that's the default of every wire in the
drawing, and repeating it everywhere would be noise rather than
information.

Selecting a wire says the same thing in the properties panel, which is
where you go when you also want to give it a colour.

A wire has no width of its own: it takes it from what it joins. Set the
width on the components — see [**Bits**](editor-basics.md#component-properties) in the
properties panel — and the wires between them follow. When two pins
disagree, the wider one wins and the other is flagged: see
[when two widths meet](simulation.md#when-two-widths-meet).

## Telling wires apart

Two problems make a crossing hard to read: every wire at the same logic
level is the same colour, and the wire you want continues on the far side
of the one it crosses.

**Hover a wire** and the whole net lights up — every wire connected to it,
not just the segment under the pointer. That is usually enough to follow a
conductor across a crossing, and it costs nothing when you're not doing it.

**Give a net a colour** for something you keep coming back to. Select a
wire and pick a colour in the properties panel on the right; *Reset* takes
it off again. The panel offers a dozen swatches and shows the code as
`#RRGGBB`, which you can also type or paste into — so the same colour can be
given to another net in another circuit without having to find it on a wheel
twice. The wheel is still there, behind the button, for anything the
swatches don't cover. The colour is drawn as a casing *around* the wire, so the
signal colour keeps the middle — what's changing during simulation stays
what you read first.

The colour belongs to the whole net, not to one wire: a net is a single
conductor, so colouring any of its wires colours all of them, and a wire
you draw onto a coloured net picks the colour up. Join two nets that have
*different* colours and both are kept — the result is visibly two-tone, so
you can see it happened and re-colour it, rather than one colour silently
winning.
