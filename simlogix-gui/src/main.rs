//! SimLogix GUI entry point.

mod app;
mod canvas;
mod circuit_tree;
mod help;
mod i18n;
mod palette;
mod placed_component;
mod project;
mod properties;
mod symbol;
mod toolbar;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(app::SimLogixApp::new(cc)))),
    )
}
