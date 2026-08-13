//! Wires: cutting, joining, reshaping, and the net rebuild that follows.
//!
//! Split out of `app.rs` as one subject. Connectivity in SimLogix is
//! *derived from the drawing* after every edit, so these two belong
//! together: everything here changes what is drawn, and `rebuild_nets` is
//! what turns the drawing back into nets.
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use simlogix_core::{ComponentId, Member, NetGroup};

use crate::placed_component::PinHandle;

use super::{SimLogixApp, WireEndpoint};

/// Where a wire's two ends and its points actually are, this frame.
pub(super) struct ResolvedRoute {
    pub from: egui::Pos2,
    pub to: egui::Pos2,
    pub waypoints: Vec<egui::Pos2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Node {
    Pin(ComponentId, usize),
    Wire(u64),
    /// One of an instance's internal nets, which has no pin of this
    /// drawing's own to stand for it.
    Inner(ComponentId, usize),
}

// **Weighted**: each node knows not only which group it is in but
// *where in it* — which bit of the group its own bit zero is. A
// plain wire joins two things at the same bit, and only a splitter
// ever asks for anything else, which is what lets "bit 3 of this
// net is bit 0 of that one" be said without a component between
// them to relay it.
//
// Offsets are signed while the sets are being built, because which
// of two nodes becomes the root is arbitrary and the other one may
// well sit below it. They are normalised to start at zero once the
// groups are known.
type Parents = HashMap<Node, (Node, isize)>;
/// The group `node` belongs to, and where its bit zero sits in it.
fn find(parent: &mut Parents, node: Node) -> (Node, isize) {
    let mut root = node;
    let mut from_node = 0;
    while let Some(&(next, delta)) = parent.get(&root) {
        if next == root {
            break;
        }
        from_node += delta;
        root = next;
    }
    // Path compression, so a long chain of taps stays cheap — and
    // now carrying the accumulated offset with it, or the shortcut
    // would forget where the node sat.
    let mut walk = node;
    let mut walked = 0;
    while let Some(&(next, delta)) = parent.get(&walk) {
        if next == root {
            break;
        }
        parent.insert(walk, (root, from_node - walked));
        walked += delta;
        walk = next;
    }
    parent.entry(node).or_insert((root, from_node));
    (root, from_node)
}

/// Says that `a`'s bit zero is `b`'s bit `delta`.
///
/// Answers false when the two are already in one group and placed
/// differently — a drawing that says two contradictory things about
/// the same conductor, which only a loop of splitters can produce.
fn union(parent: &mut Parents, a: Node, b: Node, delta: isize) -> bool {
    let (root_a, from_a) = find(parent, a);
    let (root_b, from_b) = find(parent, b);
    if root_a == root_b {
        return from_a - from_b == delta;
    }
    // Hanging `root_a` under `root_b`: whatever puts `a` exactly
    // `delta` above `b` once both are read through their roots.
    parent.insert(root_a, (root_b, delta - from_a + from_b));
    true
}

impl SimLogixApp {
    /// Every wire's endpoints and points, worked out from where the pins
    /// have landed this frame.
    ///
    /// A junction depends on its host being resolved first, and a wire can
    /// be re-attached to *any* other one, so this repeats until a pass
    /// resolves nothing new rather than assuming creation order. Wires left
    /// unresolved are the genuinely unresolvable ones — a deleted component,
    /// or a tap cycle — and are simply not drawn.
    ///
    /// Out here rather than in the frame loop because it touches none of
    /// what that loop shares: no pointer, no `Ui`, no click to consume. It
    /// reads the drawing and the pin positions and answers with data.
    pub(super) fn resolve_routes(&self, pins: &[PinHandle]) -> HashMap<u64, ResolvedRoute> {
        let pin_position = |component: ComponentId, pin_index: usize| -> Option<egui::Pos2> {
            pins.iter()
                .find(|handle| handle.component == component && handle.pin_index == pin_index)
                .map(|handle| handle.position)
        };
        // Both ends resolve the same way. A junction may not be resolvable
        // *yet*, its host coming later in the list; a later pass picks it up.
        let place = |endpoint: WireEndpoint, resolved: &HashMap<u64, ResolvedRoute>| match endpoint
        {
            WireEndpoint::Pin(component, pin_index) => pin_position(component, pin_index),
            WireEndpoint::Junction {
                wire: host,
                waypoint,
            } => resolved
                .get(&host)
                .and_then(|route| route.waypoints.get(waypoint))
                .copied(),
            WireEndpoint::Free(pos) => Some(pos),
        };

        let mut resolved: HashMap<u64, ResolvedRoute> = HashMap::new();
        let mut progressed = true;
        while progressed {
            progressed = false;
            for wire in &self.wires {
                if resolved.contains_key(&wire.id) {
                    continue;
                }
                let (Some(from), Some(to)) =
                    (place(wire.from, &resolved), place(wire.to, &resolved))
                else {
                    continue;
                };
                // A wire is exactly the points it was given: no waypoints
                // means a straight run end to end.
                //
                // There used to be an implicit mid-point bend here, back when
                // routing wasn't under the user's control. It bred phantom
                // points that only became real once dragged — and for a level
                // wire it produced *two* of them at the same spot, which is
                // what left a stray point on top of an end after a cut.
                resolved.insert(
                    wire.id,
                    ResolvedRoute {
                        from,
                        to,
                        waypoints: wire.waypoints.clone(),
                    },
                );
                progressed = true;
            }
        }
        resolved
    }

