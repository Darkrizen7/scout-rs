//! Scout price history: imports Scout's auction house scans into SQLite and shows them.

mod app;
mod config;
mod db;
mod import;
mod money;
mod ui;
mod worker;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Scout — prix HdV")
            .with_app_id("scout-prix")
            .with_inner_size([1000.0, 650.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "scout-prix",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
