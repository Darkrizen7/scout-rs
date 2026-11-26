//! The bodies of the assistant's steps (the two with state of their own live in `setup.rs`).

use std::path::PathBuf;

use eframe::egui;
use scout_core::update::AddonInstall;
use scout_worker::{Command, Task, WorkerHandle};

use super::Setup;
use crate::state::State;
use crate::ui::settings::price_server;
use crate::ui::{theme, widgets};

impl Setup {
    pub(super) fn wow_dir(&mut self, ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
        ui.label(
            "Indique le dossier _forever_ de WoW Forever (celui qui contient Interface et WTF). \
             L'outil y lit les données de l'addon Scout et y installe l'addon.",
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.wow_dir)
                .hint_text("…/_forever_")
                .desired_width(f32::INFINITY),
        );
        let saved = state.config.as_ref().and_then(|c| c.wow_dir.as_ref());
        let typed = PathBuf::from(self.wow_dir.trim());
        ui.horizontal(|ui| {
            let changed = saved != Some(&typed) && !self.wow_dir.trim().is_empty();
            if ui
                .add_enabled(changed, egui::Button::new("Valider"))
                .clicked()
            {
                worker.send(Command::SetWowDir(typed.clone()));
            }
            if saved.is_some() && !changed {
                ui.colored_label(theme::current(ui).gain, "✔ Dossier validé");
            } else {
                ui.weak("Dossier non validé");
            }
        });
    }

    pub(super) fn house(&mut self, ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
        if std::mem::take(&mut self.load_houses) {
            worker.send(Command::LoadServerHouses);
        }
        ui.label(
            "Choisis la région, le mode de jeu et l'hôtel des ventes dont tu veux suivre les prix.",
        );
        ui.add_space(6.0);
        egui::Grid::new("setup_house")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| price_server::rows(ui, state, worker));
        if let Some(Err(error)) = &state.server_houses {
            ui.colored_label(theme::current(ui).loss, format!("⚠ {error}"));
        }
    }
}

pub(super) fn addon(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    ui.label(
        "L'addon Scout relève les prix en jeu et affiche ceux de l'outil. Il est facultatif : \
         tu peux l'installer plus tard depuis les réglages.",
    );
    ui.add_space(6.0);
    match &state.scout_addon {
        None => {
            ui.weak("Recherche de l'addon…");
        }
        Some(AddonInstall::Missing) => {
            ui.colored_label(theme::current(ui).loss, "Addon non installé.");
            if state.addon_offer().is_none() {
                ui.weak("Version de l'addon pas encore reçue…");
            }
        }
        Some(AddonInstall::Dev(_)) => {
            ui.label("Addon Scout : installation de développement, laissée telle quelle.");
        }
        Some(AddonInstall::Installed(version)) => {
            ui.label(format!(
                "Addon Scout installé, version {}.",
                version.as_deref().unwrap_or("inconnue")
            ));
        }
    }
    widgets::addon_button(ui, state, worker);
    if state.busy(Task::Addon) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak(&state.status);
        });
    }
}

pub(super) fn game_data(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    ui.label("Les objets, recettes et cartes du jeu sont téléchargés depuis le serveur Scout.");
    ui.add_space(6.0);
    if state.busy(Task::GameData) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Téléchargement…");
            ui.weak(&state.status);
        });
    } else if !state.game.is_empty() {
        ui.colored_label(theme::current(ui).gain, "✔ Données prêtes.");
    } else if ui.button("⤓ Télécharger les données du jeu").clicked() {
        worker.send(Command::UpdateGameData);
    }
    if let Some(error) = &state.error {
        ui.colored_label(theme::current(ui).loss, format!("⚠ {error}"));
    }
}

pub(super) fn prices(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    if state.scout_addon == Some(AddonInstall::Missing) {
        ui.label("Addon non installé : étape sautée.");
        return;
    }
    ui.label("Les prix des derniers jours sont écrits pour l'addon (fichier History.lua).");
    ui.add_space(6.0);
    let running = state.busy(Task::Prices) || state.busy(Task::AddonHistory);
    if running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Écriture des prix…");
            ui.weak(&state.status);
        });
    } else {
        match state.addon_history {
            Some(0) => {
                ui.colored_label(
                    theme::current(ui).loss,
                    "Aucun prix reçu pour cet hôtel des ventes : réessaie dans un instant.",
                );
            }
            Some(items) => {
                ui.colored_label(
                    theme::current(ui).gain,
                    format!("✔ {items} objets envoyés à l'addon."),
                );
            }
            // Written after the prices' download (house step); nothing written in this session.
            None => {
                ui.weak("Prix pas encore envoyés : « Réessayer » les envoie.");
            }
        }
    }
    if let Some(error) = &state.error {
        ui.colored_label(theme::current(ui).loss, format!("⚠ {error}"));
    }
    if ui
        .add_enabled(!running, egui::Button::new("Réessayer"))
        .clicked()
    {
        worker.send(Command::WriteAddonHistory);
    }
}

pub(super) fn done(ui: &mut egui::Ui) {
    ui.label("Tout est prêt.");
    ui.add_space(4.0);
    ui.label("Si le jeu est lancé, tape /reload (ou relance-le) pour charger l'addon et ses prix.");
}