    /// Removes `roots`, disconnecting each one's own pin.
    ///
    /// Wires tapped onto a removed one are **kept**, with their junction
    /// frozen in place (`WireEndpoint::Free`) — `resolved` says where each
    /// wire's waypoints currently are, so the loose end lands exactly where
    /// the contact point was. Deleting these instead (which is what an
    /// earlier version did, to stop an orphaned tap resolving to nothing and
    /// silently vanishing) meant deleting one gate could wipe out wiring
    /// that had nothing to do with it.
    pub(super) fn remove_wires(
        &mut self,
        roots: Vec<u64>,
        resolved: &HashMap<u64, Vec<egui::Pos2>>,
    ) {
        for &id in &roots {
            let host_waypoints = resolved.get(&id);
            for wire in &mut self.wires {
                let WireEndpoint::Junction {
                    wire: host,
                    waypoint,
                } = wire.to
                else {
                    continue;
                };
                if host != id {
                    continue;
                }
                // Fall back to this wire's own last corner if the host's
                // geometry isn't known (it always is for a wire that was
                // just on screen), so a tap can never be left unresolvable.
                let at = host_waypoints
                    .and_then(|points| points.get(waypoint).copied())
                    .or_else(|| wire.waypoints.last().copied());
                if let Some(at) = at {
                    wire.to = WireEndpoint::Free(at);
                }
            }
        }

        // Nothing to disconnect by hand: dropping the wire from the drawing
        // is the edit, and the nets are recomputed from what's left.
        self.wires.retain(|wire| !roots.contains(&wire.id));
    }

    /// Shifts every junction tapped onto `host` at or past `from` by
    /// `delta`, so taps keep pointing at the same physical point when that
    /// wire's waypoint list grows or shrinks ahead of them.
    pub(super) fn shift_junctions(&mut self, host: u64, from: usize, delta: isize) {
        for wire in &mut self.wires {
            if let WireEndpoint::Junction { wire: w, waypoint } = &mut wire.to {
                if *w == host && *waypoint >= from {
                    *waypoint = waypoint.saturating_add_signed(delta);
                }
            }
        }
    }

    /// Collapses waypoints of a wire that have ended up on the same spot.
    ///
    /// Two points at one position make a zero-length segment and, worse,
    /// two overlapping drag handles competing for the same click — you
    /// could never separate them again. Junctions follow onto the survivor:
    /// `shift_junctions` maps a tap on the removed point down onto the one
    /// that stays, which is at the very same place, so nothing appears to
    /// move.
    pub(super) fn dedupe_waypoints(&mut self, wire_id: u64) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let mut at = 1;
        while at < self.wires[index].waypoints.len() {
            if self.wires[index].waypoints[at] == self.wires[index].waypoints[at - 1] {
                self.wires[index].waypoints.remove(at);
                self.shift_junctions(wire_id, at, -1);
            } else {
                at += 1;
            }
        }
    }

    /// Drops waypoint `index` from `wire_id`.
    ///
    /// Anything tapped onto exactly the point being removed is left where
    /// that point was, since it no longer has one to hold on to.
    pub(super) fn remove_waypoint(&mut self, wire_id: u64, index: usize, resolved: &[egui::Pos2]) {
        let Some(position) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        if index >= self.wires[position].waypoints.len() {
            return;
        }
        self.record_edit();
        self.wires[position].waypoints.remove(index);

        // Anything tapped onto the point just removed is left where that
        // point was, rather than deleted along with it.
        let at = resolved.get(index).copied();
        for wire in &mut self.wires {
            if let WireEndpoint::Junction {
                wire: host,
                waypoint,
            } = wire.to
            {
                if host == wire_id && waypoint == index {
                    if let Some(at) = at {
                        wire.to = WireEndpoint::Free(at);
                    }
                }
            }
        }
        self.shift_junctions(wire_id, index, -1);
    }

