//! The SimLogix application: state and the `eframe::App` loop.

use std::collections::HashMap;

use simlogix_core::{
    And, Buffer, Button, Circuit, Clock, Component, ComponentId, Led, Nand, NetId, Nor, Not, Or,
    Pin, PinDirection, Probe, Rail, Signal, Transistor, Xnor, Xor,
};

use crate::canvas;
use crate::i18n::{Language, Strings};
use crate::palette::{self, ComponentKind};
use crate::placed_component::PlacedComponent;
use crate::project::{SavedCircuit, SavedComponent, SavedProject};

/// Logical ticks the circuit advances per real second — the resolution the
/// whole simulation runs at. A `Clock`'s period is expressed in these ticks
/// (see `CLOCK_PERIOD_TICKS`), so this constant is what ties "one clock
/// toggle" to a specific amount of real time.
const TICKS_PER_SECOND: f32 = 60.0;
/// A placed `Clock` toggles once every this many ticks — with
/// `TICKS_PER_SECOND` above, that's once per real second.
const CLOCK_PERIOD_TICKS: u64 = 60;
/// How far an interactive change (a press, a new wire, a deletion) advances
/// the circuit to settle. Deliberately small: unlike `Circuit::run` (which
/// would let a `Clock` anywhere in the circuit burn through a huge chunk of
/// its future ticks in one synchronous burst), this barely nudges real time,
/// while still being generous enough for a modest chain of gates to fully
/// propagate in one go.
pub(crate) const SETTLE_TICKS: u64 = 32;

/// One end of a [`Wire`] that isn't a component pin: a tap into another
/// wire's waypoint (a junction/contact point), created by finishing a new
/// wire on top of an existing one's waypoint instead of a pin. Tracked by
/// the host wire's id and which of its waypoints — resolved fresh every
/// frame (see `ui()`'s pre-pass) — so moving that point (even dragging the
/// host's own implicit default-route point, which materializes into a real
/// waypoint the moment it's first dragged) drags this tap along with it,
/// same as any other point on the host would. A wire can only tap into an
/// *earlier* one (you can only tap something that already exists), which is
/// what makes that pre-pass a single forward sweep instead of needing to
/// handle cycles.
#[derive(Clone, Copy, PartialEq)]
enum WireEndpoint {
    Pin(ComponentId, usize),
    Junction { wire: u64, waypoint: usize },
}

/// A single user-drawn wire: two endpoints (`from` is always a pin — every
/// wire is drawn starting from one — `to` may be a pin or a junction tap on
/// another wire) plus every waypoint between them, grid-snapped, in order.
/// Replaces inferring wire topology from net membership (a "star" from an
/// arbitrary anchor pin to every other pin sharing a net): that model had no
/// way to represent a wire ending on a point that isn't a pin at all, which
/// is exactly what a junction is.
struct Wire {
    id: u64,
    net: NetId,
    from: (ComponentId, usize),
    to: WireEndpoint,
    waypoints: Vec<egui::Pos2>,
}

/// A wire being placed click by click: the pin and net it started from, the
/// screen position of that pin, and every waypoint confirmed so far
/// (grid-snapped, in order) — the segment from the last of these to the
/// current pointer position is drawn as a live preview until the wire is
/// finished or cancelled.
struct WireInProgress {
    from: (ComponentId, usize),
    net: NetId,
    anchor: egui::Pos2,
    waypoints: Vec<egui::Pos2>,
}

