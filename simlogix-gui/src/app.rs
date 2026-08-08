//! The SimLogix application: state and the `eframe::App` loop.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use simlogix_core::{
    And, Buffer, Button, Circuit, Clock, Component, ComponentId, Led, Nand, NetId, Nor, Not, Or,
    Pin, PinDirection, Probe, Rail, Transistor, Xnor, Xor,
};

use crate::canvas::{self, BOX_SIZE};
use crate::i18n::{Language, Strings};
use crate::palette::{self, ComponentKind};
use crate::placed_component::PlacedComponent;
use crate::project::{SavedCircuit, SavedComponent, SavedEndpoint, SavedProject, SavedWire};
use crate::toolbar::{self, Tool};

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
/// How many undo steps are kept. Each one is a whole `SavedProject`, so this
/// bounds memory rather than letting a long session grow without limit.
const MAX_UNDO_STEPS: usize = 64;
/// Zoom limits, as scene-to-screen scale. Bounded in both directions so the
/// circuit can't be shrunk to a dot or blown up past the point where a grid
/// step fills the window.
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
/// How much one notch of the wheel zooms. Applied as `exp(-scroll * this)`,
/// so zooming compounds evenly instead of accelerating as you go in.
const WHEEL_ZOOM_SENSITIVITY: f32 = 0.0015;
/// How close a click has to land to a wire to count as hitting it.
const WIRE_HIT_RADIUS: f32 = 6.0;
/// How close a dropped loose end has to land to a pin or another wire's
/// point to re-attach there instead of staying loose. Wider than
/// [`WIRE_HIT_RADIUS`]: this is a deliberate drop, so it should forgive a
/// few pixels.
const REATTACH_RADIUS: f32 = 12.0;

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
    Junction {
        wire: u64,
        waypoint: usize,
    },
    /// A loose end, left where a junction used to be after its host wire (or
    /// the contact point itself) was deleted. The wire stays visible and
    /// editable instead of vanishing — deleting one thing shouldn't quietly
    /// take unrelated wiring with it.
    Free(egui::Pos2),
}

/// A single user-drawn wire: two endpoints — either of which may be a pin, a
/// tap onto another wire, or a loose point — plus every waypoint between
/// them, grid-snapped, in order. Both ends are the same kind of thing so a
/// wire outlives the component it was drawn to, and so splitting one can
/// leave a piece that begins nowhere in particular.
/// Replaces inferring wire topology from net membership (a "star" from an
/// arbitrary anchor pin to every other pin sharing a net): that model had no
/// way to represent a wire ending on a point that isn't a pin at all, which
/// is exactly what a junction is.
struct Wire {
    id: u64,
    from: WireEndpoint,
    to: WireEndpoint,
    waypoints: Vec<egui::Pos2>,
}

/// A wire being placed click by click: the pin and net it started from, the
/// screen position of that pin, and every waypoint confirmed so far
/// (grid-snapped, in order) — the segment from the last of these to the
/// current pointer position is drawn as a live preview until the wire is
/// finished or cancelled.
struct WireInProgress {
    from: WireEndpoint,
    /// The net the start already sits on, if it started on something. A wire
    /// begun on empty canvas has none until one of its ends is connected.
    net: Option<NetId>,
    anchor: egui::Pos2,
    waypoints: Vec<egui::Pos2>,
}

/// Where a wire being drawn is about to tap into an existing one: either a
/// contact point that's already there, or one to create at that spot.
///
/// Deciding this inside the render loop but applying it afterwards keeps
/// `self.wires` untouched (stable length, no reallocation) for the whole
/// iteration — and lets the snapshot for undo be taken before the tap
/// modifies anything.
enum JunctionTarget {
    Existing {
        wire: u64,
        waypoint: usize,
    },
    Insert {
        wire: u64,
        waypoint: usize,
        /// The host's full waypoint list with the new contact point already
        /// in it, ready to be swapped in wholesale.
        waypoints: Vec<egui::Pos2>,
    },
}

/// A destructive action held back by the unsaved-changes confirmation until
/// the user says what to do about the current circuit.
#[derive(Clone, Copy, PartialEq)]
enum PendingAction {
    New,
    Open,
    Quit,
}

