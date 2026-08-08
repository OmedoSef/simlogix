//! SimLogix GUI entry point.

mod app;
mod canvas;
mod i18n;
mod palette;
mod placed_component;
mod project;
mod symbol;
mod toolbar;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app::SimLogixApp::default()))),
    )
}
