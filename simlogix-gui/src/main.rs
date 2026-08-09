//! SimLogix GUI entry point.

mod app;
mod canvas;
mod circuit_tree;
mod help;
mod i18n;
mod icon;
mod palette;
mod placed_component;
mod project;
mod properties;
mod symbol;
mod toolbar;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "SimLogix",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_icon(icon::app_icon()),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(app::SimLogixApp::new(cc)))),
    )
}
