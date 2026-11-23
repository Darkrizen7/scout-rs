//! Scout price history: imports Scout's auction house scans into SQLite and shows them.
// No console window behind the app on Windows (release builds; debug keeps it for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod state;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Scout")
            .with_app_id("scout-rs")
            .with_inner_size([1000.0, 650.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "scout-rs",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
