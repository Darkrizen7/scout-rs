//! Settings dialog: appearance, imports, price server, game data, WoW folder; button to the data credits (`/thanks` page of the price server). Explanations live in hover texts.

use std::path::PathBuf;

use eframe::egui;

mod price_server;

use crate::state::State;
use crate::ui::{theme, widgets};
use scout_core::time::format_datetime;
use scout_core::update::AddonInstall;
use scout_core::{Theme, scout_api};
use scout_worker::{Command, Task, WorkerHandle};

#[derive(Default)]
pub struct Settings {
    open: bool,
    wow_dir: String,
    /// Ask the worker again whether the Scout addon is installed (it may have been since the start).
    check_addon: bool,
    price_server: price_server::Form,
}

impl Settings {
    pub fn open(&mut self, state: &State) {
        self.open = true;
        self.check_addon = true;
        self.price_server.open(state);
        self.wow_dir = state
            .config
            .as_ref()
            .and_then(|c| c.wow_dir.as_ref())
            .map(|d| d.display().to_string())
            .unwrap_or_default();
    }

    pub fn window(&mut self, ctx: &egui::Context, state: &mut State, worker: &WorkerHandle) {
        if std::mem::take(&mut self.check_addon) {
            worker.send(Command::CheckScoutAddon);
        }
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
                section(ui, "Serveur de prix (Scout API)", false);
                self.price_server.ui(ui, state, worker);
                section(ui, "Données du jeu", false);
                game_data(ui, state, worker);
                section(ui, "WoW", false);
                self.wow(ui, state, worker);
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("ℹ Sources des données")
                        .on_hover_text(
                            "D'où viennent les données de l'outil, avec les liens : \
                             page du serveur de prix, dans le navigateur.",
                        )
                        .clicked()
                    {
                        let server = state.config.as_ref().map_or("", |c| &c.price_server.url);
                        ui.ctx()
                            .open_url(egui::OpenUrl::new_tab(scout_api::thanks_url(server)));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(version(state));
                    });
                });
                if state.any_busy() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&state.status);
                    });
                }
            });
        self.open = open;
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

/// "scout-rs 0.2.1", and whether a newer version is out or waits for a restart.
fn version(state: &State) -> String {
    let current = format!("scout-rs {}", env!("CARGO_PKG_VERSION"));
    if cfg!(debug_assertions) {
        return format!("{current} (dev)");
    }
    match (&state.update_installed, &state.update) {
        (Some(installed), _) => format!("{current} · {installed} installée, à redémarrer"),
        (None, Some(release)) => format!("{current} · {} disponible", release.version),
        (None, None) => current,
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
    let missing = state.scout_addon == Some(AddonInstall::Missing);
    ui.horizontal(|ui| {
        let palette = theme::current(ui);
        let label = "⟳ Importer depuis Scout";
        let button = if missing {
            egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE)).fill(palette.loss)
        } else {
            egui::Button::new(label)
        };
        let hint = if missing {
            "⚠ Addon Scout introuvable dans Interface/AddOns du dossier WoW (réglage « WoW » plus bas) : \
             l'import échouera. Installe l'addon (bouton ci-dessous)."
        } else {
            "Prix, noms, marchands et butin enregistrés par Scout. Après chaque scan : /reload en jeu, \
             puis importer. Les relevés déjà là sont ignorés."
        };
        let import = ui
            .add_enabled(!state.busy(Task::Import), button)
            .on_hover_text(hint);
        if import.clicked() {
            worker.send(Command::Import);
        }
        let last = state.config.as_ref().and_then(|c| c.last_import);
        match last {
            Some(ts) => ui.weak(format!("Dernier import : {}", format_datetime(ts))),
            None => ui.weak("Jamais importé"),
        };
    });
    if missing {
        let palette = theme::current(ui);
        ui.colored_label(
            palette.loss,
            "⚠ Addon Scout introuvable dans le dossier WoW",
        );
    }
    match &state.scout_addon {
        Some(AddonInstall::Dev(_)) => {
            ui.weak("Addon Scout : installation de développement (lien), jamais mise à jour par l'outil.");
        }
        Some(AddonInstall::Installed(version)) if state.addon_offer().is_none() => {
            ui.weak(format!(
                "Addon Scout {} : à jour.",
                version.as_deref().unwrap_or("(version inconnue)")
            ));
        }
        _ => {}
    }
    widgets::addon_button(ui, state, worker);
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
                "Objets, recettes et cartes de WoW Forever, préparés par le serveur de prix \
                 (depuis wago.tools et Wowhead) ; ne télécharge que s'il a plus récent.",
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
        match (&state.latest_game_data, stored) {
            (None, _) => {
                ui.weak("vérification…");
            }
            (Some(Err(e)), _) => {
                ui.weak("dernière version inconnue").on_hover_text(e);
            }
            (Some(Ok(None)), _) => {
                ui.weak("pas encore sur le serveur").on_hover_text(
                    "Le serveur de prix n'a pas encore préparé les données du jeu pour cette \
                     version de l'outil.",
                );
            }
            (Some(Ok(Some(latest))), Some(build))
                if !latest.newer_than(Some((build, game.format, game.updated))) =>
            {
                if game.complete {
                    ui.colored_label(palette.gain, "✔ À jour");
                } else {
                    ui.colored_label(palette.star, "⚠ Incomplètes")
                        .on_hover_text(
                            "Wowhead a refusé des requêtes du serveur : il complète les noms \
                         d'objets manquants plus tard, « Mettre à jour » les récupérera.",
                        );
                }
            }
            (Some(Ok(Some(latest))), Some(build)) if latest.build == build => {
                ui.colored_label(palette.star, "⚠ Nouvelles données")
                    .on_hover_text(
                        "Le serveur a de nouvelles données pour cette version du jeu \
                     (plus complètes, ou plus d'informations gardées).",
                    );
            }
            (Some(Ok(Some(latest))), _) => {
                ui.colored_label(palette.star, format!("⚠ Nouvelle version {}", latest.build));
            }
        }
    });
}