    /// Joins a wire's loose end to any other loose end sitting at the very
    /// same place.
    ///
    /// A cut can leave two ends stacked: when one wire tapped the cut point
    /// from *both* sides, joining it onto the first piece also carries its
    /// other end over, which then lands on the second piece's end. That
    /// second one has nothing left to join against by name — only by
    /// position, which is what this does.
    pub(super) fn join_touching_loose_end(&mut self, wire_id: u64, is_from: bool) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let end = if is_from {
            self.wires[index].from
        } else {
            self.wires[index].to
        };
        let WireEndpoint::Free(at) = end else {
            return;
        };

        let touching = self.wires.iter().find_map(|other| {
            if other.id == wire_id {
                return None;
            }
            [(true, other.from), (false, other.to)]
                .into_iter()
                .find_map(|(other_is_from, other_end)| match other_end {
                    // Everything here is grid-snapped, so this only forgives
                    // floating-point dust, not genuinely separate points.
                    WireEndpoint::Free(pos) if pos.distance(at) < 0.5 => {
                        Some((other.id, other_is_from))
                    }
                    _ => None,
                })
        });

        if let Some((other_id, other_is_from)) = touching {
            self.join_wires(wire_id, is_from, other_id, other_is_from, at);
            self.dedupe_waypoints(wire_id);
        }
    }

    /// Cuts one segment out of a wire, leaving the piece before it and the
    /// piece after as separate wires, each ending loose where the cut was.
    /// Cutting at either extreme leaves a single piece; cutting the only
    /// segment of an unrouted wire removes it entirely.
    ///
    /// `path` is the wire as currently drawn — `from`, then its waypoints,
    /// then `to` — so the cut segment runs from `path[cut]` to
    /// `path[cut + 1]`. Passing the drawn path (rather than reading the
    /// stored waypoints) is what lets a wire still on its automatic route
    /// keep that shape in the pieces.
    ///
    /// The pieces are no longer joined, so whatever the wire connected is
    /// disconnected in the circuit too, exactly as if it had been deleted.
    pub(super) fn split_wire(&mut self, wire_id: u64, cut: usize, path: &[egui::Pos2]) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        if path.len() < 2 || cut + 1 >= path.len() {
            return;
        }
        self.record_edit();

        let to = self.wires[index].to;
        let waypoints = &path[1..path.len() - 1];
        let last = waypoints.len();

        // A piece needs two points to exist: nothing survives before a cut
        // at the very start, or after one at the very end.
        let head = (cut >= 1).then(|| waypoints[..cut - 1].to_vec());
        let tail = (cut < last).then(|| waypoints[cut + 1..].to_vec());

        // The original wire becomes whichever piece survives — keeping its
        // id means taps and the current selection still refer to something.
        let (head_id, tail_id) = match (&head, &tail) {
            (Some(head_waypoints), _) => {
                self.wires[index].to = WireEndpoint::Free(path[cut]);
                self.wires[index].waypoints = head_waypoints.clone();
                let tail_id = tail.as_ref().map(|tail_waypoints| {
                    self.add_wire(
                        WireEndpoint::Free(path[cut + 1]),
                        to,
                        tail_waypoints.clone(),
                    )
                });
                (Some(wire_id), tail_id)
            }
            (None, Some(tail_waypoints)) => {
                self.wires[index].from = WireEndpoint::Free(path[cut + 1]);
                self.wires[index].waypoints = tail_waypoints.clone();
                (None, Some(wire_id))
            }
            (None, None) => {
                self.remove_wires(vec![wire_id], &HashMap::new());
                (None, None)
            }
        };

        // Re-home every tap: points kept by a piece move to it, while the
        // two bordering the cut are now loose ends, which can't be tapped —
        // anything attached there is cut loose in turn.
        // The two points bordering the cut stop being waypoints — each
        // becomes a piece's loose end — so taps on them have no waypoint
        // left to name. Rather than cutting those wires adrift, they're
        // noted here and joined onto the piece afterwards: they meet it end
        // to end at exactly that point, which is the one case `join_wires`
        // exists for. The connection is what matters; that two wires become
        // one is the same outcome as dropping their ends together by hand.
        let mut border_taps: Vec<(u64, bool, bool)> = Vec::new();
        for other in &mut self.wires {
            let tap_id = other.id;
            for (tap_is_from, end) in [(true, &mut other.from), (false, &mut other.to)] {
                let WireEndpoint::Junction {
                    wire: host,
                    waypoint,
                } = *end
                else {
                    continue;
                };
                if host != wire_id {
                    continue;
                }
                *end = match waypoint {
                    // Kept by the head, at the same index.
                    j if j + 1 < cut => match head_id {
                        Some(id) => WireEndpoint::Junction {
                            wire: id,
                            waypoint: j,
                        },
                        None => WireEndpoint::Free(path[j + 1]),
                    },
                    // Kept by the tail, shifted down past the cut.
                    j if j > cut => match tail_id {
                        Some(id) => WireEndpoint::Junction {
                            wire: id,
                            waypoint: j - (cut + 1),
                        },
                        None => WireEndpoint::Free(path[j + 1]),
                    },
                    // On the cut's own border: `j + 1 == cut` is the head's
                    // new end, otherwise it's the tail's new start.
                    j => {
                        border_taps.push((tap_id, tap_is_from, j + 1 == cut));
                        WireEndpoint::Free(path[j + 1])
                    }
                };
            }
        }

        for (tap_id, tap_is_from, on_head) in border_taps {
            let (Some(piece), at) = (
                if on_head { head_id } else { tail_id },
                if on_head { path[cut] } else { path[cut + 1] },
            ) else {
                continue;
            };
            // The head meets the cut at its `to`, the tail at its `from`.
            let piece_is_from = !on_head;
            // A piece has one end to give: if two wires tapped the same
            // point, the first takes it and the rest stay loose.
            let free = self.wires.iter().find(|w| w.id == piece).is_some_and(|w| {
                let end = if piece_is_from { w.from } else { w.to };
                matches!(end, WireEndpoint::Free(_))
            });
            if free {
                self.join_wires(piece, piece_is_from, tap_id, tap_is_from, at);
                self.dedupe_waypoints(piece);
            }
        }

        // Those joins can have brought a wire's far end to rest on the other
        // piece's end; nothing names it, so it's matched by position.
        if let Some(head) = head_id {
            self.join_touching_loose_end(head, false);
        }
        if let Some(tail) = tail_id {
            self.join_touching_loose_end(tail, true);
        }
    }

    /// Flips a wire end for end. Only its own geometry changes — what it
    /// connects is the same — but taps on it are mirrored so they stay on
    /// the point they were on.
    ///
    /// Used to line two wires up before joining them: a join needs one
    /// wire's loose end to be its `to` and the other's to be its `from`,
    /// and which is which depends on how each was drawn.
    pub(super) fn reverse_wire(&mut self, wire_id: u64) {
        let Some(index) = self.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };
        let count = self.wires[index].waypoints.len();
        let wire = &mut self.wires[index];
        std::mem::swap(&mut wire.from, &mut wire.to);
        wire.waypoints.reverse();

        for other in &mut self.wires {
            for end in [&mut other.from, &mut other.to] {
                if let WireEndpoint::Junction { wire, waypoint } = end {
                    if *wire == wire_id && *waypoint < count {
                        *waypoint = count - 1 - *waypoint;
                    }
                }
            }
        }
    }

    /// Joins two wires meeting at a loose end into a single one — the
    /// inverse of [`Self::split_wire`], so cutting a wire and dropping the
    /// pieces back together gives the wire back.
    ///
    /// `keep` survives and absorbs `absorb`; the point they meet at becomes
    /// an ordinary waypoint. Both are turned to face the same way first,
    /// and taps on `absorb` follow onto `keep` at their shifted position.
    pub(super) fn join_wires(
        &mut self,
        keep: u64,
        keep_end_is_from: bool,
        absorb: u64,
        absorb_end_is_from: bool,
        at: egui::Pos2,
    ) {
        if keep == absorb {
            return;
        }
        // `keep` must end at the meeting point and `absorb` must start there.
        if keep_end_is_from {
            self.reverse_wire(keep);
        }
        if !absorb_end_is_from {
            self.reverse_wire(absorb);
        }

        let (Some(keep_index), Some(absorb_index)) = (
            self.wires.iter().position(|w| w.id == keep),
            self.wires.iter().position(|w| w.id == absorb),
        ) else {
            return;
        };

        let absorbed = self.wires.remove(absorb_index);
        let keep_index = self
            .wires
            .iter()
            .position(|w| w.id == keep)
            .unwrap_or(keep_index);
        let offset = self.wires[keep_index].waypoints.len() + 1;

        self.wires[keep_index].waypoints.push(at);
        self.wires[keep_index]
            .waypoints
            .extend(absorbed.waypoints.iter().copied());
        self.wires[keep_index].to = absorbed.to;

        for other in &mut self.wires {
            for end in [&mut other.from, &mut other.to] {
                if let WireEndpoint::Junction { wire, waypoint } = end {
                    if *wire == absorb {
                        *wire = keep;
                        *waypoint += offset;
                    }
                }
            }
        }
    }

    /// Rebuilds every net from the wires as they are currently drawn.
    ///
    /// This is the whole point of the geometric model: connectivity is
    /// *derived* from the drawing rather than accumulated as wires come and
    /// go, so nothing has to work out what a deletion should undo. Two
    /// parallel wires between the same pins, one removed, simply produce the
    /// same grouping again.
    ///
    /// The grouping is a union-find over three kinds of node: a component
    /// pin, a wire, and nothing at all (a loose end joins nothing). Each
    /// wire unions itself with whatever its two ends touch, so a junction —
    /// which unions with its *host wire* — transitively picks up everything
    /// that wire reaches, however deep the chain goes and in whatever order
    /// they were drawn.
    pub(super) fn rebuild_nets(&mut self) {
        let mut parent: Parents = HashMap::new();

        for placed in &self.placed {
            let pin_count = self
                .circuit
                .try_pins(placed.id())
                .map(|pins| pins.len())
                .unwrap_or(0);
            for index in 0..pin_count {
                let node = Node::Pin(placed.id(), index);
                parent.entry(node).or_insert((node, 0));
            }
        }

        for wire in &self.wires {
            let self_node = Node::Wire(wire.id);
            parent.entry(self_node).or_insert((self_node, 0));
            for end in [wire.from, wire.to] {
                match end {
                    WireEndpoint::Pin(component, index) => {
                        union(&mut parent, self_node, Node::Pin(component, index), 0);
                    }
                    WireEndpoint::Junction { wire: host, .. } => {
                        union(&mut parent, self_node, Node::Wire(host), 0);
                    }
                    // A loose end connects nothing, so it contributes no
                    // union at all.
                    WireEndpoint::Free(_) => {}
                }
            }
        }

        // An instance's innards are not in this drawing, so what held them
        // together has to be put back by hand: the sub-circuit's own groups,
        // and each anchor pin joined to the net its port sat on.
        for placed in &self.placed {
            let Some((ports, inner_groups)) = placed.instance_wiring() else {
                continue;
            };
            // Each internal net gets a node of its own, and everything on it
            // — the innards' pins, and the instance's pins standing in for
            // the ports — is declared a member. One rule instead of two, and
            // it still holds when a net has no pins to link together.
            for (group, members) in inner_groups.iter().enumerate() {
                for &((component, pin), offset) in members {
                    // The pin sits `offset` above the group's node, not the
                    // other way round: `union(a, b, delta)` puts *`a`* above
                    // `b`, and the offset says where the pin sits on its net.
                    union(
                        &mut parent,
                        Node::Pin(component, pin),
                        Node::Inner(placed.id(), group),
                        offset,
                    );
                }
            }
            for (index, port) in ports.iter().enumerate() {
                if let Some(group) = port.group {
                    // Same way round, and the same reason: the anchor pin
                    // stands in for the port, so it sits where the port sat.
                    union(
                        &mut parent,
                        Node::Pin(placed.id(), index),
                        Node::Inner(placed.id(), group),
                        port.offset,
                    );
                }
            }
        }

        // Each pin with where it sits in its group. Everything is at zero
        // until a splitter says otherwise, so a plain drawing comes out of
        // here exactly as it always did.
        type Placed = ((ComponentId, usize), isize);
        // **What the wires say, before the splitters.** Two pins joined by
        // a chain of wires carry the same bits to both ends, so they must
        // agree on how many there are — and that is the only place a width
        // can disagree. A splitter's branch is narrower than its bus on
        // purpose, which the old rule could not tell apart from a mistake
        // because both came out as "narrower than the net".
        //
        // So the faults are read from this, and the nets from the same
        // thing with the splitters applied on top.
        let plain = parent.clone();

        // A splitter says where its branches sit on its bus, and that is the
        // whole of what it is: no component in between, so no tick and no
        // echo. Branch `i` takes the bits after everything before it, and
        // its bit zero is the bus's bit at that running total.
        //
        // Answered false when a loop of splitters says two contradictory
        // things about one conductor — kept for the fault report rather than
        // silently resolved, since picking a winner would leave half the
        // drawing reading bits it does not have.
        let mut contradictions: Vec<(ComponentId, usize)> = Vec::new();
        for placed in &self.placed {
            if placed.kind() != crate::palette::ComponentKind::Splitter {
                continue;
            }
            let mut bit = 0usize;
            for (branch, width) in placed.properties().branch_widths().iter().enumerate() {
                let agreed = union(
                    &mut parent,
                    Node::Pin(placed.id(), branch + 1),
                    Node::Pin(placed.id(), 0),
                    bit as isize,
                );
                if !agreed {
                    contradictions.push((placed.id(), branch + 1));
                }
                bit += width;
            }
        }

        let mut groups: HashMap<Node, Vec<Placed>> = HashMap::new();
        let nodes: Vec<Node> = parent.keys().copied().collect();
        for node in nodes {
            if let Node::Pin(component, index) = node {
                let (root, offset) = find(&mut parent, node);
                groups
                    .entry(root)
                    .or_default()
                    .push(((component, index), offset));
            }
        }

        // How wide each pin is, from the component's properties. The same
        // pass that says what a net *joins* is the one that says how wide it
        // is: both are read off the drawing, and neither can be restated
        // without the other.
        //
        // Per *pin* rather than per component, because a component's pins
        // are not always alike — a transceiver's direction is one bit while
        // its two bus sides are as wide as they are told, and an instance's
        // pins are as wide as the ports they stand for, one by one.
        // Everything an instance carried up about its innards, which are in
        // the engine but not in the drawing.
        let inner_widths: HashMap<(ComponentId, usize), Option<usize>> = self
            .placed
            .iter()
            .flat_map(|placed| placed.inner_pin_widths().iter().copied())
            .collect();
        let by_id: HashMap<ComponentId, &crate::placed_component::PlacedComponent> = self
            .placed
            .iter()
            .map(|placed| (placed.id(), placed))
            .collect();
        // A component the drawing does not list is one flattened in from a
        // sub-circuit; its declarations were carried up with its wiring.
        //
        // `None` is a pin with **no opinion** — a `Probe` — which is why
        // this answers an `Option` rather than a number. Such a pin never
        // widens a net and can never disagree with one, where a pin claiming
        // one bit would do both.
        let declared = |&(component, index): &(ComponentId, usize)| -> Option<usize> {
            match by_id.get(&component) {
                Some(placed) => placed.pin_width(index),
                None => inner_widths.get(&(component, index)).copied().flatten(),
            }
        };
        // **What the wires alone say**, before any splitter joined those
        // groups to the rest of a bus. Two things come out of it: how wide a
        // wire is — a branch carries two bits of an eight-bit conductor, and
        // drawing it as eight would say something false — and what a pin
        // with *no opinion* takes, which is whatever its wire carries.
        let mut plain = plain;
        let mut wired: HashMap<Node, Vec<(ComponentId, usize)>> = HashMap::new();
        for node in plain.keys().copied().collect::<Vec<_>>() {
            if let Node::Pin(component, index) = node {
                let (root, _) = find(&mut plain, node);
                wired.entry(root).or_default().push((component, index));
            }
        }
        let wire_width: HashMap<Node, usize> = wired
            .iter()
            .map(|(&root, pins)| (root, pins.iter().filter_map(declared).max().unwrap_or(1)))
            .collect();
        let carried = |pin: &(ComponentId, usize)| -> usize {
            let root = plain
                .get(&Node::Pin(pin.0, pin.1))
                .map(|_| find(&mut plain.clone(), Node::Pin(pin.0, pin.1)).0);
            root.and_then(|root| wire_width.get(&root))
                .copied()
                .unwrap_or(1)
        };

        // How far the net reaches: the pin that ends highest, measured from
        // the one that starts lowest. With everything at offset zero — which
        // is every drawing until a splitter is in it — that is the widest
        // pin, which is what it always was. A narrower pin then contributes
        // the wrong width and the engine faults it, on every bit: taking the
        // maximum rather than refusing is what makes the mismatch *visible*
        // instead of silently dropped.
        let extent = |group: &[Placed]| -> (isize, usize) {
            let base = group.iter().map(|(_, at)| *at).min().unwrap_or(0);
            let width = group
                .iter()
                .filter_map(|(pin, at)| declared(pin).map(|w| (at - base) as usize + w))
                .max()
                .unwrap_or(1);
            (base, width)
        };

        // Where each group starts, kept before the map below consumes it: a
        // wire needs it to know which of its conductor's bits it carries.
        let bases: HashMap<Node, isize> = groups
            .iter()
            .map(|(&root, group)| (root, group.iter().map(|(_, at)| *at).min().unwrap_or(0)))
            .collect();

        // A lone pin is its own net anyway, which `rewire` already does for
        // anything it isn't told about — but only at one bit, so a wide pin
        // on its own has to be named.
        let groups: Vec<NetGroup> = groups
            .into_values()
            .filter(|group| group.len() > 1 || extent(group).1 > 1)
            .map(|group| {
                let (base, width) = extent(&group);
                // Its **own declared width, at its offset**. A pin narrower
                // than the net is not wrong by itself — a splitter's branch
                // is exactly that — so being narrow is no longer the fault.
                // What is a fault is two pins *at the same offset*
                // disagreeing, which is reported below: those are joined bit
                // for bit by wire, and a conductor cannot be two widths.
                //
                // A pin with no opinion takes **what its wire carries** —
                // a probe reads whatever it is attached to. Not what is left
                // of the net from where it sits: on a splitter's branch that
                // is the rest of the bus, so a probe on bit 1 of eight read
                // seven bits and showed the value shifted.
                let members = group
                    .iter()
                    .map(|(pin, at)| {
                        let offset = (at - base) as usize;
                        Member::slice(*pin, offset, declared(pin).unwrap_or_else(|| carried(pin)))
                    })
                    .collect();
                NetGroup::sliced(members, width)
            })
            .collect();

        // Which pins disagree with the pins they are wired to. Worked out
        // here because here is where both facts are known at once, and
        // recorded per *pin* rather than per net: the net is fine — one
        // thing attached to it is wrong about how wide it is, and saying
        // which is the whole value of the complaint.
        //
        // **At the same offset**, which is what "joined by wire" comes to
        // once offsets exist: a conductor carries the same bits to both
        // ends, so two pins sharing a bit position cannot be two widths. A
        // splitter's branch sits at an offset of its own and is narrower on
        // purpose, which is exactly the case the old rule could not tell
        // apart from a mistake.
        self.width_faults = wired
            .into_values()
            .flat_map(|pins| {
                let widest = pins.iter().filter_map(declared).max();
                pins.into_iter()
                    // The narrower one is named, as it always was: the wider
                    // decides what the conductor carries.
                    .filter(move |pin| declared(pin).is_some_and(|width| Some(width) != widest))
            })
            .collect();
        // A loop of splitters that disagrees with itself puts a pin in two
        // places at once. Reported beside the width faults rather than
        // resolved: picking one of the two would leave half the drawing
        // reading bits it does not have, silently.
        self.width_faults.extend(contradictions);
        // Where each wire sits on its conductor, and how much of it it
        // carries. **Both are needed and neither comes from the net**: a
        // branch off an eight-bit bus is two bits at offset four, and
        // reading the net would give it eight bits starting at zero — which
        // is what made a one-bit branch draw itself in the neutral colour a
        // bus of mixed bits takes.
        self.wire_slices = self
            .wires
            .iter()
            .map(|wire| {
                let (plain_root, _) = find(&mut plain, Node::Wire(wire.id));
                let width = wire_width.get(&plain_root).copied().unwrap_or(1);
                let (root, at) = find(&mut parent, Node::Wire(wire.id));
                let offset = (at - bases.get(&root).copied().unwrap_or(0)).max(0) as usize;
                (wire.id, (offset, width))
            })
            .collect();

        // What a **colour** spreads over: the wires, not the net. Once a
        // splitter joins branches to their bus they are one conductor, so
        // colouring a branch would repaint the bus and every other branch —
        // which is not what "this wire" means to the person clicking it.
        let mut wire_groups: HashMap<Node, Vec<u64>> = HashMap::new();
        for wire in &self.wires {
            let (root, _) = find(&mut plain, Node::Wire(wire.id));
            wire_groups.entry(root).or_default().push(wire.id);
        }
        self.wire_colour_groups = wire_groups.clone();
        self.inherit_wire_colors(wire_groups.into_values());

        self.circuit.rewire(&groups);
    }

    /// Gives a wire the colour of the net it has just joined.
    ///
    /// Only when the group agrees on one: joining two differently coloured
    /// nets leaves both colours in place rather than picking a winner. A
    /// two-tone net is visible and can be re-coloured, whereas a silent
    /// choice is neither.
    pub(super) fn inherit_wire_colors(&mut self, groups: impl Iterator<Item = Vec<u64>>) {
        for group in groups {
            let mut colors = group
                .iter()
                .filter_map(|id| self.wires.iter().find(|wire| wire.id == *id))
                .filter_map(|wire| wire.color);
            let Some(first) = colors.next() else {
                continue;
            };
            if colors.any(|color| color != first) {
                continue;
            }
            for id in group {
                if let Some(wire) = self.wires.iter_mut().find(|wire| wire.id == id) {
                    wire.color = Some(first);
                }
            }
        }
    }

    /// Paints every wire joined to this one **by wire**, which is what
    /// "colour a wire" means: those are one conductor and get one colour.
    ///
    /// Not every wire of the *net*. A splitter makes its branches parts of
    /// its bus, so they share a net — and colouring a branch to tell it
    /// apart would repaint the bus and its seven siblings, which is the
    /// opposite of telling it apart.
    pub(super) fn color_net(&mut self, wire_id: u64, color: Option<[u8; 3]>) {
        let members: Vec<u64> = self
            .wire_colour_groups
            .values()
            .find(|group| group.contains(&wire_id))
            .cloned()
            // A wire the rebuild has not seen yet — one just drawn — takes
            // the colour on its own.
            .unwrap_or_else(|| vec![wire_id]);
        for wire in self.wires.iter_mut().filter(|w| members.contains(&w.id)) {
            wire.color = color;
        }
    }

    /// A hash of the connectivity alone: which components exist, and what
    /// each wire's two ends attach to. Deliberately blind to positions and
    /// waypoint indices — dragging a corner point doesn't change what is
    /// connected to what, and shouldn't cost a rebuild.
    pub(super) fn connectivity_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for placed in &self.placed {
            placed.id().hash(&mut hasher);
            // The declared widths are part of connectivity now: they are
            // what a net's width is derived from, so changing one has to
            // rebuild. Without this the property would move and the net
            // would keep the width it had — leaving the component driving a
            // width the net no longer has, which faults every bit of it.
            //
            // Per *pin*, and read the same way `rebuild_nets` reads it. The
            // component's own `width` property is not the same question and
            // was not enough: regrouping a splitter's branches changes how
            // wide two nets are while leaving that property alone.
            for index in 0..self.circuit.try_pins(placed.id()).map_or(0, <[_]>::len) {
                placed.pin_width(index).hash(&mut hasher);
            }
        }
        for wire in &self.wires {
            wire.id.hash(&mut hasher);
            for end in [wire.from, wire.to] {
                match end {
                    WireEndpoint::Pin(component, index) => {
                        (0u8, component, index).hash(&mut hasher)
                    }
                    // Which waypoint is tapped doesn't matter: any of them
                    // reaches the same wire.
                    WireEndpoint::Junction { wire, .. } => (1u8, wire).hash(&mut hasher),
                    WireEndpoint::Free(_) => 2u8.hash(&mut hasher),
                }
            }
        }
        hasher.finish()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Any node will do — the structure makes no distinction between its
    /// kinds, and a wire's id is a plain number where a `ComponentId` is
    /// only handed out by a real `Circuit`.
    fn pin(index: u64) -> Node {
        Node::Wire(index)
    }

    #[test]
    fn joining_at_the_same_bit_is_what_a_wire_means() {
        let mut parent = Parents::new();
        assert!(union(&mut parent, pin(0), pin(1), 0));
        assert!(union(&mut parent, pin(1), pin(2), 0));

        let (root, _) = find(&mut parent, pin(0));
        for index in 0..3 {
            let (other, offset) = find(&mut parent, pin(index));
            assert_eq!(other, root, "all three are one conductor");
            assert_eq!(offset, 0, "and all at the same bit");
        }
    }

    #[test]
    fn offsets_add_up_along_a_chain() {
        // What a splitter says, twice over: pin 1's bit zero is pin 0's bit
        // 2, and pin 2's is pin 1's bit 3 — so pin 2 sits at bit 5 of the
        // same conductor. Nothing carries the value there; it *is* there.
        let mut parent = Parents::new();
        assert!(union(&mut parent, pin(1), pin(0), 2));
        assert!(union(&mut parent, pin(2), pin(1), 3));

        let base = find(&mut parent, pin(0)).1;
        assert_eq!(find(&mut parent, pin(1)).1 - base, 2);
        assert_eq!(find(&mut parent, pin(2)).1 - base, 5);
    }

    #[test]
    fn a_loop_that_agrees_with_itself_is_not_a_contradiction() {
        // Two paths to the same place, saying the same thing. Redundant
        // wiring is ordinary, and it must not read as a fault.
        let mut parent = Parents::new();
        assert!(union(&mut parent, pin(1), pin(0), 2));
        assert!(union(&mut parent, pin(2), pin(1), 3));
        assert!(
            union(&mut parent, pin(2), pin(0), 5),
            "5 is 2 then 3, which is what the other path already said"
        );
    }

    #[test]
    fn a_loop_that_disagrees_is_reported_rather_than_resolved() {
        // The drawing says two contradictory things about one conductor,
        // which only a loop of splitters can produce. Picking a winner
        // would leave half the circuit reading bits it does not have.
        let mut parent = Parents::new();
        assert!(union(&mut parent, pin(1), pin(0), 2));
        assert!(union(&mut parent, pin(2), pin(1), 3));
        assert!(
            !union(&mut parent, pin(2), pin(0), 4),
            "the other path put pin 2 at bit 5, not 4"
        );
    }

    #[test]
    fn compression_keeps_the_offsets_it_shortens_past() {
        // The trap in a weighted union-find: the shortcut a lookup writes
        // has to carry the whole distance it skipped, or the second lookup
        // answers a different place from the first.
        //
        // Built by hand rather than by `union`, which calls `find` and so
        // compresses as it goes — a chain assembled that way is never deep
        // enough for this to be exercised at all.
        let mut parent = Parents::new();
        parent.insert(pin(0), (pin(0), 0));
        for step in 1..8 {
            parent.insert(pin(step), (pin(step - 1), 1));
        }

        // The **deepest** node first, and that is the whole point: asked in
        // order, every lookup walks one step and compresses one node at
        // distance nothing — where carrying the distance and forgetting it
        // give the same answer, so nothing is being tested.
        assert_eq!(find(&mut parent, pin(7)).1, 7);

        // Everything on that path now points straight at the root, so this
        // reads through the shortcuts rather than down the chain.
        let after: Vec<isize> = (0..8).map(|i| find(&mut parent, pin(i)).1).collect();
        assert_eq!(
            after,
            vec![0, 1, 2, 3, 4, 5, 6, 7],
            "the shortcuts have to carry the distance they skipped"
        );
    }
}
