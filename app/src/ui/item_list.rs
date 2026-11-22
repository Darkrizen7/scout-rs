//! Searchable, sortable item list, filtered by the side bar. Click a row: chart; click the star: favorite.

use std::cmp::Ordering;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::{Filter, State, item_name};
use crate::ui::widgets::star_button;
use crate::ui::{coins, theme};
use scout_core::ItemSummary;
use scout_core::search::Query;
use scout_core::time::format_datetime;
use scout_worker::WorkerHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Category,
    Price,
    Quantity,
    Scans,
}

pub struct ItemList {
    search: String,
    sort: Sort,
    descending: bool,
    /// Indices into `State::items`, filtered and sorted.
    rows: Vec<usize>,
    /// What the rows were built for: (revision, filter, search, sort, descending).
    built_for: Option<(u64, Filter, String, Sort, bool)>,
}

impl Default for ItemList {
    fn default() -> Self {
        Self {
            search: String::new(),
            sort: Sort::Name,
            descending: false,
            rows: Vec::new(),
            built_for: None,
        }
    }
}

impl ItemList {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("🔍 Rechercher un objet (nom ou numéro)")
                    .desired_width(340.0),
            );
            let filtered = !self.search.is_empty() || !state.filter.is_all();
            if ui
                .add_enabled(filtered, egui::Button::new("✖ Effacer les filtres"))
                .clicked()
            {
                self.search.clear();
                state.filter = Filter::default();
            }
        });
        ui.add_space(6.0);
        self.rebuild(state);
        if self.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.weak(if state.filter.favorites {
                    "Aucun favori : clique sur l'étoile ☆ d'un objet pour l'ajouter."
                } else {
                    "Aucun objet."
                });
            });
            return;
        }

        let palette = theme::current(ui);
        let mut open = None;
        let mut star = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(150.0).clip(true))
            .column(Column::exact(110.0))
            .column(Column::exact(80.0))
            .column(Column::exact(120.0))
            .column(Column::exact(70.0))
            .column(Column::exact(34.0))
            .header(26.0, |mut header| {
                self.sort_header(&mut header, "Nom", Sort::Name);
                self.sort_header(&mut header, "Catégorie", Sort::Category);
                self.sort_header(&mut header, "Prix", Sort::Price);
                self.sort_header(&mut header, "En vente", Sort::Quantity);
                header.col(|ui| {
                    ui.weak("Dernier scan");
                });
                self.sort_header(&mut header, "Relevés", Sort::Scans);
                header.col(|_| {});
            })
            .body(|body| {
                body.rows(30.0, self.rows.len(), |mut row| {
                    let item = &state.items[self.rows[row.index()]];
                    let favorite = state.is_favorite(item.item_id);
                    row.set_selected(state.selected == Some(item.item_id));
                    row.col(|ui| {
                        ui.label(item_name(item));
                    });
                    row.col(|ui| {
                        ui.weak(
                            item.subclass_name
                                .as_deref()
                                .or(item.class_name.as_deref())
                                .unwrap_or(""),
                        );
                    });
                    row.col(|ui| match item.price {
                        Some(price) => {
                            coins::money(ui, price, None);
                        }
                        None => {
                            ui.weak("—");
                        }
                    });
                    row.col(|ui| {
                        ui.label(item.quantity.map_or_else(|| "—".into(), |q| q.to_string()));
                    });
                    row.col(|ui| {
                        ui.weak(format_datetime(item.ts));
                    });
                    row.col(|ui| {
                        ui.weak(item.scans.to_string());
                    });
                    row.col(|ui| {
                        if star_button(ui, favorite, palette).clicked() {
                            star = Some(item.item_id);
                        }
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        open = Some(item.item_id);
                    }
                });
            });

        if let Some(id) = star {
            state.toggle_favorite(id, worker);
        } else if let Some(id) = open {
            state.open(id, worker);
        }
    }

    fn sort_header(&mut self, header: &mut egui_extras::TableRow<'_, '_>, label: &str, sort: Sort) {
        header.col(|ui| {
            let arrow = match (self.sort == sort, self.descending) {
                (false, _) => "",
                (true, false) => " ⏶",
                (true, true) => " ⏷",
            };
            let text =
                egui::RichText::new(format!("{label}{arrow}")).color(theme::current(ui).dim_text);
            if ui.add(egui::Button::new(text).frame(false)).clicked() {
                self.descending = self.sort == sort && !self.descending;
                self.sort = sort;
            }
        });
    }

    fn rebuild(&mut self, state: &State) {
        let key = (
            state.revision,
            state.filter.clone(),
            self.search.clone(),
            self.sort,
            self.descending,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let query = Query::new(&self.search);
        let items = &state.items;
        self.rows = (0..items.len())
            .filter(|&i| state.shown(items[i].item_id) && matches(&items[i], &query))
            .collect();
        let sort = self.sort;
        self.rows
            .sort_by(|&a, &b| compare(&items[a], &items[b], sort));
        if self.descending {
            self.rows.reverse();
        }
        self.built_for = Some(key);
    }
}

fn matches(item: &ItemSummary, query: &Query) -> bool {
    query.is_empty()
        || query.is_id(item.item_id)
        || item.name.as_deref().is_some_and(|n| query.matches(n))
}

fn compare(a: &ItemSummary, b: &ItemSummary, sort: Sort) -> Ordering {
    // Unnamed / uncategorized items last.
    fn text(x: &Option<String>, y: &Option<String>) -> Ordering {
        match (x, y) {
            (Some(x), Some(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
            (x, y) => y.is_some().cmp(&x.is_some()),
        }
    }
    let by = match sort {
        Sort::Name => text(&a.name, &b.name),
        Sort::Category => {
            text(&a.class_name, &b.class_name).then(text(&a.subclass_name, &b.subclass_name))
        }
        Sort::Price => a.price.cmp(&b.price),
        Sort::Quantity => a.quantity.cmp(&b.quantity),
        Sort::Scans => a.scans.cmp(&b.scans),
    };
    by.then(text(&a.name, &b.name))
        .then(a.item_id.cmp(&b.item_id))
}
