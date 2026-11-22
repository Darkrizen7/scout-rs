//! Top bar (views, import, settings) and status bar.

use std::path::PathBuf;

use eframe::egui;

use crate::app::{State, View};
use crate::config::Theme;
use crate::worker::{Command, WorkerHandle};

pub fn show(ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.view, View::List, "📋 Objets");
        ui.selectable_value(&mut state.view, View::Chart, "📈 Graphique");
        ui.selectable_value(&mut state.view, View::Calculator, "💰 Rentabilité");
        ui.separator();
        let import = ui
            .add_enabled(!state.busy, egui::Button::new("⟳ Importer depuis Scout"))
            .on_hover_text("Lance tools/export.lua sur le dernier SavedVariables/Scout.lua\n(fais /reload en jeu avant)");
        if import.clicked() {
            worker.send(Command::Import);
        }
        let legacy = ui
            .add_enabled(!state.busy, egui::Button::new("📥 Historique Auctionator / Forever Guide"))
            .on_hover_text(
                "Importe une fois les prix passés gardés par Auctionator (21 jours, un relevé par jour)\n\
                 et Forever Guide (dernier prix vu de chaque objet). Sans risque de doublon.",
            );
        if legacy.clicked() {
            worker.send(Command::ImportLegacy);
        }
        if ui.button("⚙ Réglages").clicked() {
            settings.open(state);
        }
        if state.busy {
            ui.spinner();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(config) = &mut state.config {
                let (icon, next, hint) = match config.theme {
                    Theme::Slate => ("☀", Theme::Parchment, "Thème clair (Parchemin)"),
                    Theme::Parchment => ("🌙", Theme::Slate, "Thème sombre (Ardoise)"),
                };
                if ui.button(icon).on_hover_text(hint).clicked() {
                    config.theme = next;
                    worker.send(Command::SetTheme(next));
                }
            }
        });
    });
}

pub fn status(ui: &mut egui::Ui, state: &State) {
    ui.horizontal(|ui| match &state.error {
        Some(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        None => {
            ui.label(format!("{} objets", state.items.len()));
            if !state.status.is_empty() {
                ui.separator();
                ui.label(&state.status);
            }
        }
    });
}

#[derive(Default)]
pub struct Settings {
    open: bool,
    wow_dir: String,
}

impl Settings {
    fn open(&mut self, state: &State) {
        self.open = true;
        self.wow_dir = state
            .config
            .as_ref()
            .and_then(|c| c.wow_dir.as_ref())
            .map(|d| d.display().to_string())
            .unwrap_or_default();
    }

    pub fn window(&mut self, ctx: &egui::Context, state: &State, worker: &WorkerHandle) {
        let mut open = self.open;
        let mut save = false;
        egui::Window::new("Réglages").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Dossier _forever_ de WoW :");
            ui.add(egui::TextEdit::singleline(&mut self.wow_dir).desired_width(500.0));
            ui.small("Scout.lua est cherché dans WTF/Account/*/SavedVariables, export.lua dans Interface/AddOns/Scout/tools.");
            save = ui.add_enabled(!state.busy, egui::Button::new("Enregistrer")).clicked();
        });
        if save {
            worker.send(Command::SetWowDir(PathBuf::from(self.wow_dir.trim())));
            open = false;
        }
        self.open = open;
    }
}