/// Pick a kind from the palette, click the canvas to drop it (snapped to the
/// grid), then click one pin to start a wire, click the canvas as many times
/// as you like to lay down grid-snapped waypoints, and click a pin — or an
/// existing wire's waypoint, to tap into it as a junction — to finish it.
/// The wires are the record of what's connected; the nets in `circuit` are
/// recomputed from them after every edit (see `rebuild_nets`). Escape
/// cancels a wire in progress.
pub struct SimLogixApp {
    show_about: bool,
    circuit: Circuit,
    placed: Vec<PlacedComponent>,
    /// What the next canvas click does — placing, wiring, or selecting.
    tool: Tool,
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
    /// Where this circuit was last saved to or loaded from. `None` means it
    /// has never been written anywhere, so "Save" has to ask for a path the
    /// same as "Save As".
    current_path: Option<std::path::PathBuf>,
    /// Whether the circuit has edits since the last save — drives both the
    /// `*` in the window title and the confirmation before anything that
    /// would discard them.
    dirty: bool,
    /// A destructive action waiting on that confirmation, if one is open.
    pending_action: Option<PendingAction>,
    /// Document states to step back to, oldest first — see
    /// [`SimLogixApp::record_edit`]. Snapshots are `SavedProject`s, the same
    /// thing save/load uses, so undo can't drift out of sync with what a
    /// circuit is made of.
    undo_stack: Vec<SavedProject>,
    /// States undone away from, ready to step forward into. Cleared by any
    /// fresh edit, since that makes the old forward path meaningless.
    redo_stack: Vec<SavedProject>,
    /// The last title pushed to the window manager, so the viewport command
    /// is only sent when it actually changes rather than every frame.
    window_title: String,
    /// The last save/load failure, if any, shown in a dismissible window.
    error: Option<String>,
    /// Fractional logical ticks owed to `circuit` from real elapsed time,
    /// carried between frames so `TICKS_PER_SECOND` isn't rounded away.
    tick_budget: f32,
    /// The UI's current language, overridable from the Settings menu.
    language: Language,
    /// A component that has just been placed or dropped, waiting to be
    /// checked against loose wire ends. Deliberately handled a frame late:
    /// pin positions are computed while drawing, so on the frame a drag ends
    /// they still describe where the component was before it snapped.
    pending_attach: Option<ComponentId>,
    /// Whether the simulation is advancing. Editing still works while
    /// paused — only time stops.
    running: bool,
    /// The net that refused to settle, if the engine reported one. Set only
    /// alongside `running = false`: a fault pauses rather than crashing or
    /// silently looping, and stays on screen until the user resumes.
    unstable_net: Option<NetId>,
    /// A hash of what is connected to what, so the nets are only recomputed
    /// when the drawing's *connectivity* actually changed — not on every
    /// frame, and not while a waypoint is merely being dragged.
    net_fingerprint: u64,
    /// The region of the circuit currently framed by the canvas, in scene
    /// coordinates — the whole of the zoom/pan state. `egui::Scene` derives
    /// its transform from this and writes back whatever the user pans or
    /// zooms to. Everything else (component centres, waypoints) is stored
    /// in scene coordinates too, so nothing else has to know about zoom.
    scene_rect: egui::Rect,
}

