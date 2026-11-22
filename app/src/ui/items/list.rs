//! The item list (search, table) and the landing page with the search in the middle.

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use super::{Filter, Items, Sort, category};
use crate::state::State;
use crate::ui::sidebar::class_icon;
use crate::ui::theme;
use crate::ui::widgets::{landing_search, money_or_dash, quality_color, search_box, star_button};
use scout_core::gamedata::GameData;
use scout_worker::WorkerHandle;

impl Items {
    pub(super) fn list(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        state: &mut State,
        worker: &WorkerHandle,
    ) {
        if !self.browsing && self.search.trim().is_empty() {
            return self.landing(ui, game, state);
        }
        self.browsing = true;
        ui.horizontal(|ui| {
            search_box(ui, SEARCH_ID, SEARCH_HINT, &mut self.search, 340.0);
            if ui.button("✖ Effacer les filtres").clicked() {
                self.search.clear();
                self.filter = Filter::ALL;
                self.browsing = false;
            }
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(2.0);
        self.rebuild(game, state);
        ui.weak(format!("{} objets", self.rows.len()));

        let palette = theme::current(ui);
        let mut open = None;
        let mut star = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(190.0).clip(true))
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
                        if let Some(class) = item.class_id {
                            let (glyph, color) = class_icon(class, palette.dark);
                            ui.label(RichText::new(glyph).color(color));
                        }
                        ui.label(category(game, item));
                    });
                    row.col(|ui| {
                        if item.item_level > 0 {
                            ui.label(item.item_level.to_string());
                        }
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

    /// Arrival: only the search, in the middle of the page; typing (or a side bar entry) shows the table.
    fn landing(&mut self, ui: &mut egui::Ui, game: &GameData, state: &State) {
        let count = game
            .items
            .values()
            .filter(|i| !state.hides(&i.name))
            .count();
        let caption = format!("{count} objets · ou une catégorie à gauche");
        landing_search(ui, "📦", SEARCH_ID, SEARCH_HINT, &mut self.search, &caption);
    }
}

const SEARCH_ID: &str = "items-search";
const SEARCH_HINT: &str = "🔍 Rechercher un objet (nom ou numéro)";
