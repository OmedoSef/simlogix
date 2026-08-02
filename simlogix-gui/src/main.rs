//! SimLogix GUI entry point.

mod app;
mod canvas;
mod palette;
mod placed_component;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app::SimLogixApp::default()))),
    )
}
