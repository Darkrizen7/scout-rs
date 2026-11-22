//! Settings dialog: appearance, imports, game data, WoW folder; button to the data credits. Explanations live in hover texts.

use std::path::PathBuf;

use eframe::egui;

use crate::state::State;
use crate::ui::{credits, theme};
use scout_core::time::format_datetime;
use scout_core::{GAME_FORMAT, Theme};
use scout_worker::{Command, Task, WorkerHandle};

#[derive(Default)]
pub struct Settings {
    open: bool,
    wow_dir: String,
    /// "Sources des données" window.
    credits: bool,
}

impl Settings {
    pub fn open(&mut self, state: &State) {
        self.open = true;
        self.wow_dir = state
            .config
            .as_ref()
            .and_then(|c| c.wow_dir.as_ref())
            .map(|d| d.display().to_string())
            .unwrap_or_default();
    }

    pub fn window(&mut self, ctx: &egui::Context, state: &mut State, worker: &WorkerHandle) {
        let mut open = self.open;
        egui::Window::new("Réglages")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                section(ui, "Apparence", true);
                appearance(ui, state, worker);
                section(ui, "Import", false);
                import(ui, state, worker);
                section(ui, "Données du jeu", false);
                game_data(ui, state, worker);
                section(ui, "WoW", false);
                self.wow(ui, state, worker);
                ui.add_space(6.0);
                ui.separator();
                if ui
                    .button("ℹ Sources des données")
                    .on_hover_text("D'où viennent les données de l'outil, avec les liens.")
                    .clicked()
                {
                    self.credits = true;
                }
                if state.any_busy() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&state.status);
                    });
                }
            });
        self.open = open;
        credits::window(ctx, &mut self.credits);
    }

    fn wow(&mut self, ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
        ui.horizontal(|ui| {
            let save = ui
                .add_enabled(!state.importing(), egui::Button::new("Enregistrer"))
                .on_hover_text("Enregistre le dossier _forever_ de WoW.");
            ui.add(
                egui::TextEdit::singleline(&mut self.wow_dir)
                    .hint_text("…/_forever_")
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text(
                "Dossier _forever_ de WoW. Scout.lua est cherché dans WTF/Account/*/SavedVariables, \
                 export.lua dans Interface/AddOns/Scout/tools.",
            );
            if save.clicked() {
                worker.send(Command::SetWowDir(PathBuf::from(self.wow_dir.trim())));
            }
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str, first: bool) {
    if !first {
        ui.add_space(6.0);
        ui.separator();
    }
    ui.add_space(4.0);
    ui.strong(title);
    ui.add_space(2.0);
}

fn appearance(ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
    let Some(config) = &mut state.config else {
        return;
    };
    let before = config.theme;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut config.theme, Theme::Light, "☀ Clair");
        ui.selectable_value(&mut config.theme, Theme::Dark, "🌙 Sombre");
    });
    if config.theme != before {
        worker.send(Command::SetTheme(config.theme));
    }
    let mut hide = config.hide_obsolete;
    let changed = ui
        .checkbox(&mut hide, "Masquer les objets inutilisables")
        .on_hover_text(
            "Objets restés dans les tables du jeu sans être obtenables : périmés (« Deprecated », « périmé »), \
             inutilisés (« Unused », « [INUTILISÉ] »), provisoires (« [PH] »), anciennes versions (« OLD »), \
             armes tenues par les monstres (« Monster - ») et objets internes (« DNT »).",
        )
        .changed();
    if changed {
        state.set_hide_obsolete(hide, worker);
    }
}

fn import(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    ui.horizontal(|ui| {
        let import = ui
            .add_enabled(!state.busy(Task::Import), egui::Button::new("⟳ Importer depuis Scout"))
            .on_hover_text(
                "Prix, noms, marchands et butin enregistrés par Scout. Après chaque scan : /reload en jeu, \
                 puis importer. Les relevés déjà là sont ignorés.",
            );
        if import.clicked() {
            worker.send(Command::Import);
        }
        let last = state.config.as_ref().and_then(|c| c.last_import);
        match last {
            Some(ts) => ui.weak(format!("Dernier import : {}", format_datetime(ts))),
            None => ui.weak("Jamais importé"),
        };
    });
    let legacy = ui
        .add_enabled(
            !state.busy(Task::ImportLegacy),
            egui::Button::new("📥 Historique Auctionator / Forever Guide"),
        )
        .on_hover_text(
            "Prix passés gardés par Auctionator (un relevé par jour) et Forever Guide (dernier prix vu). \
             Sans risque de doublon.",
        );
    if legacy.clicked() {
        worker.send(Command::ImportLegacy);
    }
}

fn game_data(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    let game = &state.game;
    let palette = theme::current(ui);
    let stored = (!game.is_empty()).then_some(game.build.as_str());
    ui.horizontal(|ui| {
        let update = ui
            .add_enabled(
                !state.busy(Task::GameData),
                egui::Button::new("⤓ Mettre à jour"),
            )
            .on_hover_text(
                "Tables du client WoW Forever (objets, recettes, cartes) depuis wago.tools ; \
                 ne télécharge que si la version a changé.",
            );
        if update.clicked() {
            worker.send(Command::UpdateGameData);
        }
        match stored {
            Some(build) => ui.label(format!("Version {build}")).on_hover_text(format!(
                "{} recettes · {} objets · téléchargées le {}",
                game.recipes.len(),
                game.items.len(),
                format_datetime(game.updated)
            )),
            None => ui.weak("Pas encore téléchargées"),
        };
        match (&state.latest_build, stored) {
            (None, _) => {
                ui.weak("vérification…");
            }
            (Some(Err(e)), _) => {
                ui.weak("dernière version inconnue")
                    .on_hover_text(format!("Hors ligne ? {e}"));
            }
            (Some(Ok(latest)), Some(build)) if latest == build && game.format < GAME_FORMAT => {
                ui.colored_label(palette.star, "⚠ À retélécharger")
                    .on_hover_text(
                        "L'outil garde maintenant plus d'informations sur cette version.",
                    );
            }
            (Some(Ok(latest)), Some(build)) if latest == build => {
                ui.colored_label(palette.gain, "✔ À jour");
            }
            (Some(Ok(latest)), _) => {
                ui.colored_label(palette.star, format!("⚠ Nouvelle version {latest}"));
            }
        }
    });
}
