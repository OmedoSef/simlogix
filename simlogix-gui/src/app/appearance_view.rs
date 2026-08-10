//! The appearance view: drawing a circuit's own symbol.
//!
//! Split out of `app.rs` because it is a self-contained half of the editor —
//! it shares the canvas, the camera and the pointer mapping with the
//! schematic, and nothing else. Its state lives on `SimLogixApp` alongside
//! everything else's, so this is a child module rather than a sibling: the
//! fields stay private to the rest of the crate.

use crate::appearance::Appearance;
use crate::canvas;
use crate::i18n::Strings;
use crate::placed_component::InstancePort;
use crate::toolbar;

use super::{
    SimLogixApp, SymbolSelection, APPEARANCE_PIN_HANDLE, DEFAULT_SHAPE_TEXT_SIZE,
    SHAPE_PICK_RADIUS, SYMBOL_CLIPBOARD_TAG,
};

impl SimLogixApp {
    /// The symbol the open circuit is currently showing: its own if it has
    /// been given one, and the generated box otherwise.
    ///
    /// Read through `active_circuit()` so the port order is the one
    /// `port_slots` produces everywhere else — the open circuit's entry in
    /// `circuits` is stale by construction, and a second ordering rule here
    /// is exactly how the symbol and the instance would come to disagree.
    pub(super) fn active_appearance(&self) -> (Vec<InstancePort>, Appearance) {
        let saved = self.active_circuit();
        let ports: Vec<InstancePort> = Self::port_slots(&saved)
            .into_iter()
            .map(|(_, port)| port)
            .collect();
        let appearance = self.circuits[self.active]
            .appearance
            .clone()
            .unwrap_or_else(|| Appearance::generated(&ports));
        (ports, appearance)
    }

