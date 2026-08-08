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

## A known limitation

Connectivity is tracked as nets that get merged when wires are drawn, not
recomputed from the drawing after every edit. So if **two entirely separate
wires** connect the same two components and you cut one, the pin is
disconnected even though the other route plainly still exists on screen. The
common cases — a junction bridging a cut, deleting one of several wires on a
pin — are handled; this one is not. Redraw the connection if you hit it.
