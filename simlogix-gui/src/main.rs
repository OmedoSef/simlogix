//! SimLogix GUI entry point.

use std::cell::Cell;
use std::rc::Rc;

use simlogix_core::{Button, Circuit, ComponentId, Led, NetId, Pin, PinDirection, Signal};

/// A fixed demo scene: one push button wired directly to one LED. Not yet the
/// general placement/wiring editor — this only proves the simulation engine
/// runs correctly inside the GUI loop.
struct SimLogixApp {
    show_about: bool,
    circuit: Circuit,
    button: ComponentId,
    button_pressed: Rc<Cell<bool>>,
    led_net: NetId,
}

impl SimLogixApp {
    fn new() -> Self {
        let mut circuit = Circuit::new();
        let net = circuit.add_net();

        let (button_component, button_pressed) = Button::new();
        let button = circuit.add_component(
            Box::new(button_component),
            vec![Pin {
                direction: PinDirection::Output,
                net,
            }],
        );
        circuit.add_component(
            Box::new(Led),
            vec![Pin {
                direction: PinDirection::Input,
                net,
            }],
        );

        circuit.schedule_now(button);
        // A single button wired to a single LED has no feedback loop, so this
        // can't actually be unstable.
        let _ = circuit.run();

        Self {
            show_about: false,
            circuit,
            button,
            button_pressed,
            led_net: net,
        }
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hello, SimLogix!");
            ui.add_space(12.0);

            let response = ui.button("Push");
            let is_pressed = response.is_pointer_button_down_on();
            if is_pressed != self.button_pressed.get() {
                self.button_pressed.set(is_pressed);
                self.circuit.schedule_now(self.button);
                let _ = self.circuit.run();
            }

            ui.add_space(8.0);

            let (color, label) = match self.circuit.signal_at(self.led_net) {
                Signal::High => (egui::Color32::from_rgb(220, 30, 30), "ON"),
                _ => (egui::Color32::from_gray(60), "OFF"),
            };
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 9.0, color);
                ui.label(format!("LED: {label}"));
            });
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

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(SimLogixApp::new()))),
    )
}