/// Pick a kind from the palette, click the canvas to drop it (snapped to the
/// grid), then click one pin to start a wire, click the canvas as many times
/// as you like to lay down grid-snapped waypoints, and click a pin — or an
/// existing wire's waypoint, to tap into it as a junction — to finish it.
/// That merges their nets in `circuit` (see `Circuit::merge_nets`). Escape
/// cancels a wire in progress.
pub struct SimLogixApp {
    show_about: bool,
    circuit: Circuit,
    placed: Vec<PlacedComponent>,
    pending_placement: Option<ComponentKind>,
    selected: Option<ComponentId>,
    /// A wire currently being placed click by click, if one is in progress.
    wiring_from: Option<WireInProgress>,
    /// Every wire the user has drawn (or that was reconstructed on project
    /// load) — the source of truth for both rendering and editing.
    wires: Vec<Wire>,
    /// Monotonically increasing, so each `Wire` gets a stable id independent
    /// of its position in `wires` (which changes on deletion).
    next_wire_id: u64,
    /// The currently selected wire's id, if any.
    selected_wire: Option<u64>,
    /// The last save/load failure, if any, shown in a dismissible window.
    error: Option<String>,
    /// Fractional logical ticks owed to `circuit` from real elapsed time,
    /// carried between frames so `TICKS_PER_SECOND` isn't rounded away.
    tick_budget: f32,
    /// The UI's current language, overridable from the Settings menu.
    language: Language,
}

impl Default for SimLogixApp {
    fn default() -> Self {
        Self {
            show_about: false,
            circuit: Circuit::default(),
            placed: Vec::new(),
            pending_placement: None,
            selected: None,
            wiring_from: None,
            wires: Vec::new(),
            next_wire_id: 0,
            selected_wire: None,
            error: None,
            tick_budget: 0.0,
            // Everything else defaults trivially; only the language needs a
            // real default, detected once from the OS locale at startup.
            language: Language::detect_from_os(),
        }
    }
}

