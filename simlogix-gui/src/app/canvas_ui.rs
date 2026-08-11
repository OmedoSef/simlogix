//! The canvas: the schematic, and every gesture that acts on it.
//!
//! Split out of `app.rs` because it was the bulk of one function — placing,
//! selecting, moving, wiring, cutting and the rubber band, all inside a
//! single `Scene` closure.
//!
//! What has come out of it since is what does *not* share the frame's
//! state: resolving the wire routes (`wiring`) and the camera (`camera`).
//! What is left stays one method, and the reason still holds — the pieces
//! genuinely share the pointer position, the resolved routes and the
//! click-consumed flag, and handing those between a dozen small functions
//! would move the complexity rather than reduce it.
//!
//! A child module rather than a sibling, so `SimLogixApp`'s fields stay
//! private to the rest of the crate.

use std::collections::HashMap;

use simlogix_core::{ComponentId, NetId};

use crate::canvas;
use crate::toolbar::{self, Tool};

use super::wiring::ResolvedRoute;
use super::{
    JunctionTarget, SimLogixApp, WireEndpoint, WireInProgress, MAX_ZOOM, MIN_ZOOM, REATTACH_RADIUS,
    SETTLE_TICKS, WIRE_HIT_RADIUS,
};

impl SimLogixApp {
    /// One frame of the canvas, panels aside.
    pub(super) fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            let wheel = self.take_wheel(ui);
            let mut zoom_pivot = None;
            // Copied out and written back so the closure can still borrow
            // the rest of `self`; `Scene` mutates it in place as the user
            // pans and zooms.
            let mut scene_rect = self.framed_region(ui);
            let pan_buttons = self.pan_buttons();