impl Default for SimLogixApp {
    fn default() -> Self {
        Self {
            show_about: false,
            circuit: Circuit::default(),
            placed: Vec::new(),
            tool: Tool::default(),
            selected: None,
            wiring_from: None,
            wires: Vec::new(),
            next_wire_id: 0,
            selected_wire: None,
            current_path: None,
            dirty: false,
            pending_action: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            window_title: String::new(),
            error: None,
            tick_budget: 0.0,
            // Everything else defaults trivially; only the language needs a
            // real default, detected once from the OS locale at startup.
            language: Language::detect_from_os(),
            pending_attach: None,
            net_fingerprint: 0,
            running: true,
            unstable_net: None,
            // A plain starting window onto the circuit; `Scene` re-fits this
            // if it ever ends up degenerate.
            scene_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0)),
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
        from: WireEndpoint,
        to: WireEndpoint,
        waypoints: Vec<egui::Pos2>,
    ) -> u64 {
        let id = self.next_wire_id;
        self.next_wire_id += 1;
        self.wires.push(Wire {
            id,
            from,
            to,
            waypoints,
        });
        id
    }

    /// The net a wire currently carries, read back from its endpoints rather
    /// than cached on the wire. `rebuild_nets` reallocates every net from
    /// scratch after each edit, so any stored copy would go stale as soon as
    /// something elsewhere in the drawing changed — which is exactly how
    /// wires ended up drawn in the wrong signal colour before.
    ///
    /// Either end may be the one that reaches a pin (a wire can be left
    /// dangling at the other), so both are tried. `None` means neither does:
    /// the wire is pure drawing at the moment and carries nothing.
    fn wire_net(&self, wire: &Wire) -> Option<NetId> {
        self.endpoint_net(wire.from, &mut Vec::new())
            .or_else(|| self.endpoint_net(wire.to, &mut Vec::new()))
    }

    /// The net behind a single endpoint, following a junction to its host.
    /// `visited` guards against a tap cycle, which nothing creates on
    /// purpose but which mustn't be able to hang the UI either.
    fn endpoint_net(&self, endpoint: WireEndpoint, visited: &mut Vec<u64>) -> Option<NetId> {
        match endpoint {
            WireEndpoint::Pin(component, pin_index) => self
                .circuit
                .try_pins(component)
                .and_then(|pins| pins.get(pin_index))
                .map(|pin| pin.net),
            WireEndpoint::Junction { wire, .. } => {
                if visited.contains(&wire) {
                    return None;
                }
                visited.push(wire);
                let host = self.wires.iter().find(|w| w.id == wire)?;
                self.endpoint_net(host.from, visited)
                    .or_else(|| self.endpoint_net(host.to, visited))
            }
            WireEndpoint::Free(_) => None,
        }
    }

    /// Snapshots the current layout and wiring into a saveable project —
    /// both what gets written to disk and what an undo step is made of.
    /// Runtime state (button presses, signal values) is deliberately left
    /// out; see `project.rs`.
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

        // Wires reference components and each other by index in the saved
        // lists, not by runtime id — ids aren't stable across a reload.
        let component_index: HashMap<ComponentId, usize> = self
            .placed
            .iter()
            .enumerate()
            .map(|(index, placed)| (placed.id(), index))
            .collect();
        let wire_index: HashMap<u64, usize> = self
            .wires
            .iter()
            .enumerate()
            .map(|(index, wire)| (wire.id, index))
            .collect();

        let wires = self
            .wires
            .iter()
            .filter_map(|wire| {
                let save = |endpoint: WireEndpoint| match endpoint {
                    WireEndpoint::Pin(component, pin) => {
                        Some(SavedEndpoint::Pin(*component_index.get(&component)?, pin))
                    }
                    WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    } => Some(SavedEndpoint::Junction {
                        wire: *wire_index.get(&host)?,
                        waypoint,
                    }),
                    WireEndpoint::Free(pos) => Some(SavedEndpoint::Free(pos.x, pos.y)),
                };
                Some(SavedWire {
                    from: save(wire.from)?,
                    to: save(wire.to)?,
                    waypoints: wire.waypoints.iter().map(|p| (p.x, p.y)).collect(),
                })
            })
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

    /// Rebuilds a fresh app from a saved project: re-registers every
    /// component, then replays each saved wire — merging the nets it joins
    /// and restoring its route. Only the first circuit is loaded — there's
    /// no multi-circuit editing yet.
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

        // Saved wire index -> runtime id, so a junction can resolve the wire
        // it taps. Saved order is preserved and a junction only ever refers
        // to an earlier wire, so this fills in before it's needed.
        let mut wire_ids: Vec<u64> = Vec::with_capacity(circuit.wires.len());
        for saved in &circuit.wires {
            let load = |endpoint: &SavedEndpoint| match *endpoint {
                SavedEndpoint::Pin(component_index, pin_index) => ids
                    .get(component_index)
                    .map(|&id| WireEndpoint::Pin(id, pin_index)),
                SavedEndpoint::Junction { wire, waypoint } => {
                    wire_ids.get(wire).map(|&host| WireEndpoint::Junction {
                        wire: host,
                        waypoint,
                    })
                }
                SavedEndpoint::Free(x, y) => Some(WireEndpoint::Free(egui::pos2(x, y))),
            };
            let (Some(from), Some(to)) = (load(&saved.from), load(&saved.to)) else {
                continue;
            };

            let waypoints = saved
                .waypoints
                .iter()
                .map(|&(x, y)| egui::pos2(x, y))
                .collect();
            wire_ids.push(app.add_wire(from, to, waypoints));
        }

        // The wires are the record of what's connected; the nets come from
        // them, exactly as they do after any edit.
        app.rebuild_nets();
        app.net_fingerprint = app.connectivity_fingerprint();
        let _ = app.circuit.advance(SETTLE_TICKS);

        app
    }

    /// Saves to the current path if there is one, otherwise falls back to
    /// [`Self::save_project_as`]. Returns whether the circuit actually made
    /// it to disk — the confirmation flow needs to know, so that cancelling
    /// the dialog doesn't silently go on to discard the work anyway.
    fn save_project(&mut self) -> bool {
        match self.current_path.clone() {
            Some(path) => self.write_project_to(&path),
            None => self.save_project_as(),
        }
    }

    fn save_project_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SimLogix project", &["simlogix"])
            .set_file_name("circuit.simlogix")
            .save_file()
        else {
            return false;
        };
        self.write_project_to(&path)
    }

    fn write_project_to(&mut self, path: &std::path::Path) -> bool {
        let project = self.to_project();
        let result = serde_json::to_string_pretty(&project)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(path, json).map_err(|err| err.to_string()));
        match result {
            Ok(()) => {
                self.current_path = Some(path.to_path_buf());
                self.dirty = false;
                true
            }
            Err(message) => {
                let strings = Strings::for_language(self.language);
                self.error = Some(strings.error_save_failed.replace("{}", &message));
                false
            }
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
            .and_then(|json| SavedProject::from_json(&json));
        match result {
            Ok(project) => {
                // Loading a project resets everything else, but the
                // language is a UI preference, not part of the circuit.
                let language = self.language;
                *self = Self::from_project(&project);
                self.language = language;
                self.current_path = Some(path);
            }
            Err(message) => {
                let strings = Strings::for_language(self.language);
                self.error = Some(strings.error_open_failed.replace("{}", &message));
            }
        }
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
    fn remove_wires(&mut self, roots: Vec<u64>, resolved: &HashMap<u64, Vec<egui::Pos2>>) {
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
    fn shift_junctions(&mut self, host: u64, from: usize, delta: isize) {
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
    fn dedupe_waypoints(&mut self, wire_id: u64) {
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
    fn remove_waypoint(&mut self, wire_id: u64, index: usize, resolved: &[egui::Pos2]) {
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

    /// Starts or stops the simulation. Resuming also clears a fault: the
    /// user has presumably just fixed the circuit, and if they haven't, the
    /// very next tick reports it again.
    fn toggle_running(&mut self) {
        self.running = !self.running;
        if self.running {
            self.unstable_net = None;
        }
    }

    /// Advances the simulation, unless it's paused or already stopped on a
    /// fault. An engine that reports it can't settle pauses the simulation
    /// and surfaces the offending net in the status bar — the alternative
    /// would be looping forever or taking the whole program down, and the
    /// circuit is still there to be inspected and fixed either way.
    fn advance_circuit(&mut self, ticks: u64) {
        if !self.running || self.unstable_net.is_some() {
            return;
        }
        if let Err(unstable) = self.circuit.advance(ticks) {
            self.unstable_net = Some(unstable.net);
            self.running = false;
        }
    }

    /// Joins a wire's loose end to any other loose end sitting at the very
    /// same place.
    ///
    /// A cut can leave two ends stacked: when one wire tapped the cut point
    /// from *both* sides, joining it onto the first piece also carries its
    /// other end over, which then lands on the second piece's end. That
    /// second one has nothing left to join against by name — only by
    /// position, which is what this does.
    fn join_touching_loose_end(&mut self, wire_id: u64, is_from: bool) {
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
    fn split_wire(&mut self, wire_id: u64, cut: usize, path: &[egui::Pos2]) {
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
    fn reverse_wire(&mut self, wire_id: u64) {
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
    fn join_wires(
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
    fn rebuild_nets(&mut self) {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Node {
            Pin(ComponentId, usize),
            Wire(u64),
        }

        let mut parent: HashMap<Node, Node> = HashMap::new();
        fn find(parent: &mut HashMap<Node, Node>, node: Node) -> Node {
            let mut root = node;
            while let Some(&next) = parent.get(&root) {
                if next == root {
                    break;
                }
                root = next;
            }
            // Path compression, so a long chain of taps stays cheap.
            let mut walk = node;
            while let Some(&next) = parent.get(&walk) {
                if next == root {
                    break;
                }
                parent.insert(walk, root);
                walk = next;
            }
            parent.entry(node).or_insert(root);
            root
        }
        fn union(parent: &mut HashMap<Node, Node>, a: Node, b: Node) {
            let (a, b) = (find(parent, a), find(parent, b));
            if a != b {
                parent.insert(a, b);
            }
        }

        for placed in &self.placed {
            let pin_count = self
                .circuit
                .try_pins(placed.id())
                .map(|pins| pins.len())
                .unwrap_or(0);
            for index in 0..pin_count {
                let node = Node::Pin(placed.id(), index);
                parent.entry(node).or_insert(node);
            }
        }

        for wire in &self.wires {
            let self_node = Node::Wire(wire.id);
            parent.entry(self_node).or_insert(self_node);
            for end in [wire.from, wire.to] {
                match end {
                    WireEndpoint::Pin(component, index) => {
                        union(&mut parent, self_node, Node::Pin(component, index));
                    }
                    WireEndpoint::Junction { wire: host, .. } => {
                        union(&mut parent, self_node, Node::Wire(host));
                    }
                    // A loose end connects nothing, so it contributes no
                    // union at all.
                    WireEndpoint::Free(_) => {}
                }
            }
        }

        let mut groups: HashMap<Node, Vec<(ComponentId, usize)>> = HashMap::new();
        let nodes: Vec<Node> = parent.keys().copied().collect();
        for node in nodes {
            if let Node::Pin(component, index) = node {
                let root = find(&mut parent, node);
                groups.entry(root).or_default().push((component, index));
            }
        }

        // A lone pin is its own net anyway, which `rewire` already does for
        // anything it isn't told about.
        let groups: Vec<Vec<(ComponentId, usize)>> = groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();
        self.circuit.rewire(&groups);
    }

    /// A hash of the connectivity alone: which components exist, and what
    /// each wire's two ends attach to. Deliberately blind to positions and
    /// waypoint indices — dragging a corner point doesn't change what is
    /// connected to what, and shouldn't cost a rebuild.
    fn connectivity_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for placed in &self.placed {
            placed.id().hash(&mut hasher);
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

    /// Snapshots the document so the edit about to happen can be undone.
    /// **Call this before mutating**, not after — it records the state being
    /// left behind. For a drag, that means calling it the frame the drag
    /// starts, which is why `interact_box` deliberately doesn't move
    /// anything on that first frame.
    fn record_edit(&mut self) {
        self.undo_stack.push(self.to_project());
        if self.undo_stack.len() > MAX_UNDO_STEPS {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    fn undo(&mut self) {
        let Some(previous) = self.undo_stack.pop() else {
            return;
        };
        self.redo_stack.push(self.to_project());
        self.restore(&previous);
    }

    fn redo(&mut self) {
        let Some(next) = self.redo_stack.pop() else {
            return;
        };
        self.undo_stack.push(self.to_project());
        self.restore(&next);
    }

    /// Rebuilds the circuit from a snapshot, keeping everything that isn't
    /// part of the document itself: which file this is, the undo history,
    /// and UI preferences.
    ///
    /// **Known cost**: this goes through [`Self::from_project`], so it
    /// rebuilds the `Circuit` from scratch and runtime state starts cold —
    /// a held button releases, a clock's phase resets. Accepted because it
    /// keeps undo defined by exactly one thing (the saved document) instead
    /// of needing every `Circuit` mutation to be individually invertible.
    fn restore(&mut self, project: &SavedProject) {
        let language = self.language;
        let current_path = self.current_path.take();
        let undo_stack = std::mem::take(&mut self.undo_stack);
        let redo_stack = std::mem::take(&mut self.redo_stack);
        let window_title = std::mem::take(&mut self.window_title);
        // The camera is view state, not document state -- undoing an edit
        // shouldn't also throw away where you were looking.
        let scene_rect = self.scene_rect;

        *self = Self::from_project(project);
        self.scene_rect = scene_rect;

        self.language = language;
        self.current_path = current_path;
        self.undo_stack = undo_stack;
        self.redo_stack = redo_stack;
        self.window_title = window_title;
        // Deliberately coarse: stepping back to whatever was last saved
        // still counts as dirty. Over-reporting only ever costs a redundant
        // save prompt, whereas under-reporting loses work.
        self.dirty = true;
    }

    /// Runs a destructive action, but only after the user has had a chance
    /// to rescue unsaved work: with edits pending it just opens the
    /// confirmation and returns, and the dialog calls back into
    /// [`Self::run_action`] once the answer is in.
    fn request_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.dirty {
            self.pending_action = Some(action);
        } else {
            self.run_action(action, ctx);
        }
    }

    fn run_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::New => {
                let language = self.language;
                *self = Self::default();
                self.language = language;
            }
            PendingAction::Open => self.open_project(),
            PendingAction::Quit => {
                // `dirty` is already false by the time we get here (either
                // saved, or explicitly discarded), so the close request this
                // sends won't be intercepted and bounced back again.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// `SimLogix — <file name or "Untitled"><* if unsaved>`, so the title bar
    /// answers both "which circuit is this?" and "have I saved it?".
    fn title(&self, strings: &Strings) -> String {
        let name = self
            .current_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| strings.title_untitled.to_string());
        let marker = if self.dirty { "*" } else { "" };
        format!("SimLogix — {name}{marker}")
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
            self.advance_circuit(ticks_due as u64);
        }
        ui.ctx().request_repaint();

        let strings = Strings::for_language(self.language);

        // Keep the OS window title in step with which file is open and
        // whether it still has unsaved edits, but only push it when it
        // actually changed -- this runs every frame.
        let title = self.title(strings);
        if title != self.window_title {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        // Closing the window is just another way to trigger Quit, so it goes
        // through the same unsaved-changes check: bounce this request and
        // put the confirmation up instead.
        if ui.ctx().input(|i| i.viewport().close_requested()) && self.dirty {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_action = Some(PendingAction::Quit);
        }

        let new_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
        let open_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
        let save_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        let save_as_shortcut = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::S,
        );
        // Consumed here, before any widget sees the keystroke, so a shortcut
        // never doubles as canvas input.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&new_shortcut)) {
            self.request_action(PendingAction::New, ui.ctx());
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&open_shortcut)) {
            self.request_action(PendingAction::Open, ui.ctx());
        }
        if ui
            .ctx()
            .input_mut(|i| i.consume_shortcut(&save_as_shortcut))
        {
            self.save_project_as();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&save_shortcut)) {
            self.save_project();
        }

        if ui.ctx().input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::NONE,
                egui::Key::Space,
            ))
        }) {
            self.toggle_running();
        }

        let undo_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
        // Both the Ctrl+Shift+Z and the Ctrl+Y conventions, since which one
        // means "redo" depends entirely on what the user came from.
        let redo_shortcut = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        );
        let redo_alt_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
        // Redo is tested first: Ctrl+Shift+Z would otherwise also match the
        // plain Ctrl+Z pattern and undo instead.
        if ui.ctx().input_mut(|i| {
            i.consume_shortcut(&redo_shortcut) || i.consume_shortcut(&redo_alt_shortcut)
        }) {
            self.redo();
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&undo_shortcut)) {
            self.undo();
        }

        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(strings.menu_file, |ui| {
                    let shortcut =
                        |ui: &egui::Ui, s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_new)
                                .shortcut_text(shortcut(ui, &new_shortcut)),
                        )
                        .clicked()
                    {
                        self.request_action(PendingAction::New, ui.ctx());
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_open)
                                .shortcut_text(shortcut(ui, &open_shortcut)),
                        )
                        .clicked()
                    {
                        self.request_action(PendingAction::Open, ui.ctx());
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_save)
                                .shortcut_text(shortcut(ui, &save_shortcut)),
                        )
                        .clicked()
                    {
                        self.save_project();
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(strings.menu_file_save_as)
                                .shortcut_text(shortcut(ui, &save_as_shortcut)),
                        )
                        .clicked()
                    {
                        self.save_project_as();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(strings.menu_file_quit).clicked() {
                        self.request_action(PendingAction::Quit, ui.ctx());
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_simulation, |ui| {
                    let label = if self.running {
                        strings.menu_simulation_pause
                    } else {
                        strings.menu_simulation_run
                    };
                    if ui
                        .add(
                            egui::Button::new(label).shortcut_text(ui.ctx().format_shortcut(
                                &egui::KeyboardShortcut::new(
                                    egui::Modifiers::NONE,
                                    egui::Key::Space,
                                ),
                            )),
                        )
                        .clicked()
                    {
                        self.toggle_running();
                        ui.close();
                    }
                });
                ui.menu_button(strings.menu_edit, |ui| {
                    let shortcut =
                        |ui: &egui::Ui, s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
                    // Greyed out rather than hidden when there's nothing to
                    // step to, so the menu also answers "is there anything
                    // to undo?".
                    if ui
                        .add_enabled(
                            !self.undo_stack.is_empty(),
                            egui::Button::new(strings.menu_edit_undo)
                                .shortcut_text(shortcut(ui, &undo_shortcut)),
                        )
                        .clicked()
                    {
                        self.undo();
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.redo_stack.is_empty(),
                            egui::Button::new(strings.menu_edit_redo)
                                .shortcut_text(shortcut(ui, &redo_shortcut)),
                        )
                        .clicked()
                    {
                        self.redo();
                        ui.close();
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
            let hint = if let Some(net) = self.unstable_net {
                Some(strings.status_unstable.replace("{}", &net.0.to_string()))
            } else if !self.running {
                Some(strings.status_paused.to_string())
            } else if self.wiring_from.is_some() {
                Some(strings.hint_wiring.to_string())
            } else if let Tool::Place(kind) = self.tool {
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
                    if let Some(tool) = palette::show(ui, strings, self.tool) {
                        // Clicking the active entry again drops back to
                        // selecting, so the palette doubles as its own
                        // "never mind" button.
                        self.tool = if self.tool == tool {
                            Tool::Select
                        } else {
                            tool
                        };
                    }
                });
            });

        // Declared after the palette so it spans only the canvas, not the
        // whole window: panels claim their space in declaration order, and
        // this bar acts on the canvas alone.
        egui::Panel::top("toolbar").show(ui, |ui| {
            if let Some(tool) = toolbar::show(ui, strings, self.tool) {
                self.tool = tool;
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // Claim the wheel before `Scene` sees it, so it zooms instead of
            // panning (the schematic-editor convention). Safe to do here:
            // panels are laid out before the central one, so the palette's
            // scroll area has already had its turn at this frame's input.
            let wheel = ui.ctx().input_mut(|i| {
                let dy = i.smooth_scroll_delta.y;
                if dy != 0.0 {
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                }
                dy
            });

            let mut zoom_pivot = None;
            // Copied out and written back so the closure can still borrow
            // the rest of `self`; `Scene` mutates it in place as the user
            // pans and zooms.
            let mut scene_rect = self.scene_rect;
            let scene_response = egui::Scene::new().zoom_range(MIN_ZOOM..=MAX_ZOOM).show(
                ui,
                &mut scene_rect,
                |ui| {
                    // Inside the scene everything is in scene coordinates: the
                    // visible area is the clip rect, and raw pointer positions (which
                    // egui reports globally) have to be mapped in.
                    let canvas_rect = ui.clip_rect();
                    let painter = ui.painter().clone();
                    let to_scene = ui
                        .ctx()
                        .layer_transform_from_global(ui.layer_id())
                        .unwrap_or_default();
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
                    // Faint enough to stay a background on either theme: the
                    // weak text colour already tracks the background, and the
                    // alpha keeps the dots from competing with the circuit.
                    canvas::draw_grid(
                        &painter,
                        canvas_rect,
                        ui.visuals().weak_text_color().gamma_multiply(0.55),
                    );

                    if let Some(selected) = self.selected {
                        if ui.ctx().input(|i| i.key_pressed(egui::Key::R))
                            && self.placed.iter().any(|p| p.id() == selected)
                        {
                            self.record_edit();
                            if let Some(placed) =
                                self.placed.iter_mut().find(|p| p.id() == selected)
                            {
                                placed.rotate();
                            }
                        }
                    }

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

                    let mut pin_handles = Vec::new();
                    for placed in &mut self.placed {
                        let frame = placed.draw_and_interact(
                            ui,
                            &painter,
                            &mut self.circuit,
                            self.selected,
                        );
                        if let Some(id) = frame.clicked {
                            // A component and a wire are never selected at once:
                            // Delete checks the wire first, so leaving a stale wire
                            // selected would delete that instead of the component
                            // just clicked.
                            self.selected = Some(id);
                            self.selected_wire = None;
                            click_consumed = true;
                        }
                        grab_started |= frame.grab_started;
                        input_changed |= frame.input_changed;
                        if frame.settled {
                            self.pending_attach = Some(placed.id());
                        }
                        pin_handles.extend(frame.pins);
                    }
                    if grab_started {
                        self.record_edit();
                    }
                    // A button press is runtime state, not an edit: it settles
                    // the circuit but never touches undo.
                    if input_changed {
                        self.advance_circuit(SETTLE_TICKS);
                    }
                    // A pin's current on-canvas position this frame, resolved by
                    // identity -- every `Wire` endpoint that's a pin looks itself up
                    // here rather than storing a position directly, so it tracks a
                    // moved component automatically.
                    let pin_position =
                        |component: ComponentId, pin_index: usize| -> Option<egui::Pos2> {
                            pin_handles
                                .iter()
                                .find(|h| h.component == component && h.pin_index == pin_index)
                                .map(|h| h.position)
                        };

                    let click_pos = ui
                        .ctx()
                        .input(|i| i.pointer.primary_clicked())
                        .then_some(pointer_scene)
                        .flatten();
                    // Double-clicking along an existing wire inserts a new waypoint
                    // right there, so a wire can be reshaped in more places than
                    // just its existing points.
                    // Right-clicking a wire cuts the segment under the pointer.
                    let secondary_click_pos = ui
                        .ctx()
                        .input(|i| i.pointer.secondary_clicked())
                        .then_some(pointer_scene)
                        .flatten();
                    let double_click_pos = ui
                        .ctx()
                        .input(|i| {
                            i.pointer
                                .button_double_clicked(egui::PointerButton::Primary)
                        })
                        .then_some(pointer_scene)
                        .flatten();

                    let hover_pos = pointer_scene;

                    // Every wire's endpoints and (possibly-defaulted) waypoint
                    // list, resolved once per frame. A junction depends on its
                    // host being resolved first, and a wire can be re-attached
                    // to any other one, so this repeats until a pass resolves
                    // nothing new rather than assuming creation order. Wires
                    // left unresolved are the genuinely unresolvable ones (a
                    // deleted component, or a tap cycle) and simply aren't drawn.
                    struct Resolved {
                        from: egui::Pos2,
                        to: egui::Pos2,
                        waypoints: Vec<egui::Pos2>,
                    }
                    let mut resolved: HashMap<u64, Resolved> = HashMap::new();
                    let mut progressed = true;
                    while progressed {
                        progressed = false;
                        for wire in &self.wires {
                            if resolved.contains_key(&wire.id) {
                                continue;
                            }
                            // Both ends resolve the same way. A junction may
                            // not be resolvable *yet* (its host can come later
                            // in the list); a later pass picks it up.
                            let place =
                                |endpoint: WireEndpoint, resolved: &HashMap<u64, Resolved>| {
                                    match endpoint {
                                        WireEndpoint::Pin(component, pin_index) => {
                                            pin_position(component, pin_index)
                                        }
                                        WireEndpoint::Junction {
                                            wire: host,
                                            waypoint,
                                        } => resolved
                                            .get(&host)
                                            .and_then(|r| r.waypoints.get(waypoint))
                                            .copied(),
                                        WireEndpoint::Free(pos) => Some(pos),
                                    }
                                };
                            let (Some(from_pos), Some(to_pos)) =
                                (place(wire.from, &resolved), place(wire.to, &resolved))
                            else {
                                continue;
                            };
                            // A wire is exactly the points it was given: no
                            // waypoints means a straight run end to end.
                            //
                            // There used to be an implicit mid-point bend here
                            // for wires drawn without any, back when routing
                            // wasn't under the user's control. It bred phantom
                            // points that only became real once dragged — and
                            // for a level wire it produced *two* of them at the
                            // same spot, which is what left a stray point on top
                            // of an end after cutting a segment.
                            let waypoints = wire.waypoints.clone();
                            resolved.insert(
                                wire.id,
                                Resolved {
                                    from: from_pos,
                                    to: to_pos,
                                    waypoints,
                                },
                            );
                            progressed = true;
                        }
                    }

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

                    for i in 0..self.wires.len() {
                        let wire_id = self.wires[i].id;
                        // `None` while both ends are loose: the wire is drawing,
                        // not yet a connection. Still very much on screen.
                        let net = self.wire_net(&self.wires[i]);
                        let Some(Resolved {
                            from: from_pos,
                            to: to_pos,
                            waypoints,
                        }) = resolved.remove(&wire_id)
                        else {
                            continue; // Stale, already skipped above.
                        };

                        let color = match net {
                            Some(net) => canvas::signal_color(
                                self.circuit.signal_at(net),
                                ui.visuals().dark_mode,
                            ),
                            None => ui.visuals().weak_text_color(),
                        };
                        let mut path = vec![from_pos];
                        path.extend(waypoints.iter().copied());
                        path.push(to_pos);

                        // Hovering a wire thickens it, so it's obvious which one a
                        // click is about to select out of several crossing ones.
                        // Wires are polylines, so this is a distance test rather
                        // than an `ui.interact` rect like the widgets use.
                        let is_hovered = self.wiring_from.is_none()
                            && hover_pos
                                .is_some_and(|pos| canvas::distance_to_path(pos, &path) < 6.0);
                        if is_hovered {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }

                        let is_selected_wire = self.selected_wire == Some(wire_id);
                        let stroke = if is_selected_wire {
                            egui::Stroke::new(3.0, canvas::accent_color(ui.visuals().dark_mode))
                        } else if is_hovered {
                            egui::Stroke::new(3.0, color.gamma_multiply(1.6))
                        } else {
                            egui::Stroke::new(2.0, color)
                        };

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
                                        self.selected_wire = Some(wire_id);
                                        self.selected = None;
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
                                        self.selected_wire = Some(wire_id);
                                        self.selected = None;
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
                            let response = ui.interact(
                                egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)),
                                egui::Id::new(("wire_end", wire_id, is_from)),
                                egui::Sense::click_and_drag(),
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
                                self.selected_wire = Some(wire_id);
                                self.selected = None;
                                click_consumed = true;
                            }
                        }

                        for (waypoint_index, &point) in waypoints.iter().enumerate() {
                            let handle_rect =
                                egui::Rect::from_center_size(point, egui::vec2(10.0, 10.0));
                            let response = ui.interact(
                                handle_rect,
                                egui::Id::new(("wire_point", wire_id, waypoint_index)),
                                egui::Sense::click_and_drag(),
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
                                    self.selected_wire = Some(wire_id);
                                    self.selected = None;
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

                    let delete_pressed = ui.ctx().input(|i| {
                        i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                    });
                    if delete_pressed {
                        if let Some(wire_id) = self.selected_wire {
                            self.record_edit();
                            self.remove_wires(vec![wire_id], &resolved_waypoints);
                            self.selected_wire = None;
                        } else if let Some(selected) = self.selected {
                            self.record_edit();
                            self.circuit.remove_component(selected);
                            self.placed.retain(|placed| placed.id() != selected);

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
                                    if matches!(*end, WireEndpoint::Pin(c, _) if c == selected) {
                                        if let Some(at) = at {
                                            *end = WireEndpoint::Free(at);
                                        }
                                    }
                                }
                            }
                            self.selected = None;
                        }
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
                            // With the wire tool, a click on empty canvas starts
                            // a wire there rather than doing nothing: it begins
                            // on a loose end, which can be dropped onto something
                            // later.
                            None if self.tool == Tool::Wire => {
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
                        && ui.ctx().input(|i| {
                            i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked()
                        })
                    {
                        if self.wiring_from.is_some() {
                            self.wiring_from = None;
                        } else {
                            self.selected = None;
                            self.selected_wire = None;
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
                        self.selected = None;
                        self.selected_wire = None;
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

                    if let Tool::Place(kind) = self.tool {
                        // A translucent preview of what's about to be dropped, at
                        // the grid position it will actually land on -- otherwise
                        // placing is a blind click.
                        if let Some(pos) = hover_pos {
                            let rect =
                                egui::Rect::from_center_size(canvas::snap_to_grid(pos), BOX_SIZE);
                            crate::symbol::draw(
                                &painter,
                                kind,
                                rect,
                                canvas::Rotation::default(),
                                ui.visuals().strong_text_color().gamma_multiply(0.45),
                                "",
                            );
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                        }
                    }
                },
            );

            self.scene_rect = scene_rect;

            // Placing goes through the scene's own background response, so a
            // click that landed on a component or a wire never also drops a
            // new component underneath it.
            if let Tool::Place(kind) = self.tool {
                if scene_response.response.clicked() {
                    if let Some(pos) = scene_response.response.interact_pointer_pos() {
                        self.record_edit();
                        let id = self.place(kind, canvas::snap_to_grid(pos));
                        self.pending_attach = Some(id);
                        // Holding shift keeps the kind loaded, so a row of
                        // LEDs is one trip to the palette rather than one per
                        // component. Releasing it drops back to selecting,
                        // which is what you want after the last one.
                        if !ui.ctx().input(|i| i.modifiers.shift) {
                            self.tool = Tool::Select;
                        }
                    }
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

            // Wheel zoom, applied to the framed region for the next frame:
            // shrinking it zooms in. Anchored on the pointer so the point
            // under the cursor stays put, which is what makes zooming feel
            // like it's following you rather than the window centre.
            if wheel != 0.0 {
                let pivot = zoom_pivot.unwrap_or_else(|| self.scene_rect.center());
                let factor = (-wheel * WHEEL_ZOOM_SENSITIVITY).exp();
                self.scene_rect = egui::Rect::from_min_max(
                    pivot + (self.scene_rect.min - pivot) * factor,
                    pivot + (self.scene_rect.max - pivot) * factor,
                );
            }
        });

        // The unsaved-changes gate. Modal on purpose: it's answering "what
        // happens to your work", so it shouldn't be possible to click past
        // it and forget it's there.
        if let Some(action) = self.pending_action {
            egui::Modal::new(egui::Id::new("confirm_discard")).show(ui.ctx(), |ui| {
                ui.heading(strings.confirm_discard_title);
                ui.label(strings.confirm_discard_body);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(strings.confirm_discard_save).clicked() {
                        // Only go through with the action if the save
                        // actually happened -- cancelling the file dialog
                        // must not quietly discard the work anyway.
                        if self.save_project() {
                            self.pending_action = None;
                            self.run_action(action, ui.ctx());
                        }
                    }
                    if ui.button(strings.confirm_discard_discard).clicked() {
                        self.dirty = false;
                        self.pending_action = None;
                        self.run_action(action, ui.ctx());
                    }
                    if ui.button(strings.confirm_discard_cancel).clicked() {
                        self.pending_action = None;
                    }
                });
            });
        }

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