impl SimLogixApp {
    /// Registers a new component of `kind` in `circuit` and adds it to
    /// `placed` at `center`. Returns its id — used both for interactive
    /// placement and to rebuild a saved project (see `project.rs`).
    fn place(&mut self, kind: ComponentKind, center: egui::Pos2) -> ComponentId {
        let placed = match kind {
            ComponentKind::Button => {
                let net = self.circuit.add_net();
                let (button, pressed) = Button::new();
                let id = self.circuit.add_component(
                    Box::new(button),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::button(id, center, pressed)
            }
            ComponentKind::Led => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Led),
                    vec![Pin {
                        direction: PinDirection::Input,
                        net,
                    }],
                );
                PlacedComponent::led(id, center)
            }
            ComponentKind::NTransistor | ComponentKind::PTransistor => {
                let gate = self.circuit.add_net();
                let source = self.circuit.add_net();
                let drain = self.circuit.add_net();
                let transistor = if kind == ComponentKind::NTransistor {
                    Transistor::n_type()
                } else {
                    Transistor::p_type()
                };
                let id = self.circuit.add_component(
                    Box::new(transistor),
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: gate,
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: source,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: drain,
                        },
                    ],
                );
                PlacedComponent::transistor(id, center, kind)
            }
            ComponentKind::Ground | ComponentKind::Power => {
                let net = self.circuit.add_net();
                let rail = if kind == ComponentKind::Ground {
                    Rail::ground()
                } else {
                    Rail::power()
                };
                let id = self.circuit.add_component(
                    Box::new(rail),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                self.circuit.schedule_now(id);
                PlacedComponent::rail(id, center, kind)
            }
            ComponentKind::Probe => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Probe),
                    vec![Pin {
                        direction: PinDirection::Input,
                        net,
                    }],
                );
                PlacedComponent::probe(id, center)
            }
            ComponentKind::Clock => {
                let net = self.circuit.add_net();
                let id = self.circuit.add_component(
                    Box::new(Clock::new()),
                    vec![Pin {
                        direction: PinDirection::Output,
                        net,
                    }],
                );
                // Periodic, not schedule_now: a Clock must keep re-triggering
                // itself forever (see Circuit::schedule_periodic), which the
                // per-frame advance() call in `ui()` then processes over time.
                self.circuit.schedule_periodic(id, CLOCK_PERIOD_TICKS);
                PlacedComponent::clock(id, center)
            }
            // Every 2-input combinational gate shares this shape (see
            // `PlacedComponent::TwoInputGate`'s doc comment).
            ComponentKind::And
            | ComponentKind::Or
            | ComponentKind::Nand
            | ComponentKind::Nor
            | ComponentKind::Xor
            | ComponentKind::Xnor => {
                let a = self.circuit.add_net();
                let b = self.circuit.add_net();
                let out = self.circuit.add_net();
                let component: Box<dyn Component> = if kind == ComponentKind::And {
                    Box::new(And)
                } else if kind == ComponentKind::Or {
                    Box::new(Or)
                } else if kind == ComponentKind::Nand {
                    Box::new(Nand)
                } else if kind == ComponentKind::Nor {
                    Box::new(Nor)
                } else if kind == ComponentKind::Xor {
                    Box::new(Xor)
                } else {
                    Box::new(Xnor)
                };
                let id = self.circuit.add_component(
                    component,
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: a,
                        },
                        Pin {
                            direction: PinDirection::Input,
                            net: b,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: out,
                        },
                    ],
                );
                PlacedComponent::two_input_gate(id, center, kind)
            }
            // The 1-input mirror of the above (see
            // `PlacedComponent::OneInputGate`'s doc comment).
            ComponentKind::Not | ComponentKind::Buffer => {
                let input = self.circuit.add_net();
                let output = self.circuit.add_net();
                let component: Box<dyn Component> = if kind == ComponentKind::Not {
                    Box::new(Not)
                } else {
                    Box::new(Buffer)
                };
                let id = self.circuit.add_component(
                    component,
                    vec![
                        Pin {
                            direction: PinDirection::Input,
                            net: input,
                        },
                        Pin {
                            direction: PinDirection::Output,
                            net: output,
                        },
                    ],
                );
                PlacedComponent::one_input_gate(id, center, kind)
            }
        };
        // Process just this component's own initial schedule (if any) --
        // never circuit.run(), which would let a just-placed Clock's endless
        // self-reschedule burn through a huge chunk of its future ticks
        // instantly instead of over real time.
        let _ = self.circuit.advance(0);
        let id = placed.id();
        self.placed.push(placed);
        id
    }

    /// Registers a new `Wire` and returns its id.
    fn add_wire(
        &mut self,
        net: NetId,
        from: (ComponentId, usize),
        to: WireEndpoint,
        waypoints: Vec<egui::Pos2>,
    ) -> u64 {
        let id = self.next_wire_id;
        self.next_wire_id += 1;
        self.wires.push(Wire {
            id,
            net,
            from,
            to,
            waypoints,
        });
        id
    }

    /// Snapshots the current layout and wiring into a saveable project.
    /// Runtime state (button presses, signal values) is deliberately left out
    /// — see `project.rs`. Wire shape (waypoints, junctions) isn't saved
    /// either — only which pins share a net, same as before this file's
    /// `Wire` rewrite; a reloaded project's wires are just resynthesized as a
    /// plain star (see `from_project`).
    fn to_project(&self) -> SavedProject {
        let components = self
            .placed
            .iter()
            .map(|placed| {
                let center = placed.center();
                SavedComponent {
                    kind: placed.kind(),
                    x: center.x,
                    y: center.y,
                    rotation: placed.rotation(),
                }
            })
            .collect();

        // Group every pin (across all placed components) by the net it's on;
        // any group with 2+ members is a wire to remember.
        let mut groups: HashMap<NetId, Vec<(usize, usize)>> = HashMap::new();
        for (component_index, placed) in self.placed.iter().enumerate() {
            for (pin_index, pin) in self.circuit.pins(placed.id()).iter().enumerate() {
                groups
                    .entry(pin.net)
                    .or_default()
                    .push((component_index, pin_index));
            }
        }
        let wires = groups
            .into_values()
            .filter(|group| group.len() >= 2)
            .collect();

        SavedProject {
            version: crate::project::CURRENT_VERSION,
            circuits: vec![SavedCircuit {
                name: "main".to_string(),
                components,
                wires,
            }],
        }
    }

    /// Rebuilds a fresh app from a saved project, re-registering every
    /// component, re-merging every saved wire group, and resynthesizing a
    /// `Wire` (a plain star from the group's first pin to every other, no
    /// waypoints) for each group so the reloaded circuit still renders as
    /// connected. Only the first circuit is loaded — there's no
    /// multi-circuit editing yet.
    fn from_project(project: &SavedProject) -> Self {
        let mut app = Self::default();

        let Some(circuit) = project.circuits.first() else {
            return app;
        };

        let ids: Vec<ComponentId> = circuit
            .components
            .iter()
            .map(|saved| {
                let id = app.place(saved.kind, egui::pos2(saved.x, saved.y));
                if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == id) {
                    placed.set_rotation(saved.rotation);
                }
                id
            })
            .collect();

        for group in &circuit.wires {
            let endpoint_nets: Vec<NetId> = group
                .iter()
                .map(|&(component_index, pin_index)| {
                    app.circuit.pins(ids[component_index])[pin_index].net
                })
                .collect();
            if let Some((&first_net, rest)) = endpoint_nets.split_first() {
                for &net in rest {
                    app.circuit.merge_nets(net, first_net);
                }
            }
            if let Some((&(anchor_ci, anchor_pi), rest)) = group.split_first() {
                for &(ci, pi) in rest {
                    let net = app.circuit.pins(ids[ci])[pi].net;
                    app.add_wire(
                        net,
                        (ids[anchor_ci], anchor_pi),
                        WireEndpoint::Pin(ids[ci], pi),
                        Vec::new(),
                    );
                }
            }
        }
        let _ = app.circuit.advance(SETTLE_TICKS);

        app
    }

    fn save_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SimLogix project", &["simlogix"])
            .set_file_name("circuit.simlogix")
            .save_file()
        else {
            return;
        };

        let project = self.to_project();
        let result = serde_json::to_string_pretty(&project)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(&path, json).map_err(|err| err.to_string()));
        if let Err(message) = result {
            let strings = Strings::for_language(self.language);
            self.error = Some(strings.error_save_failed.replace("{}", &message));
        }
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SimLogix project", &["simlogix"])
            .pick_file()
        else {
            return;
        };

        let result = std::fs::read_to_string(&path)
            .map_err(|err| err.to_string())
            .and_then(|json| {
                serde_json::from_str::<SavedProject>(&json).map_err(|err| err.to_string())
            });
        match result {
            Ok(project) => {
                // Loading a project resets everything else, but the
                // language is a UI preference, not part of the circuit.
                let language = self.language;
                *self = Self::from_project(&project);
                self.language = language;
            }
            Err(message) => {
                let strings = Strings::for_language(self.language);
                self.error = Some(strings.error_open_failed.replace("{}", &message));
            }
        }
    }
}

