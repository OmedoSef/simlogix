//! The SimLogix application: state and the `eframe::App` loop.

use std::collections::HashMap;

use simlogix_core::{
    Button, Circuit, ComponentId, Led, NetId, Pin, PinDirection, Probe, Rail, Signal, Transistor,
};

use crate::canvas;
use crate::palette::{self, ComponentKind};
use crate::placed_component::PlacedComponent;

/// Pick a kind from the palette, click the canvas to drop it (snapped to the
/// grid), then drag from one pin to another to wire them together — that
/// merges their nets in `circuit` (see `Circuit::merge_nets`).
#[derive(Default)]
pub struct SimLogixApp {
    show_about: bool,
    circuit: Circuit,
    placed: Vec<PlacedComponent>,
    pending_placement: Option<ComponentKind>,
    selected: Option<ComponentId>,
    /// The net and screen anchor a wire-drag started from, if one is in progress.
    wiring_from: Option<(NetId, egui::Pos2)>,
    /// The currently selected wire: a net, and the index (within that net's
    /// group of pins sharing it) of the "far" endpoint — see the drawing loop.
    selected_wire: Option<(NetId, usize)>,
}

impl SimLogixApp {
    fn place(&mut self, kind: ComponentKind, center: egui::Pos2) {
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
                PlacedComponent::transistor(id, center, kind.label())
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
                PlacedComponent::rail(id, center, kind.label())
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
        };
        // A newly placed, unconnected component can't be part of a feedback
        // loop yet, so this can't actually be unstable.
        let _ = self.circuit.run();
        self.placed.push(placed);
    }
}

impl eframe::App for SimLogixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("?", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                    }
                });
            });
        });

        egui::Panel::left("palette").show(ui, |ui| {
            if let Some(kind) = palette::show(ui, self.pending_placement) {
                self.pending_placement = Some(kind);
            }
            if self.selected.is_some() {
                ui.add_space(8.0);
                ui.label("Press R to rotate, Delete to remove the selected component");
            }
            if self.selected_wire.is_some() {
                ui.add_space(8.0);
                ui.label("Press Delete to remove the selected wire");
            }
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

            let mut pin_handles = Vec::new();
            for placed in &mut self.placed {
                let frame =
                    placed.draw_and_interact(ui, &painter, &mut self.circuit, self.selected);
                if let Some(id) = frame.clicked {
                    self.selected = Some(id);
                }
                pin_handles.extend(frame.pins);
            }

            // Persistent wires: any two (or more) pins already sharing a net,
            // colored like the demo LED — red while `High`, gray otherwise.
            // Drawn as a star from the first pin in the group to every other
            // one, so each line is a distinct, selectable "wire".
            let mut handles_by_net: HashMap<NetId, Vec<&crate::placed_component::PinHandle>> =
                HashMap::new();
            for handle in &pin_handles {
                handles_by_net.entry(handle.net).or_default().push(handle);
            }

            let click_pos = ui.ctx().input(|i| {
                i.pointer
                    .primary_clicked()
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            });

            for (&net, handles) in &handles_by_net {
                if handles.len() < 2 {
                    continue;
                }
                let color = if self.circuit.signal_at(net) == Signal::High {
                    egui::Color32::from_rgb(220, 30, 30)
                } else {
                    egui::Color32::from_gray(120)
                };
                let anchor = handles[0].position;
                for (index, endpoint) in handles.iter().enumerate().skip(1) {
                    let is_selected_wire = self.selected_wire == Some((net, index));
                    let stroke = if is_selected_wire {
                        egui::Stroke::new(3.0, egui::Color32::from_rgb(90, 160, 255))
                    } else {
                        egui::Stroke::new(2.0, color)
                    };
                    painter.line_segment([anchor, endpoint.position], stroke);

                    if let Some(click_pos) = click_pos {
                        if canvas::distance_to_segment(click_pos, anchor, endpoint.position) < 6.0 {
                            self.selected_wire = Some((net, index));
                        }
                    }
                }
            }

            let delete_pressed = ui
                .ctx()
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
            if delete_pressed {
                if let Some((net, index)) = self.selected_wire {
                    // A wire is selected: delete it, not the component.
                    if let Some(endpoint) = handles_by_net.get(&net).and_then(|h| h.get(index)) {
                        self.circuit
                            .disconnect_pin(endpoint.component, endpoint.pin_index);
                        let _ = self.circuit.run();
                    }
                    self.selected_wire = None;
                } else if let Some(selected) = self.selected {
                    self.circuit.remove_component(selected);
                    self.placed.retain(|placed| placed.id() != selected);
                    self.selected = None;
                }
            }

            // A wire being dragged into existence: start it at the pin where the
            // drag began, follow the pointer, and complete it on release if the
            // pointer lands on a different pin.
            for handle in &pin_handles {
                if handle.drag_started {
                    self.wiring_from = Some((handle.net, handle.position));
                }
            }
            if let Some((from_net, anchor)) = self.wiring_from {
                let pointer_pos = ui.ctx().pointer_latest_pos().unwrap_or(anchor);
                painter.line_segment(
                    [anchor, pointer_pos],
                    egui::Stroke::new(2.0, egui::Color32::from_gray(200)),
                );

                if ui.ctx().input(|i| i.pointer.primary_released()) {
                    let target = pin_handles.iter().find(|handle| {
                        handle.net != from_net && handle.position.distance(pointer_pos) < 10.0
                    });
                    if let Some(target) = target {
                        self.circuit.merge_nets(from_net, target.net);
                        let _ = self.circuit.run();
                    }
                    self.wiring_from = None;
                }
            }

            if let Some(kind) = self.pending_placement {
                if let Some(click_pos) = canvas_response.interact_pointer_pos() {
                    self.place(kind, canvas::snap_to_grid(click_pos));
                    self.pending_placement = None;
                }
            }
        });

        egui::Window::new("About SimLogix")
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label("SimLogix — a cross-platform logic simulator.");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            });
    }
}
