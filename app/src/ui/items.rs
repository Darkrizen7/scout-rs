//! Objects module: every game item, by category, with prices; a click opens the item sheet.

mod rows;
mod sheet;
mod sidebar;

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use crate::state::State;
use crate::ui::theme;
use crate::ui::widgets::{money_or_dash, quality_color, star_button};
use scout_core::gamedata::{GameData, GameItem};
use scout_worker::{Command, WorkerHandle};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Level,
    Auction,
}

#[derive(Clone, PartialEq, Eq)]
struct Filter {
    favorites: bool,
    class_id: Option<i64>,
    subclass_id: Option<i64>,
}

pub struct Items {
    search: String,
    filter: Filter,
    sort: Sort,
    descending: bool,
    expanded: HashSet<i64>,
    /// Item ids shown, filtered and sorted.
    rows: Vec<i64>,
    built_for: Option<(usize, u64, Filter, String, Sort, bool)>,
    /// Open sheet, and the sheets opened before it (back button).
    sheet: Option<sheet::Sheet>,
    back: Vec<i64>,
}

impl Default for Items {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: Filter {
                favorites: false,
                class_id: None,
                subclass_id: None,
            },
            sort: Sort::Name,
            descending: false,
            expanded: HashSet::new(),
            rows: Vec::new(),
            built_for: None,
            sheet: None,
            back: Vec::new(),
        }
    }
}

impl Items {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        let game = Arc::clone(&state.game);
        if game.is_empty() {
            return empty(ui, state, worker);
        }
        if let Some(item) = state.open_item.take() {
            self.open(item, state, worker);
        }
        let palette = theme::current(ui);
        egui::Panel::left("items-sidebar")
            .exact_size(230.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(8, 10)),
            )
            .show(ui, |ui| self.sidebar(ui, &game, state));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                if let Some(sheet) = &mut self.sheet {
                    let back = if self.back.is_empty() {
                        "← Liste"
                    } else {
                        "← Retour"
                    };
                    if ui.button(back).clicked() {
                        match self.back.pop() {
                            Some(previous) => {
                                self.sheet = Some(sheet::Sheet::new(previous, state, worker))
                            }
                            None => self.sheet = None,
                        }
                        return;
                    }
                    match sheet.show(ui, &game, state, worker) {
                        Some(sheet::Link::Item(id)) => self.open(id, state, worker),
                        Some(sheet::Link::Recipe(i)) => state.show_recipe(i),
                        None => {}
                    }
                } else {
                    self.list(ui, &game, state, worker);
                }
            });
    }

    /// Opens an item sheet, remembering the one shown before.
    fn open(&mut self, item_id: i64, state: &mut State, worker: &WorkerHandle) {
        if let Some(current) = &self.sheet {
            if current.item_id == item_id {
                return;
            }
            self.back.push(current.item_id);
        }
        self.sheet = Some(sheet::Sheet::new(item_id, state, worker));
    }

    fn list(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        state: &mut State,
        worker: &WorkerHandle,
    ) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("🔍 Rechercher un objet (nom ou numéro)")
                    .desired_width(340.0),
            );
            let all = Filter {
                favorites: false,
                class_id: None,
                subclass_id: None,
            };
            let filtered = !self.search.is_empty() || self.filter != all;
            if ui
                .add_enabled(filtered, egui::Button::new("✖ Effacer les filtres"))
                .clicked()
            {
                self.search.clear();
                self.filter = all;
            }
        });
        ui.add_space(6.0);
        self.rebuild(game, state);
        ui.weak(format!("{} objets", self.rows.len()));

        let palette = theme::current(ui);
        let mut open = None;
        let mut star = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(160.0).clip(true))
            .column(Column::exact(55.0))
            .column(Column::exact(115.0))
            .column(Column::exact(115.0))
            .column(Column::exact(100.0))
            .column(Column::exact(34.0))
            .header(26.0, |mut header| {
                self.sort_header(&mut header, "Nom", Some(Sort::Name));
                self.sort_header(&mut header, "Catégorie", None);
                self.sort_header(&mut header, "Niv.", Some(Sort::Level));
                self.sort_header(&mut header, "Prix HdV", Some(Sort::Auction));
                self.sort_header(&mut header, "Achat PNJ", None);
                self.sort_header(&mut header, "Revente", None);
                header.col(|_| {});
            })
            .body(|body| {
                body.rows(30.0, self.rows.len(), |mut row| {
                    let id = self.rows[row.index()];
                    let Some(item) = game.items.get(&id) else {
                        return;
                    };
                    row.col(|ui| {
                        let mut name = RichText::new(&item.name);
                        if let Some(color) = quality_color(item.quality, palette.dark) {
                            name = name.color(color);
                        }
                        ui.label(name);
                    });
                    row.col(|ui| {
                        ui.weak(category(game, item));
                    });
                    row.col(|ui| {
                        ui.weak(if item.item_level > 0 {
                            item.item_level.to_string()
                        } else {
                            String::new()
                        });
                    });
                    row.col(|ui| money_or_dash(ui, state.auction_price(id)));
                    row.col(|ui| {
                        money_or_dash(ui, state.vendor.price(id).map(|p| p.round() as i64))
                    });
                    row.col(|ui| money_or_dash(ui, Some(item.sell_price).filter(|&p| p > 0)));
                    row.col(|ui| {
                        if star_button(ui, state.is_favorite(id), palette).clicked() {
                            star = Some(id);
                        }
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        open = Some(id);
                    }
                });
            });
        if let Some(id) = star {
            state.toggle_favorite(id, worker);
        } else if let Some(id) = open {
            self.open(id, state, worker);
        }
    }

    fn sort_header(
        &mut self,
        header: &mut egui_extras::TableRow<'_, '_>,
        label: &str,
        sort: Option<Sort>,
    ) {
        header.col(|ui| {
            let dim = theme::current(ui).dim_text;
            let Some(sort) = sort else {
                ui.label(RichText::new(label).color(dim));
                return;
            };
            let arrow = match (self.sort == sort, self.descending) {
                (false, _) => "",
                (true, false) => " ⏶",
                (true, true) => " ⏷",
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(format!("{label}{arrow}")).color(dim))
                        .frame(false),
                )
                .clicked()
            {
                self.descending = self.sort == sort && !self.descending;
                self.sort = sort;
            }
        });
    }
}

/// "Epées à deux mains", else the class name.
pub fn category(game: &GameData, item: &GameItem) -> String {
    let Some(class) = item.class_id else {
        return String::new();
    };
    game.category_name(class, item.subclass_id)
        .or_else(|| game.category_name(class, None))
        .unwrap_or_default()
        .to_owned()
}

fn empty(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.label(RichText::new("📦").size(48.0));
        ui.heading("Objets");
        ui.add_space(6.0);
        ui.label("Il faut d'abord télécharger les données du jeu (objets, recettes, métiers de WoW Forever).");
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