impl eframe::App for SimLogixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Advance the circuit by real elapsed time every frame, not just
        // after an explicit interaction -- this is what lets a placed Clock
        // keep ticking on its own. Requesting continuous repaint is what
        // makes egui keep calling this at all without new input; the
        // tradeoff is constant redraw (and CPU use) instead of only on
        // interaction, even for a circuit with no Clock in it.
        self.tick_budget += ui.ctx().input(|i| i.stable_dt) * TICKS_PER_SECOND;
        let ticks_due = self.tick_budget.floor();
        if ticks_due > 0.0 {
            self.tick_budget -= ticks_due;
            let _ = self.circuit.advance(ticks_due as u64);
        }
        ui.ctx().request_repaint();

        let strings = Strings::for_language(self.language);

        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(strings.menu_file, |ui| {
                    if ui.button(strings.menu_file_new).clicked() {
                        let language = self.language;
                        *self = Self::default();
                        self.language = language;
                    }
                    if ui.button(strings.menu_file_open).clicked() {
                        self.open_project();
                    }
                    if ui.button(strings.menu_file_save).clicked() {
                        self.save_project();
                    }
                    if ui.button(strings.menu_file_quit).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(strings.menu_settings, |ui| {
                    ui.label(strings.menu_settings_theme);
                    // egui already defaults to ThemePreference::System (follows
                    // the OS) and tracks the current choice itself -- read it,
                    // let the built-in widget mutate the local copy, write it
                    // back. No SimLogixApp-level state needed for this.
                    let mut theme_preference = ui.ctx().options(|opt| opt.theme_preference);
                    theme_preference.radio_buttons(ui);
                    ui.ctx().set_theme(theme_preference);

                    ui.separator();

                    ui.label(strings.menu_settings_language);
                    ui.horizontal(|ui| {
                        for language in [Language::English, Language::French, Language::Italian] {
                            ui.selectable_value(&mut self.language, language, language.label());
                        }
                    });
                });
                ui.menu_button(strings.menu_help, |ui| {
                    if ui.button(strings.menu_help_about).clicked() {
                        self.show_about = true;
                    }
                });
            });
        });

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            let hint = if self.wiring_from.is_some() {
                Some(strings.hint_wiring.to_string())
            } else if let Some(kind) = self.pending_placement {
                let label = strings.component_kind_label(kind);
                Some(strings.palette_click_to_place.replace("{}", label))
            } else if self.selected_wire.is_some() {
                Some(strings.hint_delete_wire.to_string())
            } else if self.selected.is_some() {
                Some(strings.hint_rotate_delete_component.to_string())
            } else {
                None
            };
            ui.label(hint.unwrap_or_default());
        });

        egui::Panel::left("palette")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| {
                // A resizable panel only stays at the width the user drags it
                // to if its content actually fills that width — otherwise it
                // re-shrinks to fit content on the next layout (e.g. right
                // after collapsing a category). `ScrollArea` does that, and
                // also means a longer palette scrolls instead of shrinking
                // the whole window.
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if let Some(kind) = palette::show(ui, strings) {
                        self.pending_placement = Some(kind);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let canvas_rect = ui.available_rect_before_wrap();
            let canvas_response =
                ui.interact(canvas_rect, egui::Id::new("canvas"), egui::Sense::click());
            let painter = ui.painter_at(canvas_rect);
            canvas::draw_grid(&painter, canvas_rect);

            if let Some(selected) = self.selected {
                if ui.ctx().input(|i| i.key_pressed(egui::Key::R)) {
                    if let Some(placed) = self.placed.iter_mut().find(|p| p.id() == selected) {
                        placed.rotate();
                    }
                }
            }

            // Whether this frame's click landed on something selectable (a
            // component, a wire, a waypoint, a pin). A click that lands on
            // nothing is a click on empty canvas, which clears the selection
            // -- see the end of this closure.
            let mut click_consumed = false;

            let mut pin_handles = Vec::new();
            for placed in &mut self.placed {
                let frame =
                    placed.draw_and_interact(ui, &painter, &mut self.circuit, self.selected);
                if let Some(id) = frame.clicked {
                    // A component and a wire are never selected at once:
                    // Delete checks the wire first, so leaving a stale wire
                    // selected would delete that instead of the component
                    // just clicked.
                    self.selected = Some(id);
                    self.selected_wire = None;
                    click_consumed = true;
                }
                pin_handles.extend(frame.pins);
            }
            // A pin's current on-canvas position this frame, resolved by
            // identity -- every `Wire` endpoint that's a pin looks itself up
            // here rather than storing a position directly, so it tracks a
            // moved component automatically.
            let pin_position = |component: ComponentId, pin_index: usize| -> Option<egui::Pos2> {
                pin_handles
                    .iter()
                    .find(|h| h.component == component && h.pin_index == pin_index)
                    .map(|h| h.position)
            };

            let click_pos = ui.ctx().input(|i| {
                i.pointer
                    .primary_clicked()
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            });
            // Double-clicking along an existing wire inserts a new waypoint
            // right there, so a wire can be reshaped in more places than
            // just its existing points.
            let double_click_pos = ui.ctx().input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            });

            // Every wire's endpoints and (possibly-defaulted) waypoint list,
            // resolved once per frame in creation order: a junction can only
            // tap into an already-existing (so already-resolved) earlier
            // wire, so a single forward pass is always enough -- no need to
            // handle cycles or iterate to a fixed point.
            struct Resolved {
                from: egui::Pos2,
                to: egui::Pos2,
                waypoints: Vec<egui::Pos2>,
            }
            let mut resolved: HashMap<u64, Resolved> = HashMap::new();
            for wire in &self.wires {
                let Some(from_pos) = pin_position(wire.from.0, wire.from.1) else {
                    continue; // Stale: its component is gone.
                };
                let to_pos = match wire.to {
                    WireEndpoint::Pin(component, pin_index) => {
                        match pin_position(component, pin_index) {
                            Some(pos) => pos,
                            None => continue, // Stale: its component is gone.
                        }
                    }
                    WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    } => {
                        match resolved.get(&host).and_then(|r| r.waypoints.get(waypoint)) {
                            Some(&pos) => pos,
                            None => continue, // Host gone, or that point no longer exists.
                        }
                    }
                };
                // No user-placed route yet: the automatic single-bend Z
                // route, grid-aligned at the midpoint so a fresh connection
                // never starts off-grid -- expressed as two waypoints
                // forming that same vertical bend, so dragging either one
                // (below) seamlessly turns this default into a real route.
                let waypoints = if wire.waypoints.is_empty() {
                    let bend_x = canvas::snap_coord_to_grid((from_pos.x + to_pos.x) / 2.0);
                    vec![egui::pos2(bend_x, from_pos.y), egui::pos2(bend_x, to_pos.y)]
                } else {
                    wire.waypoints.clone()
                };
                resolved.insert(
                    wire.id,
                    Resolved {
                        from: from_pos,
                        to: to_pos,
                        waypoints,
                    },
                );
            }

            // Finishing a new wire on top of another wire's waypoint (a
            // junction tap) is decided inside the loop below but applied
            // after it, to keep `self.wires` stable (an unchanging length,
            // no reallocation) for the whole iteration.
            let mut junction_finish: Option<(NetId, u64, usize)> = None;

            for i in 0..self.wires.len() {
                let wire_id = self.wires[i].id;
                let net = self.wires[i].net;
                let Some(Resolved {
                    from: from_pos,
                    to: to_pos,
                    waypoints,
                }) = resolved.remove(&wire_id)
                else {
                    continue; // Stale, already skipped above.
                };

                let color = if self.circuit.signal_at(net) == Signal::High {
                    egui::Color32::from_rgb(220, 30, 30)
                } else {
                    egui::Color32::from_gray(120)
                };
                let is_selected_wire = self.selected_wire == Some(wire_id);
                let stroke = if is_selected_wire {
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(90, 160, 255))
                } else {
                    egui::Stroke::new(2.0, color)
                };

                let mut path = vec![from_pos];
                path.extend(waypoints.iter().copied());
                path.push(to_pos);
                canvas::draw_path(&painter, &path, stroke);
                for &point in &waypoints {
                    painter.circle_filled(point, 3.5, stroke.color);
                }

                // Only select an existing wire by clicking on it, or reshape
                // it by dragging a waypoint, while not actively placing a
                // new one -- otherwise a click meant to add a waypoint to
                // the new wire would hijack this one instead.
                if self.wiring_from.is_none() {
                    if let Some(click_pos) = click_pos {
                        if canvas::distance_to_path(click_pos, &path) < 6.0 {
                            self.selected_wire = Some(wire_id);
                            self.selected = None;
                            click_consumed = true;
                        }
                    }

                    if let Some(dbl_pos) = double_click_pos {
                        if let Some((segment, distance)) = canvas::closest_segment(&path, dbl_pos) {
                            if distance < 6.0 {
                                if self.wires[i].waypoints.is_empty() {
                                    self.wires[i].waypoints = waypoints.clone();
                                }
                                self.wires[i]
                                    .waypoints
                                    .insert(segment, canvas::snap_to_grid(dbl_pos));
                                self.selected_wire = Some(wire_id);
                                self.selected = None;
                                click_consumed = true;
                            }
                        }
                    }
                }

                for (waypoint_index, &point) in waypoints.iter().enumerate() {
                    let handle_rect = egui::Rect::from_center_size(point, egui::vec2(10.0, 10.0));
                    let response = ui.interact(
                        handle_rect,
                        egui::Id::new(("wire_point", wire_id, waypoint_index)),
                        egui::Sense::click_and_drag(),
                    );

                    if let Some(in_progress) = &self.wiring_from {
                        // A wire is being drawn: clicking another wire's
                        // waypoint taps into it as a junction, finishing the
                        // new wire here instead of on a pin.
                        if response.clicked() && net != in_progress.net {
                            junction_finish = Some((net, wire_id, waypoint_index));
                        }
                    } else {
                        if response.dragged() {
                            if self.wires[i].waypoints.is_empty() {
                                self.wires[i].waypoints = waypoints.clone();
                            }
                            self.wires[i].waypoints[waypoint_index] += response.drag_delta();
                        }
                        if response.drag_stopped() {
                            if let Some(p) = self.wires[i].waypoints.get_mut(waypoint_index) {
                                *p = canvas::snap_to_grid(*p);
                            }
                        }
                        if response.clicked() {
                            self.selected_wire = Some(wire_id);
                            self.selected = None;
                            click_consumed = true;
                        }
                    }
                }
            }

            let delete_pressed = ui
                .ctx()
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
            if delete_pressed {
                if let Some(wire_id) = self.selected_wire {
                    // Deleting a wire that other wires are junction-tapped
                    // onto would otherwise orphan them (their host id no
                    // longer resolves, so they'd silently stop rendering and
                    // become impossible to select) -- cascade the delete to
                    // them too, and to anything tapped onto *those*, and so
                    // on.
                    let mut to_remove = vec![wire_id];
                    let mut i = 0;
                    while i < to_remove.len() {
                        let host = to_remove[i];
                        for wire in &self.wires {
                            if let WireEndpoint::Junction { wire: w, .. } = wire.to {
                                if w == host && !to_remove.contains(&wire.id) {
                                    to_remove.push(wire.id);
                                }
                            }
                        }
                        i += 1;
                    }
                    for id in to_remove {
                        if let Some(pos) = self.wires.iter().position(|w| w.id == id) {
                            let wire = self.wires.remove(pos);
                            // Disconnect whichever endpoint is a real pin
                            // that's uniquely this wire's own (a junction has
                            // none, so fall back to `from`, which every wire
                            // has).
                            let (component, pin_index) = match wire.to {
                                WireEndpoint::Pin(component, pin_index) => (component, pin_index),
                                WireEndpoint::Junction { .. } => wire.from,
                            };
                            self.circuit.disconnect_pin(component, pin_index);
                            let _ = self.circuit.advance(SETTLE_TICKS);
                        }
                    }
                    self.selected_wire = None;
                } else if let Some(selected) = self.selected {
                    self.circuit.remove_component(selected);
                    self.placed.retain(|placed| placed.id() != selected);

                    // Same cascade as deleting a wire directly (see above):
                    // removing every wire touching this component would
                    // otherwise orphan anything junction-tapped onto them.
                    let mut to_remove: Vec<u64> = self
                        .wires
                        .iter()
                        .filter(|w| {
                            w.from.0 == selected
                                || matches!(w.to, WireEndpoint::Pin(c, _) if c == selected)
                        })
                        .map(|w| w.id)
                        .collect();
                    let mut i = 0;
                    while i < to_remove.len() {
                        let host = to_remove[i];
                        for wire in &self.wires {
                            if let WireEndpoint::Junction { wire: w, .. } = wire.to {
                                if w == host && !to_remove.contains(&wire.id) {
                                    to_remove.push(wire.id);
                                }
                            }
                        }
                        i += 1;
                    }
                    self.wires.retain(|w| !to_remove.contains(&w.id));
                    self.selected = None;
                }
            }

            // A wire being placed click by click: clicking a pin starts one
            // (or finishes it, if one's already in progress and this pin is
            // on a different net); clicking empty canvas along the way adds
            // a grid-snapped waypoint; Escape cancels it.
            let clicked_pin = pin_handles
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
                    if net != in_progress.net {
                        self.circuit.merge_nets(in_progress.net, net);
                        let _ = self.circuit.advance(SETTLE_TICKS);
                        self.add_wire(
                            net,
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
                        from: (component, pin_index),
                        net,
                        anchor: position,
                        waypoints: Vec::new(),
                    });
                }
            } else if let Some((target_net, host_wire, host_waypoint)) = junction_finish {
                if let Some(in_progress) = self.wiring_from.take() {
                    self.circuit.merge_nets(in_progress.net, target_net);
                    let _ = self.circuit.advance(SETTLE_TICKS);
                    self.add_wire(
                        target_net,
                        in_progress.from,
                        WireEndpoint::Junction {
                            wire: host_wire,
                            waypoint: host_waypoint,
                        },
                        in_progress.waypoints,
                    );
                }
            } else if let Some(pos) = click_pos {
                if let Some(in_progress) = &mut self.wiring_from {
                    in_progress.waypoints.push(canvas::snap_to_grid(pos));
                }
            }

            // Right-click is the common "let go of what I'm doing" gesture in
            // most editors, so it backs out the same as Escape -- left-click
            // can't double as either, since it's already how a waypoint gets
            // added. One step at a time: a wire in progress is the innermost
            // thing to back out of, so it goes first; only once there's no
            // wire being drawn does the same gesture clear the selection.
            if ui
                .ctx()
                .input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked())
            {
                if self.wiring_from.is_some() {
                    self.wiring_from = None;
                } else {
                    self.selected = None;
                    self.selected_wire = None;
                    self.pending_placement = None;
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
                && self.pending_placement.is_none()
            {
                self.selected = None;
                self.selected_wire = None;
            }

            if let Some(in_progress) = &self.wiring_from {
                let pointer_pos = ui.ctx().pointer_latest_pos().unwrap_or(in_progress.anchor);
                let mut preview = vec![in_progress.anchor];
                preview.extend(in_progress.waypoints.iter().copied());
                preview.push(pointer_pos);
                canvas::draw_path(
                    &painter,
                    &preview,
                    egui::Stroke::new(2.0, egui::Color32::from_gray(200)),
                );
                for &waypoint in &in_progress.waypoints {
                    painter.circle_filled(waypoint, 3.0, egui::Color32::from_gray(200));
                }
            }

            if let Some(kind) = self.pending_placement {
                if let Some(click_pos) = canvas_response.interact_pointer_pos() {
                    self.place(kind, canvas::snap_to_grid(click_pos));
                    self.pending_placement = None;
                }
            }
        });

        egui::Window::new(strings.about_title)
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(strings.about_body);
                ui.label(
                    strings
                        .about_version
                        .replace("{}", env!("CARGO_PKG_VERSION")),
                );
            });

        let mut error_open = self.error.is_some();
        if let Some(message) = &self.error {
            egui::Window::new(strings.error_title)
                .open(&mut error_open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(message);
                });
        }
        if !error_open {
            self.error = None;
        }
    }
}
