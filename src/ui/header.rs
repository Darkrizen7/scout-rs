//! Module tabs (top bar), Market header (back, title), status bar and the settings dialog.

use std::path::PathBuf;

use eframe::egui;

use crate::config::Theme;
use crate::state::{Module, State, View, item_name};
use crate::ui::theme;
use crate::worker::{Command, WorkerHandle};

/// Top bar: module switcher in the middle (GNOME view switcher style), settings on the right.
pub fn tabs(ui: &mut egui::Ui, state: &mut State, settings: &mut Settings) {
    ui.set_min_height(34.0);
    let full = ui.max_rect();
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(full).layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        ),
        |ui| {
            for (module, icon, label) in [
                (Module::Items, "📦", "Objets"),
                (Module::Recipes, "🔨", "Recettes"),
                (Module::Market, "📈", "Market"),
            ] {
                if tab(ui, icon, label, state.module == module).clicked() {
                    state.module = module;
                }
            }
        },
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            if ui
                .button("⚙ Réglages")
                .on_hover_text("Thème, imports, dossier WoW")
                .clicked()
            {
                settings.open(state);
            }
            if state.busy {
                ui.spinner();
            }
        },
    );
}

/// One tab of the switcher: rounded, filled when selected.
fn tab(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let palette = theme::current(ui);
    let text = egui::RichText::new(format!("{icon}  {label}")).size(15.0);
    let fill = if selected {
        palette.sidebar_selected
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(egui::Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(130.0, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Market header: back button and where we are.
pub fn show(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if state.view == View::Chart && ui.button("←").on_hover_text("Retour à la liste").clicked()
        {
            state.view = View::List;
        }
        ui.label(egui::RichText::new(title(state)).heading());
    });
}

/// "Objets › Artisanat › Tissu", "Favoris", "Analyse › Artisanat", an item name…
fn title(state: &State) -> String {
    let base = match state.view {
        View::Chart => return state.selected_item().map(item_name).unwrap_or_default(),
        View::Calculator => return "Rentabilité".into(),
        View::Analysis => "Analyse",
        View::List if state.filter.favorites => return "Favoris".into(),
        View::List => "Tous les objets",
    };
    let f = &state.filter;
    let category = state
        .items
        .iter()
        .find(|i| f.class_id.is_some() && i.class_id == f.class_id);
    let mut parts = vec![base.to_owned()];
    if let Some(item) = category {
        parts.extend(item.class_name.clone());
        if f.subclass_id.is_some() {
            let sub = state
                .items
                .iter()
                .find(|i| i.class_id == f.class_id && i.subclass_id == f.subclass_id);
            parts.extend(sub.and_then(|i| i.subclass_name.clone()));
        }
    }
    parts.join(" › ")
}

pub fn status(ui: &mut egui::Ui, state: &State) {
    ui.horizontal(|ui| match &state.error {
        Some(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        None => {
            ui.weak(format!(
                "{} objets · {} favoris",
                state.items.len(),
                state.favorites.len()
            ));
            if !state.status.is_empty() {
                ui.weak("·");
                ui.weak(&state.status);
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

    pub fn window(&mut self, ctx: &egui::Context, state: &mut State, worker: &WorkerHandle) {
        let mut open = self.open;
        egui::Window::new("Réglages")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.strong("Apparence");
                if let Some(config) = &mut state.config {
                    let before = config.theme;
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut config.theme, Theme::Light, "☀ Clair");
                        ui.selectable_value(&mut config.theme, Theme::Dark, "🌙 Sombre");
                    });
                    if config.theme != before {
                        worker.send(Command::SetTheme(config.theme));
                    }
                }
                ui.separator();
                ui.strong("Importer les prix");
                ui.add_space(2.0);
                let import = ui
                    .add_enabled(!state.busy, egui::Button::new("⟳ Importer depuis Scout"))
                    .on_hover_text("Lance tools/export.lua sur le dernier SavedVariables/Scout.lua");
                if import.clicked() {
                    worker.send(Command::Import);
                }
                ui.weak("Après chaque scan : /reload en jeu, puis importer. Les relevés déjà là sont ignorés.");
                ui.add_space(6.0);
                let legacy = ui.add_enabled(!state.busy, egui::Button::new("📥 Historique Auctionator / Forever Guide"));
                if legacy.clicked() {
                    worker.send(Command::ImportLegacy);
                }
                ui.weak(
                    "Prix passés gardés par Auctionator (un relevé par jour) et Forever Guide \
                     (dernier prix vu). Sans risque de doublon.",
                );
                if state.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&state.status);
                    });
                }

                ui.separator();
                ui.strong("Données du jeu (onglet Recettes)");
                if state.game.is_empty() {
                    ui.weak("Pas encore téléchargées.");
                } else {
                    ui.weak(format!(
                        "WoW Forever {} · {} recettes · {} objets · téléchargées le {}",
                        state.game.build,
                        state.game.recipes.len(),
                        state.game.items.len(),
                        crate::ui::chart::format_datetime(state.game.updated)
                    ));
                }
                let palette = crate::ui::theme::current(ui);
                let stored = (!state.game.is_empty()).then_some(state.game.build.as_str());
                match (&state.latest_build, stored) {
                    (None, _) => {
                        ui.weak("Vérification de la dernière version de WoW Forever…");
                    }
                    (Some(Err(e)), _) => {
                        ui.weak(format!("Dernière version inconnue (hors ligne ?) : {e}"));
                    }
                    (Some(Ok(latest)), Some(build)) if latest == build && state.game.format < crate::db::GAME_FORMAT => {
                        ui.colored_label(palette.star, format!(
                            "⚠ Version {latest} à jour, mais à retélécharger : l'outil garde maintenant plus d'informations."
                        ));
                    }
                    (Some(Ok(latest)), Some(build)) if latest == build => {
                        ui.colored_label(palette.gain, format!("✔ À jour : c'est la dernière version de WoW Forever ({latest})."));
                    }
                    (Some(Ok(latest)), _) => {
                        ui.colored_label(palette.star, format!("⚠ Nouvelle version disponible : WoW Forever {latest}."));
                    }
                }
                let update = ui
                    .add_enabled(!state.busy, egui::Button::new("⤓ Mettre à jour les données du jeu"))
                    .on_hover_text("Cherche la dernière version de WoW Forever sur wago.tools ; ne télécharge que si elle a changé.");
                if update.clicked() {
                    worker.send(Command::UpdateGameData);
                }
                ui.separator();
                ui.strong("Dossier _forever_ de WoW");
                ui.add(egui::TextEdit::singleline(&mut self.wow_dir).desired_width(f32::INFINITY));
                ui.weak("Scout.lua est cherché dans WTF/Account/*/SavedVariables, export.lua dans Interface/AddOns/Scout/tools.");
                if ui.add_enabled(!state.busy, egui::Button::new("Enregistrer le dossier")).clicked() {
                    worker.send(Command::SetWowDir(PathBuf::from(self.wow_dir.trim())));
                }
            });
        self.open = open;
    }
}
