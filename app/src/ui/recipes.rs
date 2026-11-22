//! Recipes module: search a craft, or an item and the crafts that make or use it.
//! Side bar (professions), then the search: in the middle of the page on arrival, above the results after.

pub(crate) mod detail;
mod reagents;
mod results;
mod skill_bar;

use std::sync::Arc;

use eframe::egui;

use crate::state::State;
use crate::ui::sidebar::{Icon, entry, rgb, tint};
use crate::ui::theme;
use crate::ui::widgets::landing_search;
use scout_core::gamedata::GameData;
use scout_worker::{Command, Task, WorkerHandle};

/// A search result.
#[derive(Clone, Copy)]
enum Hit {
    Recipe(usize),
    /// An item that recipes make or use.
    Item(i64),
}

#[derive(Default)]
pub struct Recipes {
    search: String,
    profession: Option<i64>,
    /// Recipe shown (index in `game.recipes`).
    selection: Option<usize>,
    hits: Vec<Hit>,
    /// (data address, search, profession) the hits were computed for.
    built_for: Option<(usize, String, Option<i64>)>,
    /// False on arrival: the search sits in the middle of the page until a profession is chosen or text typed.
    browsing: bool,
}

impl Recipes {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        let game = Arc::clone(&state.game);
        if game.is_empty() {
            return empty(ui, state, worker);
        }
        if let Some(recipe) = state.open_recipe.take() {
            self.selection = Some(recipe);
            self.browsing = true;
        }
        let palette = theme::current(ui);
        egui::Panel::left("recipes-sidebar")
            .exact_size(230.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(8, 10)),
            )
            .show(ui, |ui| self.sidebar(ui, &game));
        if !self.browsing && self.search.trim().is_empty() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(palette.view))
                .show(ui, |ui| {
                    let caption =
                        format!("{} recettes · ou un métier à gauche", game.recipes.len());
                    landing_search(ui, "🔨", SEARCH_ID, SEARCH_HINT, &mut self.search, &caption);
                });
            return;
        }
        self.browsing = true;
        egui::Panel::left("recipes-results")
            .exact_size(340.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| self.results(ui, &game, state));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                let Some(recipe) = self.selection else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.weak("Choisis une recette dans la liste (un objet ouvre sa fiche dans l'onglet Objets).");
                    });
                    return;
                };
                if let Some(item) = detail::show(ui, &game, state, recipe) {
                    state.show_item(item);
                }
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, game: &GameData) {
        let palette = theme::current(ui);
        let all = entry(
            ui,
            Some(("📚", palette.accent)),
            "Tous les métiers",
            Some(game.recipes.len()),
            self.browsing && self.profession.is_none(),
            0,
        );
        if all.clicked() {
            self.profession = None;
            self.browsing = true;
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for p in &game.professions {
                    let count = game.recipes.iter().filter(|r| r.profession == p.id).count();
                    let icon = profession_icon(p.id, palette.dark);
                    if entry(
                        ui,
                        Some(icon),
                        &p.name,
                        Some(count),
                        self.profession == Some(p.id),
                        0,
                    )
                    .clicked()
                    {
                        self.profession = Some(p.id);
                        self.browsing = true;
                    }
                }
                ui.add_space(8.0);
                ui.weak(format!("Données : WoW Forever {}", game.build));
                if game.updated > 0 {
                    ui.weak(format!(
                        "téléchargées le {}",
                        scout_core::time::format_datetime(game.updated)
                    ));
                }
            });
    }
}

const SEARCH_ID: &str = "recipes-search";
const SEARCH_HINT: &str = "🔍 Rechercher une recette ou un objet";

/// No game data yet: explain and offer the download.
fn empty(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.label(egui::RichText::new("🔨").size(48.0));
        ui.heading("Recettes");
        ui.add_space(6.0);
        ui.label("Il faut d'abord télécharger les données du jeu (recettes, objets, métiers de WoW Forever).");
        ui.weak("Tables du client Forever en français, depuis wago.tools — environ 12 Mo, une seule fois.");
        ui.add_space(10.0);
        if ui.add_enabled(!state.busy(Task::GameData), egui::Button::new("⤓ Télécharger les données du jeu")).clicked() {
            worker.send(Command::UpdateGameData);
        }
        if state.busy(Task::GameData) {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&state.status);
            });
        }
    });
}

/// Icon of a profession (skill line id); only glyphs present in egui's bundled fonts.
pub(super) fn profession_icon(id: i64, dark: bool) -> Icon {
    let (glyph, color) = match id {
        171 => ("🍶", rgb(0x3584e4)), // Alchemy
        164 => ("⚒", rgb(0xc64600)),  // Blacksmithing
        333 => ("✨", rgb(0x9141ac)), // Enchanting
        202 => ("🔧", rgb(0x5e5c64)), // Engineering
        182 => ("🌿", rgb(0x26a269)), // Herbalism
        165 => ("👢", rgb(0x986a44)), // Leatherworking
        186 => ("⛏", rgb(0x865e3c)),  // Mining
        393 => ("🔪", rgb(0xa51d2d)), // Skinning
        197 => ("✂", rgb(0xc061cb)),  // Tailoring
        185 => ("🍳", rgb(0xe66100)), // Cooking
        129 => ("❤", rgb(0xe01b24)),  // First aid
        356 => ("🎣", rgb(0x2190a4)), // Fishing
        40 => ("☠", rgb(0x4e9a06)),   // Poisons
        _ => ("📖", rgb(0x77767b)),
    };
    (glyph, tint(color, dark))
}