            let scene_response = egui::Scene::new()
                .zoom_range(MIN_ZOOM..=MAX_ZOOM)
                .drag_pan_buttons(pan_buttons)
                .show(ui, &mut scene_rect, |ui| {
                    // Inside the scene everything is in scene coordinates: the
                    // visible area is the clip rect, and raw pointer positions (which
                    // egui reports globally) have to be mapped in.
                    let canvas_rect = ui.clip_rect();
                    let painter = ui.painter().clone();
                    let to_scene = ui
                        .ctx()
                        .layer_transform_from_global(ui.layer_id())
                        .unwrap_or_default();
                    // Recorded because this is the only place it is known:
                    // it depends on how the panels divided the window and on
                    // what `Scene` then did with what was left. Deriving it
                    // again elsewhere would mean copying `Scene`'s own fitting
                    // rule and drifting from it.
                    #[cfg(test)]
                    {
                        self.canvas_to_screen = to_scene.inverse();
                        self.canvas_layer = Some(ui.layer_id());
                    }
                    // Only the pointer while it's actually over the canvas.
                    // Panels are laid out first, so a click on the palette
                    // still reaches this code -- and, mapped into scene
                    // coordinates, would otherwise read as a canvas click and
                    // (for instance) start a wire under the palette.
                    let pointer_scene = ui
                        .ctx()
                        .pointer_latest_pos()
                        .map(|pos| to_scene * pos)
                        .filter(|pos| canvas_rect.contains(*pos));
                    zoom_pivot = pointer_scene;

                    // The band rides the scene's *own* background response
                    // rather than a widget of its own. A full-canvas
                    // `ui.interact` here covered that background, which is
                    // what placement and panning both go through — so it
                    // silently broke both. The origin is set after the scene
                    // closes (see below); this only paints it and notices the
                    // release, which is what has to happen in here, where
                    // every wire's resolved route is known.
                    let mut band_finished = None;
                    let released = ui.ctx().input(|i| i.pointer.primary_released());
                    if self.bands_on_left_drag() {
                        if let (Some(origin), Some(now)) = (self.band_origin, pointer_scene) {
                            let rect = egui::Rect::from_two_pos(origin, now);
                            let accent = canvas::accent_color(ui.visuals().dark_mode);
                            painter.rect_filled(rect, 0.0, accent.gamma_multiply(0.12));
                            painter.rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(1.0, accent),
                                egui::StrokeKind::Inside,
                            );
                            if released {
                                band_finished = Some(rect);
                            }
                        }
                        if released {
                            self.band_origin = None;
                        }
                    } else {
                        self.band_origin = None;
                    }

                    // Faint enough to stay a background on either theme: the
                    // weak text colour already tracks the background, and the
                    // alpha keeps the dots from competing with the circuit.
                    canvas::draw_grid(
                        &painter,
                        canvas_rect,
                        ui.visuals().weak_text_color().gamma_multiply(0.55),
                    );

                    // The appearance view shares the grid, the camera and the
                    // pointer mapping and nothing else: no components, no
                    // wires, no simulation. Branching here rather than at the
                    // panel keeps all of that in one place.
                    if self.view == toolbar::View::Appearance {
                        self.appearance_ui(ui, &painter, pointer_scene);
                        return;
                    }

                    // The simulation view is the *same* drawing, with every
                    // gesture that edits taken away rather than a copy of it
                    // put on screen. One flag, read by each thing that could
                    // change the document — components stop being draggable,
                    // pins stop starting wires, waypoints and loose ends stop
                    // answering at all, and the keyboard stops deleting.
                    let editing = self.view == toolbar::View::Schematic;

                    // Rotation applies to everything selected. Each turns on
                    // its own centre rather than the group's: a component's
                    // pins have to land on the grid, and turning the group as
                    // one body would put them between dots.
                    // `Shift+R` reflects instead of turning, and is tested
                    // first: egui matches modifiers exactly, so the two can
                    // never both fire, but the order says which wins if
                    // that ever stops holding.
                    //
                    // A mirror is not a rotation. Four quarter-turns all
                    // preserve the order of a symbol's pins; a splitter used
                    // as a merger wants to face the other way *without* its
                    // branches ending up bottom to top, which is exactly
                    // what a half turn does to them.
                    let reading_keys = editing && !ui.ctx().text_edit_focused();
                    // `consume_key` matches the modifiers exactly and takes
                    // the event, so a `Shift+R` cannot also read as a plain
                    // `R` below — and the chord is read off the event
                    // rather than the modifier state, which is how every
                    // other shortcut here is matched.
                    let mirror_pressed = reading_keys
                        && ui
                            .ctx()
                            .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::R));
                    let rotate_pressed =
                        reading_keys && ui.ctx().input(|i| i.key_pressed(egui::Key::R));

                    if mirror_pressed && matches!(self.tool, Tool::Place(_)) {
                        self.place_mirrored = !self.place_mirrored;
                    } else if mirror_pressed && !self.selection.components.is_empty() {
                        self.record_edit();
                        let chosen = self.selection.components.clone();
                        for placed in &mut self.placed {
                            if chosen.contains(&placed.id()) {
                                // Each about its own centre, as rotation
                                // is: reflecting the group as a body would
                                // move every pin off the grid.
                                let now = !placed.is_mirrored();
                                placed.set_mirrored(now);
                            }
                        }
                        self.dirty = true;
                    }

                    // While something is queued for placement, `R` turns
                    // *that* rather than the selection: it is the thing under
                    // your pointer, and rotating after dropping means placing
                    // it wrong first on purpose.
                    if rotate_pressed && matches!(self.tool, Tool::Place(_)) {
                        self.place_rotation = self.place_rotation.next_clockwise();
                    } else if rotate_pressed && !self.selection.components.is_empty() {
                        self.record_edit();
                        let chosen = self.selection.components.clone();
                        for placed in &mut self.placed {
                            if chosen.contains(&placed.id()) {
                                placed.rotate();
                            }
                        }
                    }

                    // Shift turns a click into "add to what's already there",
                    // the gesture every list and canvas uses.
                    let extend_selection = ui.ctx().input(|i| i.modifiers.shift);

                    // Set last frame, so the pin positions below are the ones
                    // this component actually came to rest on.
                    let landed = self.pending_attach.take();

                    // Whether this frame's click landed on something selectable (a
                    // component, a wire, a waypoint, a pin). A click that lands on
                    // nothing is a click on empty canvas, which clears the selection
                    // -- see the end of this closure.
                    let mut click_consumed = false;
                    // Collected during the component loop and acted on after it:
                    // `record_edit` needs all of `self`, which isn't available while
                    // `self.placed` is being iterated mutably.
                    let mut grab_started = false;
                    let mut input_changed = false;
                    // Which selected component the pointer is actually
                    // dragging, and by how much: the rest of the selection is
                    // carried by the same amount once the loop is done, so the
                    // group keeps its shape.
                    let mut group_drag: Option<(ComponentId, egui::Vec2)> = None;
                    let mut group_settled = false;
                    // A switch that wants flipping. Applied after the loop
                    // because its position is document data: the undo
                    // snapshot has to be taken while it still holds the old
                    // one, and `record_edit` needs all of `self`.
                    let mut toggled_switch: Option<ComponentId> = None;
                    let chosen = self.selection.components.clone();

                    let mut pin_handles = Vec::new();
                    for placed in &mut self.placed {
                        let is_selected = self.selection.components.contains(&placed.id());
                        let frame = placed.draw_and_interact(
                            ui,
                            &painter,
                            &mut self.circuit,
                            is_selected,
                            editing,
                            self.base,
                        );
                        if let Some(id) = frame.clicked {
                            self.selection.pick_component(id, extend_selection);
                            click_consumed = true;
                        }
                        grab_started |= frame.grab_started;
                        input_changed |= frame.input_changed;
                        if frame.toggled {
                            toggled_switch = Some(placed.id());
                        }
                        if is_selected && frame.dragged_by != egui::Vec2::ZERO {
                            group_drag = Some((placed.id(), frame.dragged_by));
                        }
                        if frame.settled {
                            group_settled |= is_selected;
                            self.pending_attach = Some(placed.id());
                        }
                        #[cfg(test)]
                        self.pin_positions.insert(
                            placed.id(),
                            frame.pins.iter().map(|pin| pin.position).collect(),
                        );
                        pin_handles.extend(frame.pins);
                    }

                    if let Some(id) = toggled_switch {
                        // Straight into the cell the engine reads, never
                        // through the document: where a switch is *now* is
                        // runtime state, and its property is where it rests
                        // when the project opens. So no snapshot, no dirty
                        // flag, and no undo step — flipping one to see what
                        // happens is not an edit to the drawing.
                        if let Some(on) = self
                            .placed
                            .iter()
                            .find(|p| p.id() == id)
                            .and_then(|placed| placed.switch_position())
                        {
                            on.set(!on.get());
                        }
                        self.circuit.schedule_now(id);
                        input_changed = true;
                    }

                    if let Some((mover, delta)) = group_drag {
                        // Everything but the one under the pointer, which
                        // `interact_box` has already moved.
                        for placed in &mut self.placed {
                            if placed.id() != mover && chosen.contains(&placed.id()) {
                                placed.move_by(delta);
                            }
                        }
                        // A selected wire's own points come along; the ends
                        // that sit on pins follow those pins anyway.
                        for wire in &mut self.wires {
                            if self.selection.wires.contains(&wire.id) {
                                for point in &mut wire.waypoints {
                                    *point += delta;
                                }
                                for end in [&mut wire.from, &mut wire.to] {
                                    if let WireEndpoint::Free(at) = end {
                                        *at += delta;
                                    }
                                }
                            }
                        }
                    }
                    if group_settled {
                        for placed in &mut self.placed {
                            if chosen.contains(&placed.id()) {
                                placed.snap();
                            }
                        }
                        for wire in &mut self.wires {
                            if self.selection.wires.contains(&wire.id) {
                                for point in &mut wire.waypoints {
                                    *point = canvas::snap_to_grid(*point);
                                }
                                for end in [&mut wire.from, &mut wire.to] {
                                    if let WireEndpoint::Free(at) = end {
                                        *at = canvas::snap_to_grid(*at);
                                    }
                                }
                            }
                        }
                    }
                    if grab_started {
                        self.record_edit();
                    }
                    // A button press is runtime state, not an edit: it settles
                    // the circuit but never touches undo.
                    if input_changed {
                        self.advance_circuit(SETTLE_TICKS);
                    }
                    let click_pos = ui
                        .ctx()
                        .input(|i| i.pointer.primary_clicked())
                        .then_some(pointer_scene)
                        .flatten();
                    // Double-clicking along an existing wire inserts a new waypoint
                    // right there, so a wire can be reshaped in more places than
                    // just its existing points.
                    // Right-clicking a wire cuts the segment under the pointer.
                    // Both are cut off here rather than at each thing they
                    // reach: a cut, a waypoint inserted, a waypoint removed —
                    // one `editing` at the source is one place to be right.
                    let secondary_click_pos = (editing
                        && ui.ctx().input(|i| i.pointer.secondary_clicked()))
                    .then_some(pointer_scene)
                    .flatten();
                    let double_click_pos = (editing
                        && ui.ctx().input(|i| {
                            i.pointer
                                .button_double_clicked(egui::PointerButton::Primary)
                        }))
                    .then_some(pointer_scene)
                    .flatten();

                    let hover_pos = pointer_scene;

                    // Resolved once per frame, before anything is drawn --
                    // see `resolve_routes`.
                    // Ringed in the error colour, on the pin rather than on
                    // the net: the net is fine and one thing attached to it
                    // is wrong about how wide it is. Drawn from the handles
                    // the component loop just collected, so nothing about
                    // drawing a component had to learn about widths.
                    for &(component, index) in &self.width_faults {
                        if let Some(handle) = pin_handles.iter().find(|handle| {
                            handle.component == component && handle.pin_index == index
                        }) {
                            painter.circle_stroke(
                                handle.position,
                                6.0,
                                egui::Stroke::new(
                                    2.0,
                                    canvas::signal_color(
                                        simlogix_core::Level::Error,
                                        ui.visuals().dark_mode,
                                    ),
                                ),
                            );
                        }
                    }

                    let mut bus_hint_shown = false;
                    let mut resolved = self.resolve_routes(&pin_handles);

                    // Where every wire's points ended up this frame, kept past
                    // the loop below: deleting a wire has to know where the taps
                    // on it currently sit so it can leave them there.
                    let resolved_waypoints: HashMap<u64, Vec<egui::Pos2>> = resolved
                        .iter()
                        .map(|(&id, r)| (id, r.waypoints.clone()))
                        .collect();
                    // Likewise where each wire's two ends sit, so deleting a
                    // component can leave its wires loose exactly where they
                    // were attached.
                    let wire_ends: HashMap<u64, (egui::Pos2, egui::Pos2)> = resolved
                        .iter()
                        .map(|(&id, r)| (id, (r.from, r.to)))
                        .collect();

                    // Finishing a new wire on top of another wire's waypoint (a
                    // junction tap) is decided inside the loop below but applied
                    // after it, to keep `self.wires` stable (an unchanging length,
                    // no reallocation) for the whole iteration.
                    let mut junction_finish: Option<JunctionTarget> = None;
                    // The mirror of the above for the *start* of a wire: with the
                    // wire tool, clicking an existing wire begins a new one tapped
                    // onto it rather than merely selecting it.
                    let mut junction_start: Option<(Option<NetId>, JunctionTarget, egui::Pos2)> =
                        None;
                    // Deleting a waypoint mutates the wire list, so it's
                    // decided in the loop and applied after it.
                    let mut waypoint_to_remove: Option<(u64, usize, Vec<egui::Pos2>)> = None;
                    // Likewise for cutting a segment out of a wire.
                    let mut segment_to_cut: Option<(u64, usize, Vec<egui::Pos2>)> = None;
                    // And for joining two wires: it removes one of them, which
                    // would leave this loop indexing past the end.
                    let mut wires_to_join: Option<(u64, bool, u64, bool, egui::Pos2)> = None;

                    // Which net the pointer is over, worked out before any
                    // wire is drawn. Hovering has to light up the *whole*
                    // net: following one conductor across a crossing is the
                    // difficulty, and highlighting the single segment under
                    // the cursor doesn't help with it at all.
                    let hovered_net =
                        hover_pos
                            .filter(|_| self.wiring_from.is_none())
                            .and_then(|pos| {
                                self.wires.iter().find_map(|wire| {
                                    let route = resolved.get(&wire.id)?;
                                    let mut path = vec![route.from];
                                    path.extend(route.waypoints.iter().copied());
                                    path.push(route.to);
                                    if canvas::distance_to_path(pos, &path) < WIRE_HIT_RADIUS {
                                        self.wire_net(wire)
                                    } else {
                                        None
                                    }
                                })
                            });

                    for i in 0..self.wires.len() {
                        let wire_id = self.wires[i].id;
                        let wire_color = self.wires[i].color;
                        // `None` while both ends are loose: the wire is drawing,
                        // not yet a connection. Still very much on screen.
                        let net = self.wire_net(&self.wires[i]);
                        let Some(ResolvedRoute {
                            from: from_pos,
                            to: to_pos,
                            waypoints,
                        }) = resolved.remove(&wire_id)
                        else {
                            continue; // Stale, already skipped above.
                        };

                        let user_color =
                            wire_color.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b));
                        // With the signal state showing, the core is the
                        // level and a colour of your own rings it. With the
                        // state hidden the core has nothing left to say, so
                        // the colour takes it over — a casing around a core
                        // that reports nothing is just a thicker wire.
                        let color = if self.show_signal_state {
                            match net {
                                Some(net) => {
                                    // **Its own bits.** A branch off a bus
                                    // shares the net, so reading that would
                                    // paint a one-bit wire in the neutral
                                    // colour a bus of mixed bits takes —
                                    // and it would never show its level.
                                    let level = canvas::bus_color(
                                        &self.wire_signal(wire_id, Some(net)),
                                        ui.visuals().dark_mode,
                                    );
                                    // Faded when nothing but a pass
                                    // transistor is holding it up: the level
                                    // is real, the noise margin is gone, and
                                    // that is worth seeing before it bites.
                                    if self.circuit.is_weakly_driven(net) {
                                        level.gamma_multiply(canvas::WEAK_FADE)
                                    } else {
                                        level
                                    }
                                }
                                None => ui.visuals().weak_text_color(),
                            }
                        } else {
                            user_color.unwrap_or_else(|| match net {
                                Some(_) => ui.visuals().strong_text_color(),
                                None => ui.visuals().weak_text_color(),
                            })
                        };
                        let mut path = vec![from_pos];
                        path.extend(waypoints.iter().copied());
                        path.push(to_pos);

                        // Hovering a wire thickens it, so it's obvious which one a
                        // click is about to select out of several crossing ones.
                        // Wires are polylines, so this is a distance test rather
                        // than an `ui.interact` rect like the widgets use.
                        // On the same net as whatever is under the pointer —
                        // or, for a wire that reaches no pin and so has no
                        // net, under the pointer itself.
                        let is_hovered = self.wiring_from.is_none()
                            && match (net, hovered_net) {
                                (Some(net), Some(hovered)) => net == hovered,
                                _ => hover_pos.is_some_and(|pos| {
                                    canvas::distance_to_path(pos, &path) < WIRE_HIT_RADIUS
                                }),
                            };
                        if is_hovered {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            // How wide it is, without having to select it.
                            // Only for a bus: a plain wire's width is the
                            // default, and saying "1 bit" over every wire on
                            // the schematic is noise rather than an answer.
                            //
                            // Once per frame, since hovering lights the whole
                            // net and several wires may be part of it.
                            let width = self.wire_width(wire_id);
                            if width > 1 && !bus_hint_shown {
                                bus_hint_shown = true;
                                self.show_bus_hint(
                                    ui,
                                    crate::i18n::Strings::for_language(self.language),
                                    wire_id,
                                    width,
                                    net,
                                );
                            }
                        }

                        let is_selected_wire = self.selection.wires.contains(&wire_id);
                        // A bus is drawn heavier than a wire, so the two can
                        // be told apart at a glance — which everything else
                        // about buses depends on. Not proportional to the
                        // width: what matters is one bit against several,
                        // and a 32-bit wire as thick as a component would
                        // be a schematic nobody can read.
                        let bus = self.wire_width(wire_id) > 1;
                        let heavier = if bus { 2.0 } else { 0.0 };
                        let stroke = if is_selected_wire {
                            egui::Stroke::new(
                                3.0 + heavier,
                                canvas::accent_color(ui.visuals().dark_mode),
                            )
                        } else if is_hovered {
                            egui::Stroke::new(3.0 + heavier, color.gamma_multiply(1.6))
                        } else {
                            egui::Stroke::new(2.0 + heavier, color)
                        };

                        // The user's colour goes *underneath*, as a casing:
                        // the signal colour keeps the full width of the core,
                        // so the thing that changes during simulation stays
                        // the thing the eye reads first.
                        // Only while the core carries the level. Once the
                        // colour *is* the core, a casing would be the same
                        // colour twice.
                        if self.show_signal_state {
                            if let Some(casing) = user_color {
                                canvas::draw_path(
                                    &painter,
                                    &path,
                                    egui::Stroke::new(stroke.width + 4.0, casing),
                                );
                            }
                        }
                        canvas::draw_path(&painter, &path, stroke);
                        for &point in &waypoints {
                            painter.circle_filled(point, 3.5, stroke.color);
                        }

                        // Only select an existing wire by clicking on it, or reshape
                        // it by dragging a waypoint, while not actively placing a
                        // new one -- otherwise a click meant to add a waypoint to
                        // the new wire would hijack this one instead.
                        if self.wiring_from.is_none() {
                            if let Some(click) = click_pos {
                                if canvas::distance_to_path(click, &path) < WIRE_HIT_RADIUS {
                                    if self.tool == Tool::Wire {
                                        if let Some((segment, _)) =
                                            canvas::closest_segment(&path, click)
                                        {
                                            let at = canvas::snap_to_grid(click);
                                            let mut inserted = waypoints.clone();
                                            inserted.insert(segment, at);
                                            junction_start = Some((
                                                net,
                                                JunctionTarget::Insert {
                                                    wire: wire_id,
                                                    waypoint: segment,
                                                    waypoints: inserted,
                                                },
                                                at,
                                            ));
                                        }
                                    } else {
                                        self.selection.pick_wire(wire_id, extend_selection);
                                    }
                                    click_consumed = true;
                                }
                            }

                            if let Some(pos) = secondary_click_pos {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, pos)
                                {
                                    if distance < WIRE_HIT_RADIUS {
                                        segment_to_cut = Some((wire_id, segment, path.clone()));
                                    }
                                }
                            }

                            if let Some(dbl_pos) = double_click_pos {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, dbl_pos)
                                {
                                    if distance < 6.0 {
                                        self.record_edit();
                                        self.wires[i]
                                            .waypoints
                                            .insert(segment, canvas::snap_to_grid(dbl_pos));
                                        self.dedupe_waypoints(wire_id);
                                        self.selection.pick_wire(wire_id, extend_selection);
                                        click_consumed = true;
                                    }
                                }
                            }
                        }

                        // Connecting to an existing wire shouldn't mean
                        // aiming at one of its dots: while drawing, a click
                        // anywhere along a wire drops a contact point right
                        // there and finishes on it. Only used if no existing
                        // waypoint claimed the click below, so a deliberate
                        // hit on a dot still reuses that dot.
                        if let (Some(in_progress), Some(click)) = (&self.wiring_from, click_pos) {
                            // Finishing is allowed even onto the same net: the
                            // wire is still a real drawn connection, and refusing
                            // silently just left the wire stuck to the cursor with
                            // no hint as to why. The one thing to refuse is
                            // finishing on the very point the wire started from,
                            // which would be a wire of no length.
                            let starting_here = in_progress.waypoints.is_empty()
                                && matches!(
                                    in_progress.from,
                                    WireEndpoint::Junction { wire: host, .. } if host == wire_id
                                );
                            if !starting_here {
                                if let Some((segment, distance)) =
                                    canvas::closest_segment(&path, click)
                                {
                                    if distance < WIRE_HIT_RADIUS {
                                        let mut inserted = waypoints.clone();
                                        inserted.insert(segment, canvas::snap_to_grid(click));
                                        junction_finish = Some(JunctionTarget::Insert {
                                            wire: wire_id,
                                            waypoint: segment,
                                            waypoints: inserted,
                                        });
                                    }
                                }
                            }
                        }

                        // Both ends can be loose (splitting a wire makes one of
                        // each), so both get the same treatment: drawn hollow --
                        // it reads as "attached to nothing", unlike the filled
                        // dots of real waypoints -- with a handle to move it or
                        // drop it back onto something.
                        for (is_from, at) in [(true, from_pos), (false, to_pos)] {
                            let end = if is_from {
                                self.wires[i].from
                            } else {
                                self.wires[i].to
                            };
                            if !matches!(end, WireEndpoint::Free(_)) {
                                continue;
                            }

                            painter.circle_stroke(at, 4.0, stroke);
                            // Still drawn, so a loose end is visible while a
                            // circuit runs — just not something that answers.
                            let response = ui.interact(
                                egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)),
                                egui::Id::new(("wire_end", wire_id, is_from)),
                                if editing {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::hover()
                                },
                            );
                            if response.hovered() {
                                painter.circle_stroke(
                                    at,
                                    7.0,
                                    egui::Stroke::new(
                                        1.5,
                                        canvas::accent_color(ui.visuals().dark_mode),
                                    ),
                                );
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if response.drag_started() {
                                self.record_edit();
                            }
                            if response.dragged() {
                                let end = if is_from {
                                    &mut self.wires[i].from
                                } else {
                                    &mut self.wires[i].to
                                };
                                if let WireEndpoint::Free(pos) = end {
                                    *pos += response.drag_delta();
                                }
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            if response.drag_stopped() {
                                // Dropping a loose end on a pin or on another
                                // wire's point re-attaches it there, which is what
                                // makes a dangling wire worth keeping: swap a gate
                                // out and drag its wires onto the new one instead
                                // of redrawing them. Anywhere else, it just stays
                                // put, snapped to the grid.
                                let current = if is_from {
                                    self.wires[i].from
                                } else {
                                    self.wires[i].to
                                };
                                let dropped = match current {
                                    WireEndpoint::Free(pos) => canvas::snap_to_grid(pos),
                                    _ => at,
                                };

                                // Another wire's loose end: the two are joined
                                // into one rather than one tapping the other,
                                // which is what "these two pieces are the same
                                // wire again" actually means.
                                let onto_loose_end = self
                                    .wires
                                    .iter()
                                    .filter(|w| w.id != wire_id)
                                    .find_map(|other| {
                                        let ends = wire_ends.get(&other.id)?;
                                        [(true, other.from, ends.0), (false, other.to, ends.1)]
                                            .into_iter()
                                            .find(|(_, end, point)| {
                                                matches!(end, WireEndpoint::Free(_))
                                                    && point.distance(dropped) < REATTACH_RADIUS
                                            })
                                            .map(|(other_is_from, _, _)| (other.id, other_is_from))
                                    });

                                if let Some((other_id, other_is_from)) = onto_loose_end {
                                    wires_to_join =
                                        Some((wire_id, is_from, other_id, other_is_from, dropped));
                                    continue;
                                }

                                let onto_pin = pin_handles
                                    .iter()
                                    .find(|h| h.position.distance(dropped) < REATTACH_RADIUS)
                                    .map(|h| (WireEndpoint::Pin(h.component, h.pin_index), h.net));

                                let onto_wire = onto_pin
                                    .is_none()
                                    .then(|| {
                                        resolved_waypoints
                                            .iter()
                                            .filter(|(&id, _)| id != wire_id)
                                            .flat_map(|(&id, points)| {
                                                points
                                                    .iter()
                                                    .enumerate()
                                                    .map(move |(index, &point)| (id, index, point))
                                            })
                                            .find(|(_, _, point)| {
                                                point.distance(dropped) < REATTACH_RADIUS
                                            })
                                            .and_then(|(host, index, _)| {
                                                let net = self
                                                    .wires
                                                    .iter()
                                                    .find(|w| w.id == host)
                                                    .and_then(|w| self.wire_net(w))?;
                                                Some((
                                                    WireEndpoint::Junction {
                                                        wire: host,
                                                        waypoint: index,
                                                    },
                                                    net,
                                                ))
                                            })
                                    })
                                    .flatten();

                                // Anywhere along another wire, not just on one of
                                // its points: drop a contact point there and tap
                                // it, exactly as drawing a wire onto another one
                                // does. Without this, aiming between two points
                                // silently did nothing and the end stayed loose.
                                let onto_path = (onto_pin.is_none() && onto_wire.is_none())
                                    .then(|| {
                                        let mut best: Option<(u64, usize, f32)> = None;
                                        for (&host, points) in &resolved_waypoints {
                                            if host == wire_id {
                                                continue;
                                            }
                                            let Some(&(host_from, host_to)) = wire_ends.get(&host)
                                            else {
                                                continue;
                                            };
                                            let mut host_path = vec![host_from];
                                            host_path.extend(points.iter().copied());
                                            host_path.push(host_to);
                                            let Some((segment, distance)) =
                                                canvas::closest_segment(&host_path, dropped)
                                            else {
                                                continue;
                                            };
                                            if distance < REATTACH_RADIUS
                                                && best.is_none_or(|(_, _, best)| distance < best)
                                            {
                                                best = Some((host, segment, distance));
                                            }
                                        }
                                        best.map(|(host, segment, _)| (host, segment))
                                    })
                                    .flatten();

                                if let Some((host, segment)) = onto_path {
                                    if let Some(host_index) =
                                        self.wires.iter().position(|w| w.id == host)
                                    {
                                        self.wires[host_index].waypoints.insert(segment, dropped);
                                        self.shift_junctions(host, segment, 1);
                                        self.dedupe_waypoints(host);
                                        let endpoint = WireEndpoint::Junction {
                                            wire: host,
                                            waypoint: segment,
                                        };
                                        if is_from {
                                            self.wires[i].from = endpoint;
                                        } else {
                                            self.wires[i].to = endpoint;
                                        }
                                        continue;
                                    }
                                }

                                let endpoint = match onto_pin.or(onto_wire) {
                                    Some((endpoint, _)) => endpoint,
                                    None => WireEndpoint::Free(dropped),
                                };
                                if is_from {
                                    self.wires[i].from = endpoint;
                                } else {
                                    self.wires[i].to = endpoint;
                                }
                                // Re-read the net: this wire may have had none at
                                // all while it dangled, in which case the pin it
                                // just landed on is now its net and there's
                                // nothing to merge.
                            }
                            if response.clicked() {
                                self.selection.pick_wire(wire_id, extend_selection);
                                click_consumed = true;
                            }
                        }

                        for (waypoint_index, &point) in waypoints.iter().enumerate() {
                            let handle_rect =
                                egui::Rect::from_center_size(point, egui::vec2(10.0, 10.0));
                            let response = ui.interact(
                                handle_rect,
                                egui::Id::new(("wire_point", wire_id, waypoint_index)),
                                if editing {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::hover()
                                },
                            );

                            // A waypoint doubles as a junction target, so it gets a
                            // ring on hover the same as a pin -- that's the cue that
                            // you can drop a wire onto it, not just drag it.
                            if response.hovered() {
                                painter.circle_stroke(
                                    point,
                                    6.0,
                                    egui::Stroke::new(
                                        1.5,
                                        canvas::accent_color(ui.visuals().dark_mode),
                                    ),
                                );
                                ui.ctx().set_cursor_icon(if self.wiring_from.is_some() {
                                    egui::CursorIcon::Crosshair
                                } else {
                                    egui::CursorIcon::Grab
                                });
                            }

                            if let Some(in_progress) = &self.wiring_from {
                                // A wire is being drawn: clicking another wire's
                                // waypoint taps into it as a junction, finishing the
                                // new wire here instead of on a pin.
                                let starting_here = in_progress.waypoints.is_empty()
                                    && matches!(
                                        in_progress.from,
                                        WireEndpoint::Junction { wire: host, waypoint }
                                            if host == wire_id && waypoint == waypoint_index
                                    );
                                if response.clicked() && !starting_here {
                                    junction_finish = Some(JunctionTarget::Existing {
                                        wire: wire_id,
                                        waypoint: waypoint_index,
                                    });
                                }
                            } else {
                                if response.drag_started() {
                                    self.record_edit();
                                }
                                if response.dragged() {
                                    self.wires[i].waypoints[waypoint_index] +=
                                        response.drag_delta();
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                                if response.drag_stopped() {
                                    if let Some(p) = self.wires[i].waypoints.get_mut(waypoint_index)
                                    {
                                        *p = canvas::snap_to_grid(*p);
                                    }
                                }
                                if response.clicked() {
                                    self.selection.pick_wire(wire_id, extend_selection);
                                    click_consumed = true;
                                }
                                if response.secondary_clicked() {
                                    waypoint_to_remove =
                                        Some((wire_id, waypoint_index, waypoints.clone()));
                                }
                            }
                        }
                    }

                    // A right-click that landed on a waypoint means "remove
                    // that point", not "cut the segment it happens to sit on".
                    // A pin that came to rest on a loose wire end picks it up —
                    // the mirror of dragging that end onto the pin. No
                    // `record_edit` here: the move (or placement) that brought
                    // it here already took the snapshot, so undoing that undoes
                    // this along with it.
                    if let Some(component) = landed {
                        let mut attach: Vec<(u64, bool, usize, NetId)> = Vec::new();
                        for handle in pin_handles.iter().filter(|h| h.component == component) {
                            for wire in &self.wires {
                                let Some(ends) = wire_ends.get(&wire.id) else {
                                    continue;
                                };
                                for (is_from, end, at) in
                                    [(true, wire.from, ends.0), (false, wire.to, ends.1)]
                                {
                                    let already_taken = attach
                                        .iter()
                                        .any(|&(id, from, _, _)| id == wire.id && from == is_from);
                                    if !already_taken
                                        && matches!(end, WireEndpoint::Free(_))
                                        && at.distance(handle.position) < REATTACH_RADIUS
                                    {
                                        attach.push((
                                            wire.id,
                                            is_from,
                                            handle.pin_index,
                                            handle.net,
                                        ));
                                    }
                                }
                            }
                        }

                        for (wire_id, is_from, pin_index, _) in attach {
                            let Some(index) = self.wires.iter().position(|w| w.id == wire_id)
                            else {
                                continue;
                            };
                            let endpoint = WireEndpoint::Pin(component, pin_index);
                            if is_from {
                                self.wires[index].from = endpoint;
                            } else {
                                self.wires[index].to = endpoint;
                            }
                            self.dirty = true;
                        }
                    }

                    if let Some((keep, keep_is_from, absorb, absorb_is_from, at)) = wires_to_join {
                        self.join_wires(keep, keep_is_from, absorb, absorb_is_from, at);
                        self.dedupe_waypoints(keep);
                    }

                    // Whether the right-click was aimed at a wire, in which case
                    // it shouldn't also clear the selection as a side effect.
                    let consumed_secondary =
                        waypoint_to_remove.is_some() || segment_to_cut.is_some();
                    if let Some((wire_id, index, resolved)) = waypoint_to_remove {
                        self.remove_waypoint(wire_id, index, &resolved);
                    } else if let Some((wire_id, segment, path)) = segment_to_cut {
                        self.split_wire(wire_id, segment, &path);
                    }

                    // Applied here rather than where the band ends, because
                    // this is where every wire's resolved route is known.
                    if let Some(rect) = band_finished {
                        if !extend_selection {
                            self.selection.clear();
                        }
                        for placed in &self.placed {
                            // Its own box: an instance is taller than one
                            // grid cell, and a band that missed it would be
                            // the kind of bug nobody thinks to report.
                            if rect.intersects(placed.rect()) {
                                self.selection.components.insert(placed.id());
                            }
                        }
                        for wire in &self.wires {
                            let ends_inside = wire_ends
                                .get(&wire.id)
                                .is_some_and(|(a, b)| rect.contains(*a) || rect.contains(*b));
                            let points_inside = resolved_waypoints
                                .get(&wire.id)
                                .is_some_and(|points| points.iter().any(|p| rect.contains(*p)));
                            if ends_inside || points_inside {
                                self.selection.wires.insert(wire.id);
                            }
                        }
                        // A band that swept over nothing is still a deliberate
                        // gesture, not a click on empty canvas.
                        click_consumed = true;
                    }

                    let delete_pressed = editing
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| {
                            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                        });
                    if delete_pressed && !self.selection.is_empty() {
                        self.record_edit();

                        let doomed_wires: Vec<u64> = self.selection.wires.iter().copied().collect();
                        if !doomed_wires.is_empty() {
                            self.remove_wires(doomed_wires, &resolved_waypoints);
                        }

                        let doomed = self.selection.components.clone();
                        for &component in &doomed {
                            self.circuit.remove_component(component);
                        }
                        self.placed.retain(|placed| !doomed.contains(&placed.id()));

                        // The wires that touched it are kept, cut loose
                        // where the pin used to be: redrawing them is far
                        // more work than deleting one you didn't want, and
                        // a loose end can be dragged straight onto the
                        // replacement component.
                        for wire in &mut self.wires {
                            for (end, at) in [
                                (&mut wire.from, wire_ends.get(&wire.id).map(|e| e.0)),
                                (&mut wire.to, wire_ends.get(&wire.id).map(|e| e.1)),
                            ] {
                                if matches!(*end, WireEndpoint::Pin(c, _) if doomed.contains(&c)) {
                                    if let Some(at) = at {
                                        *end = WireEndpoint::Free(at);
                                    }
                                }
                            }
                        }
                        self.selection.clear();
                    }

                    // A wire being placed click by click: clicking a pin starts one
                    // (or finishes it, if one's already in progress and this pin is
                    // on a different net); clicking empty canvas along the way adds
                    // a grid-snapped waypoint; Escape cancels it.
                    let clicked_pin =
                        pin_handles
                            .iter()
                            .find(|handle| handle.clicked)
                            .map(|handle| {
                                (
                                    handle.component,
                                    handle.pin_index,
                                    handle.net,
                                    handle.position,
                                )
                            });

                    if let Some((component, pin_index, net, position)) = clicked_pin {
                        click_consumed = true;
                        if let Some(in_progress) = self.wiring_from.take() {
                            if in_progress.net != Some(net) {
                                self.record_edit();
                                self.add_wire(
                                    in_progress.from,
                                    WireEndpoint::Pin(component, pin_index),
                                    in_progress.waypoints,
                                );
                            } else {
                                // Clicked back onto the same net (e.g. the wire's
                                // own start pin) -- not a valid finish, keep going.
                                self.wiring_from = Some(in_progress);
                            }
                        } else {
                            self.wiring_from = Some(WireInProgress {
                                from: WireEndpoint::Pin(component, pin_index),
                                net: Some(net),
                                anchor: position,
                                waypoints: Vec::new(),
                            });
                        }
                    } else if let Some(target) = junction_finish {
                        if let Some(in_progress) = self.wiring_from.take() {
                            self.record_edit();

                            let (host_wire, host_waypoint) = match target {
                                JunctionTarget::Existing { wire, waypoint } => (wire, waypoint),
                                JunctionTarget::Insert {
                                    wire,
                                    waypoint,
                                    waypoints,
                                } => {
                                    if let Some(host) = self.wires.iter_mut().find(|w| w.id == wire)
                                    {
                                        host.waypoints = waypoints;
                                    }
                                    // Points at or past the new one shifted
                                    // along; taps on them have to follow.
                                    self.shift_junctions(wire, waypoint, 1);
                                    (wire, waypoint)
                                }
                            };

                            self.add_wire(
                                in_progress.from,
                                WireEndpoint::Junction {
                                    wire: host_wire,
                                    waypoint: host_waypoint,
                                },
                                in_progress.waypoints,
                            );
                        }
                    } else if let Some(pos) = click_pos {
                        let at = canvas::snap_to_grid(pos);
                        match &mut self.wiring_from {
                            Some(in_progress) => {
                                // Clicking the same grid point twice shouldn't
                                // stack two points there.
                                let last = in_progress.waypoints.last().copied();
                                if last != Some(at) && (last.is_some() || at != in_progress.anchor)
                                {
                                    in_progress.waypoints.push(at);
                                }
                            }
                            // With the wire tool, a click on **empty canvas**
                            // starts a wire there rather than doing nothing: it
                            // begins on a loose end, which can be dropped onto
                            // something later.
                            //
                            // `!click_consumed` is what makes it *empty*. A
                            // click that a component already answered is not a
                            // place to begin a wire — the wire would start at a
                            // loose point under the middle of a gate, which is
                            // not something anyone means. Its pins are the way
                            // in, and they are checked before this.
                            None if self.tool == Tool::Wire && !click_consumed => {
                                // Started on an existing wire: tap it, so the
                                // new wire is connected from its first click
                                // rather than merely beginning next to it.
                                let start = junction_start.take().map(|(host_net, target, at)| {
                                    let (host, waypoint) = match target {
                                        JunctionTarget::Existing { wire, waypoint } => {
                                            (wire, waypoint)
                                        }
                                        JunctionTarget::Insert {
                                            wire,
                                            waypoint,
                                            waypoints,
                                        } => {
                                            self.record_edit();
                                            if let Some(host) =
                                                self.wires.iter_mut().find(|w| w.id == wire)
                                            {
                                                host.waypoints = waypoints;
                                            }
                                            self.shift_junctions(wire, waypoint, 1);
                                            self.dedupe_waypoints(wire);
                                            (wire, waypoint)
                                        }
                                    };
                                    (
                                        WireEndpoint::Junction {
                                            wire: host,
                                            waypoint,
                                        },
                                        host_net,
                                        at,
                                    )
                                });
                                let (from, net, anchor) =
                                    start.unwrap_or((WireEndpoint::Free(at), None, at));
                                self.wiring_from = Some(WireInProgress {
                                    from,
                                    net,
                                    anchor,
                                    waypoints: Vec::new(),
                                });
                                click_consumed = true;
                            }
                            None => {}
                        }
                    }

                    // Enter ends a wire where the pointer is, leaving that end
                    // loose -- the counterpart to Escape, which throws the whole
                    // wire away. Without it a wire could only ever be finished on
                    // something, which defeats drawing one ahead of what it will
                    // connect to.
                    if self.wiring_from.is_some()
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        if let Some(in_progress) = self.wiring_from.take() {
                            let mut waypoints = in_progress.waypoints;
                            // The last point clicked *becomes* the end, rather
                            // than the wire running on to wherever the pointer
                            // happens to be: the rubber-band segment is a
                            // preview of a click not yet made, so Enter drops
                            // it. With nothing clicked at all there's only the
                            // start point, which is no wire.
                            if let Some(end) = waypoints.pop() {
                                self.record_edit();
                                self.add_wire(in_progress.from, WireEndpoint::Free(end), waypoints);
                            }
                        }
                    }

                    // Right-click is the common "let go of what I'm doing" gesture in
                    // most editors, so it backs out the same as Escape -- left-click
                    // can't double as either, since it's already how a waypoint gets
                    // added. One step at a time: a wire in progress is the innermost
                    // thing to back out of, so it goes first; only once there's no
                    // wire being drawn does the same gesture clear the selection.
                    if !consumed_secondary
                        && !ui.ctx().text_edit_focused()
                        && ui.ctx().input(|i| {
                            i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked()
                        })
                    {
                        if self.wiring_from.is_some() {
                            self.wiring_from = None;
                        } else {
                            self.selection.clear();
                            self.tool = Tool::Select;
                        }
                    }

                    // A click that hit nothing selectable is a click on empty
                    // canvas: clear the selection, the way every schematic/drawing
                    // editor does. Skipped while a wire is being drawn (that click
                    // was a waypoint) or a placement is queued (it's about to drop a
                    // component there).
                    if click_pos.is_some()
                        && !click_consumed
                        && self.wiring_from.is_none()
                        && self.tool == Tool::Select
                    {
                        self.selection.clear();
                    }

                    if let Some(in_progress) = &self.wiring_from {
                        // Scene coordinates, like every other point on the
                        // path: the raw pointer position is global, so using
                        // it directly would send the rubber-band line off to
                        // the wrong place as soon as the view is zoomed or
                        // panned away from 1:1.
                        let pointer_pos = pointer_scene.unwrap_or(in_progress.anchor);
                        let mut preview = vec![in_progress.anchor];
                        preview.extend(in_progress.waypoints.iter().copied());
                        preview.push(pointer_pos);
                        canvas::draw_path(
                            &painter,
                            &preview,
                            egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                        );
                        for &waypoint in &in_progress.waypoints {
                            painter.circle_filled(waypoint, 3.0, ui.visuals().strong_text_color());
                        }
                    }

                    self.draw_placement_ghost(ui, &painter, hover_pos);
                });

            self.scene_rect = scene_rect;

            // A drag that the scene's background saw is a drag on empty
            // canvas -- everything else would have claimed it first. Only the
            // primary button: the middle one is still panning.
            if self.bands_on_left_drag()
                && scene_response
                    .response
                    .drag_started_by(egui::PointerButton::Primary)
            {
                self.band_origin = scene_response.response.interact_pointer_pos();
            }

            // Placing goes through the scene's own background response, so a
            // click that landed on a component or a wire never also drops a
            // new component underneath it.
            if scene_response.response.clicked() {
                if let Some(pos) = scene_response.response.interact_pointer_pos() {
                    self.drop_placed(ui, pos);
                }
            }

            // Every edit above changed the drawing, never the nets: they're
            // recomputed here, once, from whatever the drawing now says.
            let fingerprint = self.connectivity_fingerprint();
            if fingerprint != self.net_fingerprint {
                self.net_fingerprint = fingerprint;
                self.rebuild_nets();
                self.advance_circuit(SETTLE_TICKS);
            }

            // Over the canvas as well as in the status bar: the bar is at
            // the far edge of the window while the eye is on the circuit,
            // and a click that appears to do nothing is exactly the moment
            // nobody is looking down there.
            if !self.running && self.change_pending() {
                canvas::draw_notice(
                    ui,
                    ui.clip_rect(),
                    crate::i18n::Strings::for_language(self.language).status_pending_short,
                    ui.visuals().warn_fg_color,
                );
            }

            self.zoom_by_wheel(wheel, zoom_pivot);
        });
    }
}
