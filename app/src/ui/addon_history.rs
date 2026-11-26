//! Small window shown while the prices are written for the addon, then its result.

use eframe::egui;
use scout_worker::Task;

use crate::state::State;

pub fn window(ctx: &egui::Context, state: &mut State) {
    let running = state.busy(Task::AddonHistory);
    if !running && state.addon_history.is_none() {
        return;
    }
    let mut close = false;
    egui::Window::new("Prix de l'addon")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| match state.addon_history {
            Some(items) => {
                ui.label(format!(
                    "Fait : {items} objets envoyés. Tape /reload en jeu."
                ));
                ui.add_space(6.0);
                close = ui.button("OK").clicked();
            }
            None => {
                ui.label("Mise à jour des prix de l'addon…");
            }
        });
    if close {
        state.addon_history = None;
    }
}
