//! Recipes module: search a craft, or an item and the crafts that make or use it.
//! Side bar (search, professions), results list, detail of the selected recipe or item.

pub(crate) mod detail;

use std::sync::Arc;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::State;
use crate::ui::sidebar::{Icon, entry, rgb, tint};
use crate::ui::theme;
use scout_core::gamedata::GameData;
use scout_worker::{Command, WorkerHandle};

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
}

impl Recipes {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        let game = Arc::clone(&state.game);
        if game.is_empty() {
            return empty(ui, state, worker);
        }
        if let Some(recipe) = state.open_recipe.take() {
            self.selection = Some(recipe);
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
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("🔍 Recette ou objet…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        let palette = theme::current(ui);
        let all = entry(
            ui,
            Some(("📚", palette.accent)),
            "Tous les métiers",
            Some(game.recipes.len()),
            self.profession.is_none(),
            0,
        );
        if all.clicked() {
            self.profession = None;
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

    fn results(&mut self, ui: &mut egui::Ui, game: &GameData, state: &mut State) {
        self.rebuild(game);
        ui.weak(format!("{} résultats", self.hits.len()));
        let mut clicked = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().clip(true))
            .body(|body| {
                body.rows(40.0, self.hits.len(), |mut row| {
                    let hit = self.hits[row.index()];
                    row.set_selected(matches!(hit, Hit::Recipe(i) if self.selection == Some(i)));
                    row.col(|ui| {
                        ui.vertical(|ui| match hit {
                            Hit::Recipe(i) => {
                                let r = &game.recipes[i];
                                ui.label(&r.name);
                                let skill =
                                    r.required.map_or_else(String::new, |s| format!(" · {s}"));
                                ui.weak(format!("{}{skill}", game.profession_name(r.profession)));
                            }
                            Hit::Item(id) => {
                                ui.label(format!("📦 {}", game.item_name(id)));
                                let made = game.made_by.get(&id).map_or(0, Vec::len);
                                let used = game.used_in.get(&id).map_or(0, Vec::len);
                                ui.weak(format!(
                                    "objet · produit par {made} · utilisé dans {used}"
                                ));
                            }
                        });
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        clicked = Some(hit);
                    }
                });
            });
        match clicked {
            Some(Hit::Recipe(i)) => {
                self.selection = Some(i);
            }
            Some(Hit::Item(id)) => state.show_item(id),
            None => {}
        }
    }

    /// Recipes whose name or product matches, then items made or used by recipes.
    fn rebuild(&mut self, game: &GameData) {
        let key = (
            game as *const GameData as usize,
            self.search.clone(),
            self.profession,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let needle = self.search.trim().to_lowercase();
        let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
        let mut recipes: Vec<usize> = (0..game.recipes.len())
            .filter(|&i| {
                let r = &game.recipes[i];
                self.profession.is_none_or(|p| r.profession == p)
                    && (matches(&r.name)
                        || r.creates
                            .is_some_and(|c| matches(&game.item_name(c.item_id))))
            })
            .collect();
        recipes.sort_by_key(|&i| {
            (
                game.recipes[i].required.unwrap_or(0),
                game.recipes[i].name.to_lowercase(),
            )
        });
        self.hits = recipes.into_iter().map(Hit::Recipe).collect();
        if !needle.is_empty() {
            // Reagents (and products) matching the search, limited to the selected profession.
            let mut items: Vec<i64> = game
                .used_in
                .keys()
                .chain(game.made_by.keys())
                .copied()
                .filter(|id| matches(&game.item_name(*id)))
                .filter(|id| {
                    self.profession.is_none_or(|p| {
                        let uses = |list: Option<&Vec<usize>>| {
                            list.is_some_and(|l| l.iter().any(|&i| game.recipes[i].profession == p))
                        };
                        uses(game.used_in.get(id)) || uses(game.made_by.get(id))
                    })
                })
                .collect();
            items.sort_unstable();
            items.dedup();
            items.sort_by_key(|id| game.item_name(*id).to_lowercase());
            self.hits.extend(items.into_iter().map(Hit::Item));
        }
        self.built_for = Some(key);
    }
}

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
        if ui.add_enabled(!state.busy, egui::Button::new("⤓ Télécharger les données du jeu")).clicked() {
            worker.send(Command::UpdateGameData);
        }
        if state.busy {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&state.status);
            });
        }
    });
}

/// Icon of a profession (skill line id); only glyphs present in egui's bundled fonts.
fn profession_icon(id: i64, dark: bool) -> Icon {
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
