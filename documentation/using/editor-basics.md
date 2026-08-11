# Editor basics

The window is split into a menu bar, a toolbar above the canvas, the circuit
tree and component palette down the left side, the canvas itself, and a
status bar along the bottom.

The **status bar is worth watching**: it always describes what the current
selection or mode lets you do, so most of what follows can be discovered
without this page.

## Tools

The toolbar holds the interaction modes — what a click on the canvas *does*:

| Tool | What a click or drag on the canvas does |
|---|---|
| **Select** | Select and move things. The resting state. |
| **Selection rectangle** | Always sweeps a selection, whatever the setting below says. |
| **Wire** | Start drawing a wire, anywhere — see [Wiring](wiring.md). |
| **Pan** | Always moves the view. |

Choosing a component in the palette is a fifth, temporary mode: the next
click drops that component, then you fall back to Select.

The last two exist so both canvas gestures stay reachable however you set
**Settings → Left drag** — one of them is always a click away.

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

The palette is grouped into **Interface** (Input, Output, Bidirectional),
**Sources** (Button, Switch, Tri-state source, Constant, Clock, GND, PWR),
**Outputs** (LED, Probe), **Transistors** (NMOS, PMOS), **Gates** (AND, OR,
NAND, NOR, XOR, XNOR, NOT, Buffer, Tri-state buffer), **Memory** (SR latch)
and **Buses** (Splitter, the two transceivers). Each category folds away, and the
panel edge can be dragged to resize it.