    /// The appearance view: the open circuit's symbol, with a handle on
    /// every pin.
    ///
    /// The symbol is drawn on the origin because it has no position of its
    /// own — where it ends up on a canvas is the instance's business.
    pub(super) fn appearance_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
    ) {
        let (ports, mut appearance) = self.active_appearance();
        let center = egui::Pos2::ZERO;
        let names: Vec<&str> = ports.iter().map(|port| port.name.as_str()).collect();
        let pin_positions = appearance.draw(
            painter,
            center,
            canvas::Rotation::Deg0,
            ui.visuals().strong_text_color(),
            &names,
            &crate::symbol::TextLayer::for_ui(ui),
        );

        // Drawn here too, and not only on an instance: a checkbox whose
        // effect you can't see while ticking it is a checkbox you have to
        // guess at.
        if appearance.show_name {
            crate::symbol::TextLayer::for_ui(ui).text(
                appearance.name_anchor(center),
                egui::Align2::CENTER_BOTTOM,
                &self.circuits[self.active].name,
                10.0,
                ui.visuals().strong_text_color(),
            );
        }

        // A circuit with no ports has a symbol with no pins — worth saying,
        // since the box looks finished and simply can't connect to anything.
        if ports.is_empty() {
            let strings = Strings::for_language(self.language);
            crate::symbol::TextLayer::for_ui(ui).text(
                egui::pos2(center.x, appearance.rect(center).bottom() + 12.0),
                egui::Align2::CENTER_TOP,
                strings.appearance_no_ports,
                10.0,
                ui.visuals().weak_text_color(),
            );
        }

        let accent = canvas::accent_color(ui.visuals().dark_mode);

        // Everything picked is lit in the accent colour: a shape redrawn over
        // itself, a pin ringed. A symbol is line art, and a box around a
        // diagonal line says less than the line lit up.
        for &index in &self.symbol_selection.shapes {
            let path = appearance.shape_path(index, center);
            if path.len() > 1 {
                painter.line(path, egui::Stroke::new(3.0, accent));
            }
        }
        for &index in &self.symbol_selection.pins {
            if let Some(at) = pin_positions.get(index) {
                painter.circle_stroke(*at, 6.0, egui::Stroke::new(2.0, accent));
            }
        }

        let changed = self.appearance_edit(
            ui,
            painter,
            pointer,
            center,
            &pin_positions,
            &mut appearance,
            accent,
        );

        if changed {
            // The first edit is also what turns the generated box into a
            // symbol of this circuit's own — there is nothing else to store,
            // and nothing was lost, since it is the very box that was on
            // screen a frame ago.
            self.circuits[self.active].appearance = Some(appearance);
        }
    }

    /// Drawing, picking, moving and deleting what a symbol is made of.
    /// Reports whether the symbol changed.
    ///
    /// **No widget is allocated here.** Press and release are read straight
    /// off the pointer, because a full-canvas `ui.interact` would cover the
    /// `Scene`'s own background response — which is what panning goes
    /// through. That is exactly how the rubber band once broke placement and
    /// panning at once, and the fix then was the same: add no widget.
    #[allow(clippy::too_many_arguments)]
    fn appearance_edit(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
        center: egui::Pos2,
        pin_positions: &[egui::Pos2],
        appearance: &mut Appearance,
        accent: egui::Color32,
    ) -> bool {
        let (pressed, released, double_clicked, shift) = ui.ctx().input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
            )
        });
        // Snapped in symbol coordinates, which are centre-relative — the
        // same space the shapes themselves are stored in.
        let aimed = pointer.map(|at| {
            let snap = |v: f32| {
                (v / crate::appearance::SHAPE_SNAP).round() * crate::appearance::SHAPE_SNAP
            };
            (snap(at.x - center.x), snap(at.y - center.y))
        });
        let preview = egui::Stroke::new(1.6, accent.gamma_multiply(0.7));
        let world = |(x, y): (f32, f32)| egui::pos2(center.x + x, center.y + y);
        let typing = ui.ctx().text_edit_focused();
        let mut changed = false;

        // Escape backs out of a shape in progress first, then the selection —
        // one step at a time, as it does on the schematic.
        if !typing && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.drawing.take().is_none() {
                self.symbol_selection = SymbolSelection::default();
            }
            self.shape_drag = None;
        }

        changed |= self.appearance_keys(ui, appearance, typing);

        match self.shape_tool {
            toolbar::ShapeTool::Pan => {}
            toolbar::ShapeTool::Select => {
                changed |= self.appearance_select(
                    ui,
                    painter,
                    pointer,
                    center,
                    pin_positions,
                    appearance,
                    accent,
                    (pressed, released, shift),
                );
            }
            toolbar::ShapeTool::Text => {
                if let (Some(at), true) = (aimed, pressed) {
                    self.record_edit();
                    let strings = Strings::for_language(self.language);
                    appearance.shapes.push(crate::appearance::Shape::Text {
                        at,
                        align: crate::appearance::TextAlign::Center,
                        size: DEFAULT_SHAPE_TEXT_SIZE,
                        // Placed with something readable on it rather than
                        // empty: an empty label draws nothing, so it would
                        // land invisible and there would be no way to tell a
                        // missed click from a placed one.
                        text: strings.shape_text_default.to_string(),
                    });
                    self.symbol_selection = SymbolSelection {
                        shapes: vec![appearance.shapes.len() - 1],
                        pins: Vec::new(),
                    };
                    // Back to Select so the label just dropped can be typed
                    // and moved straight away, instead of the next click
                    // dropping a second one.
                    self.shape_tool = toolbar::ShapeTool::Select;
                    changed = true;
                }
            }
            toolbar::ShapeTool::Line => {
                // A double-click lands as a press as well, so whether this
                // click ends the line is settled before another point can be
                // added to it.
                let finish = double_clicked
                    || (!typing && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)));

                if let Some(at) = aimed {
                    if let Some(points) = &self.drawing {
                        let mut path: Vec<egui::Pos2> = points.iter().map(|&p| world(p)).collect();
                        path.push(world(at));
                        painter.line(path, preview);
                    }
                    if pressed && !finish {
                        self.drawing.get_or_insert_with(Vec::new).push(at);
                    }
                }

                if finish {
                    if let Some(points) = self.drawing.take() {
                        // One point is a click, not a line.
                        if points.len() > 1 {
                            self.record_edit();
                            appearance.shapes.push(crate::appearance::Shape::Polyline {
                                points,
                                closed: false,
                            });
                            changed = true;
                        }
                    }
                }
            }
            toolbar::ShapeTool::Arc => {
                // Click the two ends, then move to bulge it and click again.
                // The middle point is the one being chosen in that last step,
                // which is exactly what the shape stores.
                if let Some(at) = aimed {
                    match self.drawing.as_deref() {
                        Some([start]) => {
                            painter.line(vec![world(*start), world(at)], preview);
                        }
                        Some([start, end]) => {
                            let arc = crate::appearance::Shape::Arc {
                                start: *start,
                                mid: at,
                                end: *end,
                            };
                            appearance.shapes.push(arc);
                            let index = appearance.shapes.len() - 1;
                            let path = appearance.shape_path(index, center);
                            appearance.shapes.pop();
                            painter.line(path, preview);
                        }
                        _ => {}
                    }
                    if pressed {
                        let points = self.drawing.get_or_insert_with(Vec::new);
                        points.push(at);
                        if points.len() == 3 {
                            let (start, end, mid) = (points[0], points[1], points[2]);
                            self.drawing = None;
                            self.record_edit();
                            appearance.shapes.push(crate::appearance::Shape::Arc {
                                start,
                                mid,
                                end,
                            });
                            changed = true;
                        }
                    }
                }
            }
            toolbar::ShapeTool::Rect | toolbar::ShapeTool::Circle => {
                if pressed {
                    self.shape_drag = aimed;
                }
                if let (Some(from), Some(to)) = (self.shape_drag, aimed) {
                    let shape = if self.shape_tool == toolbar::ShapeTool::Rect {
                        crate::appearance::Shape::Polyline {
                            // A rectangle is a closed four-point polyline —
                            // no shape of its own, because it isn't one.
                            points: vec![from, (to.0, from.1), to, (from.0, to.1)],
                            closed: true,
                        }
                    } else {
                        crate::appearance::Shape::Circle {
                            center: from,
                            radius: (to.0 - from.0).hypot(to.1 - from.1),
                        }
                    };
                    if released {
                        self.shape_drag = None;
                        // A click that never moved is a miss, not an empty
                        // shape nobody can see or select.
                        if from != to {
                            self.record_edit();
                            appearance.shapes.push(shape);
                            changed = true;
                        }
                    } else {
                        appearance.shapes.push(shape);
                        let index = appearance.shapes.len() - 1;
                        let path = appearance.shape_path(index, center);
                        appearance.shapes.pop();
                        painter.line(path, preview);
                    }
                }
            }
        }
        changed
    }

    /// Picking, moving and sweeping a band — everything the Select tool does.
    #[allow(clippy::too_many_arguments)]
    fn appearance_select(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        pointer: Option<egui::Pos2>,
        center: egui::Pos2,
        pin_positions: &[egui::Pos2],
        appearance: &mut Appearance,
        accent: egui::Color32,
        (pressed, released, shift): (bool, bool, bool),
    ) -> bool {
        let mut changed = false;

        if let (Some(at), true) = (pointer, pressed) {
            // A pin wins a tie with a shape: its target is the smaller of
            // the two, so aiming at one is the more deliberate act.
            let pin = pin_positions
                .iter()
                .enumerate()
                .map(|(index, position)| (index, position.distance(at)))
                .filter(|(_, distance)| *distance <= APPEARANCE_PIN_HANDLE / 2.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index);
            let shape = pin.is_none().then(|| {
                (0..appearance.shapes.len())
                    .map(|index| (index, appearance.distance_to_shape(index, at, center)))
                    .filter(|(_, distance)| *distance <= SHAPE_PICK_RADIUS)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| index)
            });
            let shape = shape.flatten();

            changed |= self.drop_emptied_labels(appearance, shape);

            match (pin, shape) {
                (Some(index), _) if shift => {
                    SymbolSelection::toggle(&mut self.symbol_selection.pins, index)
                }
                (Some(index), _) => {
                    if !self.symbol_selection.pins.contains(&index) {
                        self.symbol_selection = SymbolSelection {
                            shapes: Vec::new(),
                            pins: vec![index],
                        };
                    }
                }
                (None, Some(index)) if shift => {
                    SymbolSelection::toggle(&mut self.symbol_selection.shapes, index)
                }
                (None, Some(index)) => {
                    if !self.symbol_selection.shapes.contains(&index) {
                        self.symbol_selection = SymbolSelection {
                            shapes: vec![index],
                            pins: Vec::new(),
                        };
                    }
                }
                // Nothing under the pointer: a press on empty canvas starts
                // a band, and only clears the selection if it stays a click.
                (None, None) => self.shape_band = Some(at),
            }
        }

        // Moving whatever is picked, by the pointer's position in *scene*
        // coordinates. Not by the raw pointer delta: that one is in screen
        // pixels, so a zoomed view moved things by the wrong amount and they
        // drifted away from the cursor. Requiring the pointer to be over the
        // canvas is also what stops a drag on the panel's size slider from
        // dragging the selection along with it.
        let held = ui.ctx().input(|i| i.pointer.primary_down());
        if let (Some(now), true, None) = (pointer, held, self.shape_band) {
            if !self.symbol_selection.is_empty() {
                let (from, moved_before) = *self.moving_shape.get_or_insert((now, false));
                let by = now - from;
                if by != egui::Vec2::ZERO {
                    // Snapshotted on the first movement rather than on the
                    // press: picking something up to look at it isn't an
                    // edit, and taking it here still catches the state before
                    // anything has moved, since the translation is next.
                    if !moved_before {
                        self.record_edit();
                    }
                    self.translate_selection(appearance, by);
                    self.moving_shape = Some((now, true));
                    changed = true;
                }
            }
        }

        if let (Some(origin), Some(now)) = (self.shape_band, pointer) {
            let rect = egui::Rect::from_two_pos(origin, now);
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, accent),
                egui::StrokeKind::Inside,
            );
        }

        if released {
            if let Some(origin) = self.shape_band.take() {
                let now = pointer.unwrap_or(origin);
                let rect = egui::Rect::from_two_pos(origin, now);
                if rect.width() > 1.0 || rect.height() > 1.0 {
                    // Anything the band touched at all, not only what it
                    // swallowed whole: a long line drawn across the symbol
                    // would otherwise be impossible to catch.
                    self.symbol_selection = SymbolSelection {
                        shapes: (0..appearance.shapes.len())
                            .filter(|&index| {
                                appearance
                                    .shape_path(index, center)
                                    .iter()
                                    .any(|point| rect.contains(*point))
                            })
                            .collect(),
                        pins: pin_positions
                            .iter()
                            .enumerate()
                            .filter(|(_, at)| rect.contains(**at))
                            .map(|(index, _)| index)
                            .collect(),
                    };
                } else if !shift {
                    // A press that never became a sweep is a click on empty
                    // canvas, which is how you deselect.
                    self.symbol_selection = SymbolSelection::default();
                }
            }
            if matches!(self.moving_shape, Some((_, true))) {
                self.snap_selection(appearance);
                changed = true;
            }
            self.moving_shape = None;
        }

        changed
    }

    /// Keyboard: nudging, deleting, and copy/paste of shapes.
    fn appearance_keys(
        &mut self,
        ui: &mut egui::Ui,
        appearance: &mut Appearance,
        typing: bool,
    ) -> bool {
        if typing {
            return false;
        }
        let mut changed = false;

        // Arrow keys move by one step of whatever the thing snaps to, so a
        // nudge lands where a drag would have — the point of having them is
        // to place something exactly, not to place it slightly off.
        let nudge = ui.ctx().input(|i| {
            let mut by = egui::Vec2::ZERO;
            for (key, step) in [
                (egui::Key::ArrowLeft, egui::vec2(-1.0, 0.0)),
                (egui::Key::ArrowRight, egui::vec2(1.0, 0.0)),
                (egui::Key::ArrowUp, egui::vec2(0.0, -1.0)),
                (egui::Key::ArrowDown, egui::vec2(0.0, 1.0)),
            ] {
                if i.key_pressed(key) {
                    by += step;
                }
            }
            by
        });
        if nudge != egui::Vec2::ZERO && !self.symbol_selection.is_empty() {
            self.record_edit();
            for &index in &self.symbol_selection.shapes {
                appearance.translate_shape(index, nudge * crate::appearance::SHAPE_SNAP);
            }
            for &index in &self.symbol_selection.pins {
                if let Some(pin) = appearance.pins.get_mut(index) {
                    let by = nudge * canvas::GRID_SPACING;
                    pin.at = (pin.at.0 + by.x, pin.at.1 + by.y);
                }
            }
            changed = true;
        }

        if ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && !self.symbol_selection.shapes.is_empty()
        {
            self.record_edit();
            // Removed from the back so the earlier indices stay valid.
            let mut doomed = self.symbol_selection.shapes.clone();
            doomed.sort_unstable();
            for index in doomed.into_iter().rev() {
                if index < appearance.shapes.len() {
                    appearance.shapes.remove(index);
                }
            }
            self.symbol_selection = SymbolSelection::default();
            changed = true;
        }

        // Only shapes travel. A pin belongs to a port, so there is nothing
        // for a copy of one to be a pin *of*.
        let (copy, paste) = ui.ctx().input(|i| {
            let mut copy = false;
            let mut paste = None;
            for event in &i.events {
                match event {
                    egui::Event::Copy | egui::Event::Cut => copy = true,
                    egui::Event::Paste(text) => paste = Some(text.clone()),
                    _ => {}
                }
            }
            (copy, paste)
        });
        if copy && !self.symbol_selection.shapes.is_empty() {
            let picked: Vec<&crate::appearance::Shape> = self
                .symbol_selection
                .shapes
                .iter()
                .filter_map(|&index| appearance.shapes.get(index))
                .collect();
            if let Ok(json) = serde_json::to_string(&picked) {
                ui.ctx().copy_text(format!("{SYMBOL_CLIPBOARD_TAG}{json}"));
            }
        }
        if let Some(text) = paste {
            if let Some(json) = text.strip_prefix(SYMBOL_CLIPBOARD_TAG) {
                if let Ok(shapes) = serde_json::from_str::<Vec<crate::appearance::Shape>>(json) {
                    if !shapes.is_empty() {
                        self.record_edit();
                        let first = appearance.shapes.len();
                        appearance.shapes.extend(shapes);
                        // Offset so the copy is visibly its own thing rather
                        // than hidden exactly beneath the original.
                        for index in first..appearance.shapes.len() {
                            appearance.translate_shape(
                                index,
                                egui::Vec2::splat(crate::appearance::SHAPE_SNAP * 2.0),
                            );
                        }
                        self.symbol_selection = SymbolSelection {
                            shapes: (first..appearance.shapes.len()).collect(),
                            pins: Vec::new(),
                        };
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    /// Moves everything picked by the same amount.
    fn translate_selection(&self, appearance: &mut Appearance, by: egui::Vec2) {
        for &index in &self.symbol_selection.shapes {
            appearance.translate_shape(index, by);
        }
        for &index in &self.symbol_selection.pins {
            if let Some(pin) = appearance.pins.get_mut(index) {
                pin.at = (pin.at.0 + by.x, pin.at.1 + by.y);
            }
        }
    }

    /// Puts everything picked back on its own step, once a drag has ended.
    ///
    /// A pin snaps to the *whole* grid and a shape to a quarter of it: a pin
    /// is what a wire attaches to and has to land on a dot.
    fn snap_selection(&self, appearance: &mut Appearance) {
        for &index in &self.symbol_selection.shapes {
            appearance.snap_shape(index);
        }
        for &index in &self.symbol_selection.pins {
            if let Some(pin) = appearance.pins.get_mut(index) {
                let snapped = canvas::snap_to_grid(egui::pos2(pin.at.0, pin.at.1));
                pin.at = (snapped.x, snapped.y);
            }
        }
    }

    /// Drops any selected label whose text was cleared, now that the click
    /// has moved on from it.
    ///
    /// An empty label draws nothing, so leaving it would put an invisible
    /// shape on the symbol that you'd have to remember the position of to
    /// ever select again — and moving on is the point at which "I didn't
    /// want it" is actually settled.
    fn drop_emptied_labels(&mut self, appearance: &mut Appearance, keeping: Option<usize>) -> bool {
        let emptied: Vec<usize> = self
            .symbol_selection
            .shapes
            .iter()
            .copied()
            .filter(|index| Some(*index) != keeping)
            .filter(|index| {
                matches!(
                    appearance.shapes.get(*index),
                    Some(crate::appearance::Shape::Text { text, .. }) if text.trim().is_empty()
                )
            })
            .collect();
        if emptied.is_empty() {
            return false;
        }
        let mut doomed = emptied;
        doomed.sort_unstable();
        for index in doomed.into_iter().rev() {
            appearance.shapes.remove(index);
        }
        // Indices below the removals have shifted; rather than repair them,
        // the selection is dropped — the click that got here is about to set
        // a new one anyway.
        self.symbol_selection = SymbolSelection::default();
        true
    }
}
