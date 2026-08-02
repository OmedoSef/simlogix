//! SimLogix GUI entry point.

struct SimLogixApp;

impl eframe::App for SimLogixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hello, SimLogix!");
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(SimLogixApp))),
    )
}