The circuit tree above it is a palette too — of your own circuits. See
[Circuits](files-and-history.md#circuits).

## Selecting, moving, rotating, deleting

- **Select** — click a component or a wire. A selected item gets a blue
  outline; hovering shows a fainter one, so you can tell what a click is
  about to take.
- **Select several** — drag a rectangle across the canvas, or `Shift`-click
  to add and remove one at a time. Components and wires can be selected
  together.
- **Deselect** — click empty canvas, or press Escape, or right-click.
- **Move** — drag any selected item and the whole selection comes with it,
  keeping its shape. It follows the pointer freely and snaps to the grid
  when you let go, which keeps dragging smooth instead of jerky.
- **Rotate** — `R` turns every selected component a quarter turn clockwise,
  pins included. Each turns on **its own** centre rather than the group's:
  pins have to land on the grid, and turning the group as one body would put
  them between dots.
- **Delete** — `Delete` or `Backspace` removes everything selected.
- **Copy and paste** — `Ctrl+C` and `Ctrl+V`, or the Edit menu. The copy
  lands one grid step down and right, and *it* becomes the selection, so a
  second paste or a drag acts on the copy rather than the original.

A wire is copied only when **both** its ends are inside the selection. One
whose far end is a pin you didn't take has nowhere to attach, so it is left
out rather than pasted dangling — select the components *and* the wires
between them.

The copy travels on the system clipboard, so it can be pasted into another
SimLogix window. Pasting anything else — a URL, some text — does nothing.

Deleting a component **keeps the wires that were attached to it**, cut loose
where the pin used to be. Swapping one gate for another is then a matter of
dragging those loose ends onto the replacement rather than redrawing them.

## Moving around the canvas

- **Zoom** — mouse wheel, anchored on the pointer: whatever is under the
  cursor stays under it.
- **Pan** — drag with the **middle** button, anywhere, whatever the tool.
  Or use the Pan tool, or set the left drag to pan (below).

Dragging a component or a wire point moves that instead, since those claim
the gesture first — the background is only reached where there is nothing.

The view is not part of the circuit: it isn't saved in the project file, and
undo won't move you somewhere else.

## Settings

**Settings → Left drag** decides what dragging the empty canvas with the
left button does: sweep a **selection rectangle** (the default) or **move
the view**. Pick whichever you do more often; the other stays one toolbar
click away.

**Settings → Values shown in** picks the base every value on screen is
written in — binary, decimal or hexadecimal. *Automatic* is the default and
means `0`/`1` on a plain wire and hexadecimal on a bus, which is how a bit
pattern is read. Any single component can be told to use another base with
its own **Shown in** property; everything else keeps following this, so
changing it here still moves them.

**Settings → Theme** follows the operating system by default, and can be
forced to light or dark. **Settings → Language** offers English, French and
Italian, defaulting to the system locale. Neither affects what a project
file means — the language you edit in is invisible to the saved circuit.

These are remembered between runs, along with the window size and the panel
widths. **Settings → Reset to defaults** puts back the three above — it
leaves the window and panels alone, since a button labelled "settings"
rearranging your layout would be a surprise.

Resetting the language doesn't force English: it clears your *choice*, so
the editor goes back to following the system locale and keeps following it.

## Keyboard reference

The same list, plus the mouse gestures, is in the app under **? → Shortcuts
and gestures**.

| Key | Action |
|---|---|
| `R` | Rotate the selected components |
| `Delete` / `Backspace` | Delete the selection |
| `Ctrl+C` / `Ctrl+V` | Copy / paste the selection |
| `Shift`+click | Add to or remove from the selection |
| `Escape` | Cancel the wire being drawn, otherwise clear the selection |
| `Enter` | Finish the wire being drawn, leaving its end loose |
| `Space` | Run / pause the simulation |
| `F10` | Step one tick |
| `Shift+F10` | Skip to the next event |
| `Ctrl+F10` | Step one clock edge |
| `C` | Show / hide the signal state on wires |
| `F12` | Show the engine state |
| `F2` | Rename the circuit you are in |
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
| **Type** | transistors, transceivers | Switches between the pair — the symbol follows. |
| **Pressed at rest** | Button | The button rests pressed, so clicking it *releases* it. |
| **Closed at rest** | Switch | Where the switch sits when the project opens. |
| **Three-state** | Input, Bidirectional | Whether clicking can also leave the port undriven. |
| **Resting value** | Input, Bidirectional, Tri-state source | Where it sits when the project opens. |
| **Value** (panel) | Input, Bidirectional, Tri-state source | What it is driving *now* — runtime, never saved. Clicking the component steps it by one. |
| **Colour** | LED | What it glows when lit. *Reset* puts it back to red. |
| **Bits** | the three ports, the plain gates, Constant, Tri-state buffer, the transceivers | How many bits its *data* pins carry. A pin that disagrees with its wire is [ringed in red](simulation.md#when-two-widths-meet). |
| **Value** | Constant | What it puts on its wire. Typed in decimal, or `0x…` / `0b…`. |
| **Shown in** | Probe, the three ports, Tri-state source, Constant | Which base its value is written in. *Follow the setting* is the default. |
| **Branches** | Splitter | How many bits each branch takes, from bit 0 upward. |

A component that shows a value **keeps its body upright when you rotate
it**: only its pin moves round to another edge. Text is never drawn
sideways, so turning the body would leave a wide value lying across a tall
narrow box. It is the rule the plain gates have always followed — the shape
stays put and which edge carries the pins is what turns.

A component that shows a value **grows to hold it**: a 32-bit port is wider
than a one-bit one, and wider again in binary than in hexadecimal. It is
sized from the width and the base, never from the value on it, so it keeps
its size while the simulation runs rather than shifting its own pins about.
A one-bit component is exactly the box it has always been.

A **constant** is the one component that is nothing but its value: it puts
a fixed number on its wire and the symbol draws that number, shown in hex
past four bits and in decimal below. It is not clickable — the value is a
setting, so changing it is an ordinary edit that `Ctrl+Z` undoes, unlike a
port's value which is runtime state. On a one-bit wire it is simply 0 or 1.

A gate told it is four bits wide is four gates side by side: it computes on
every bit on its own, and all of its pins are that width.

Not every component's pins are alike, and the setting only widens the ones
that carry data: a tri-state buffer's enable and a transceiver's direction
stay one bit whatever passes through. A **sub-circuit instance** has no such
setting at all — each of its pins is as wide as the port it stands for, so
you set the width on the ports inside and the instance follows.

The SR latch is deliberately left out: a wide one would be a register, and
what `S` and `R` should mean for it is a design question rather than a
width.

A **button** and a **switch** now say the same thing: where it *rests*. A
press springs back on its own; a switch is put back there when the project
opens. Where a switch is right now — like what a port is driving right now —
is in the **Value** panel below the properties, and flipping one there or on
the canvas takes no undo step and doesn't mark the project modified.

That's the line the project file draws, and it is worth stating plainly:
**what you set is kept; what the simulation produced is not.** Signal levels,
a clock's phase, a button's press and where a switch has been flipped to are
produced. A resting position and a port's resting value are set.

Setting a property is an ordinary edit, so `Ctrl+Z` undoes it. Typing a name
counts as one step from the moment you click into the field; the colour
picker leaves a step per change while you drag through it.

## The circuit's interface

The **Interface** category holds the three ports that make a circuit usable
inside another one: `Input`, `Output` and `Bidirectional`. Each is one pin
on the circuit's boundary, and giving it a **Name** is what will label it on
the parent's symbol.

They are useful before anything contains the circuit, which is deliberate —
you test a circuit long before you reuse it:

- An **input** is a latching switch you click. Unlike a button it stays
  where you put it, because it stands for what a parent will drive.
- An **output** just reads its net.
- A **bidirectional** port does both: click it to drive, or leave it
  undriven and watch what the circuit puts there.

All three show what their net carries, using the probe's letters
(`1`/`0`/`?`/`E`/`Z`). Only the readout follows the signal colour — the body
and the arrow say which way the value crosses the boundary, and that doesn't
change as the circuit runs.

**Three-state** adds an undriven position to the click cycle, which then
goes undriven → high → low. What "undriven" *means* differs by port, and the
difference matters:

- an **input** goes to unknown — nothing outside is supplying it;
- a **bidirectional** port goes to high-impedance — it lets go, so the
  circuit inside can drive the net. Unknown would count as a driver and put
  the net into conflict instead of stepping aside.

An output has no such setting: it never drives, and it already reads all
five states including the absence of one.

## Memory

The palette's **Memory** category holds the `SR latch`: `S` sets `Q` high,
`R` clears it, and with neither asserted it holds what it was last told —
the simplest thing in the editor that remembers.

Both outputs are lettered `Q`; the one carrying an inversion **bubble** is
the complement.

Two behaviours worth knowing, because both are deliberate:

- Asserting `S` and `R` together has no defined answer, so both outputs go
  to the error colour rather than picking one. The same goes for an input
  that nothing is driving: the latch reports that it no longer knows,
  instead of assuming the undriven input is low.
- A freshly placed latch reads as unknown until you set or reset it. Real
  hardware comes up either way, and inventing a power-on value here would
  hide bugs that depend on one.

You can also build a latch by hand from two cross-coupled NAND gates — that
works, and is what the simulator's own tests exercise. Its inputs are
active *low*, unlike the component's.

## Shared buses

The `Tri-state buffer` in the Gates category is the one component that can
*stop* driving. While its enable input is high it passes its data input
through like an ordinary buffer; while the enable is low it lets go of the
net entirely, so something else can drive it.

That's what lets several outputs share one wire — wire two of them to the
same net, enable one at a time, and the net carries whichever is speaking.

The states you'll see on a shared net:

| Situation | The net reads |
|---|---|
| One buffer enabled | whatever it's passing |
| None enabled | unknown — a floating wire, not a low one |
| Two enabled, disagreeing | error — a short between two drivers |
| Two enabled, agreeing | that value; there's nothing to report |

An enable that nothing drives isn't the same as an enable held low: the
buffer reports that it doesn't know whether it should be driving, rather
than assuming it shouldn't.

### Driving a shared net by hand

Testing any of this needs a source that can *stop* driving, and a switch
cannot: open, it still puts a low on the wire. That is what the **Tri-state
source** is for. Clicking it steps through three positions — driving high,
driving low, and letting go — drawn as a change-over lever thrown to the
supply, to ground, or to neither.

Letting go is a real release, not a low: the net is then carried by whatever
else is on it, which is exactly what you want to watch when you are testing
a buffer or a transceiver.

It shows the net's own value in the same one-letter readout a probe uses,
and that is deliberately a different fact from the lever: with the lever
centred, the letter is telling you what *something else* decided.

A tri-state source has no three-state setting of its own, unlike the ports —
three positions is the whole of what it is, and a two-position one is just a
switch. It does have a **Resting value**, which is where it sits when the
project is opened.

### Both ways at once

The Buses category has a transceiver, which joins two buses and passes
traffic one way at a time. `DIR` picks the direction — high sends the left
side (`A`) to the right (`B`), low sends it back — and the enable switches
the whole thing off, letting go of both buses at once.

It comes in two flavours, differing only in which way round that enable is:

| Entry | Enable | On when |
|---|---|---|
| **(EN)** | `EN`, plain | high |
| **(OE)** | `OE`, with a bubble on the pin | low |

The bubble is what tells them apart on the canvas — it's the standard mark
for an inverted input, and `(OE)` matches the polarity of the real 74x245.
Pick whichever suits the logic you already have; you can also switch an
existing one from **Type** in the properties panel.

The side that is *listening* drives nothing, so it never fights the bus it
is reading. Flipping `DIR` while both buses are being driven produces one
tick of crossover, the same turnaround a real transceiver has; bus protocols
leave a spare cycle there for exactly that reason.

## Changing a component's type

Some components come in pairs that differ by one setting — a transistor's
channel, a transceiver's enable polarity. Select one and the properties
panel offers **Type**; switching it redraws the symbol and keeps everything
attached, wires and routes included.

It's an ordinary edit, so `Ctrl+Z` puts it back.

## Using a circuit inside another

Give a circuit some ports (see above), then right-click it in the tree and
choose **Place in this circuit**. A preview follows the pointer, showing the
box you're about to drop: inputs down the left, outputs down the right, each
labelled with its port's name.

The pins appear in the order the ports sit **in the sub-circuit**, top to
bottom. Move a port up in there and its pin moves up on every box out here —
that's the only ordering there is, and it's one you can see.

**Sub-circuits nest as deep as you like.** A circuit built from circuits
that are themselves built from circuits works the same at every level;
there is no limit and no cost beyond the depth itself.

An instance is not a snapshot: editing the circuit it refers to changes what
its instances do, and two instances of the same circuit are two genuinely
independent copies. A circuit cannot contain itself, directly or through
another one; the menu entry is greyed out on the circuit you're editing, and
a longer loop is refused with a message.

The sub-circuit's own switches, buttons and ports aren't yours to click from
out here — from the parent, the box is driven entirely through its pins.

The generated box is a starting point, not the only option: a circuit can
carry a symbol you drew instead. See
[Drawing a circuit's symbol](symbols.md).
