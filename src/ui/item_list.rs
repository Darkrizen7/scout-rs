//! Searchable, sortable item list. Clicking an item opens its chart.

use std::cmp::Ordering;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::app::{State, View, item_name};
use crate::db::ItemSummary;
use crate::money;
use crate::ui::chart::format_datetime;
use crate::worker::WorkerHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
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
    /// (items revision, search, sort, descending) the rows were built for.
    built_for: Option<(u64, String, Sort, bool)>,
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
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Nom ou numéro d'objet")
                    .desired_width(300.0),
            );
            if !self.search.is_empty() && ui.small_button("✖").clicked() {
                self.search.clear();
            }
        });
        ui.add_space(4.0);
        self.rebuild(state);

        let mut clicked = None;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(70.0))
            .column(Column::exact(110.0))
            .column(Column::exact(80.0))
            .column(Column::exact(130.0))
            .column(Column::exact(70.0))
            .header(22.0, |mut header| {
                self.sort_header(&mut header, "Nom", Some(Sort::Name));
                header.col(|ui| {
                    ui.strong("ID");
                });
                self.sort_header(&mut header, "Prix", Some(Sort::Price));
                self.sort_header(&mut header, "En vente", Some(Sort::Quantity));
                header.col(|ui| {
                    ui.strong("Dernier scan");
                });
                self.sort_header(&mut header, "Relevés", Some(Sort::Scans));
            })
            .body(|body| {
                body.rows(20.0, self.rows.len(), |mut row| {
                    let item = &state.items[self.rows[row.index()]];
                    row.set_selected(state.selected == Some(item.item_id));
                    row.col(|ui| {
                        ui.label(item_name(item));
                    });
                    row.col(|ui| {
                        ui.label(item.item_id.to_string());
                    });
                    row.col(|ui| {
                        ui.label(item.price.map(money::format).unwrap_or_else(|| "—".into()));
                    });
                    row.col(|ui| {
                        ui.label(item.quantity.map_or_else(|| "—".into(), |q| q.to_string()));
                    });
                    row.col(|ui| {
                        ui.label(format_datetime(item.ts));
                    });
                    row.col(|ui| {
                        ui.label(item.scans.to_string());
                    });
                    if row.response().clicked() {
                        clicked = Some(item.item_id);
                    }
                });
            });

        if let Some(id) = clicked {
            state.select(id, worker);
            state.view = View::Chart;
        }
    }

    fn sort_header(
        &mut self,
        header: &mut egui_extras::TableRow<'_, '_>,
        label: &str,
        sort: Option<Sort>,
    ) {
        header.col(|ui| {
            let Some(sort) = sort else { return };
            let arrow = match (self.sort == sort, self.descending) {
                (false, _) => "",
                (true, false) => " ⏶",
                (true, true) => " ⏷",
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(format!("{label}{arrow}")).strong())
                        .frame(false),
                )
                .clicked()
            {
                self.descending = self.sort == sort && !self.descending;
                self.sort = sort;
            }
        });
    }

    fn rebuild(&mut self, state: &State) {
        let key = (
            state.revision,
            self.search.clone(),
            self.sort,
            self.descending,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let needle = self.search.trim().to_lowercase();
        let items = &state.items;
        self.rows = (0..items.len())
            .filter(|&i| matches(&items[i], &needle))
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

fn matches(item: &ItemSummary, needle: &str) -> bool {
    needle.is_empty()
        || item.item_id.to_string() == needle
        || item
            .name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains(needle))
}

fn compare(a: &ItemSummary, b: &ItemSummary, sort: Sort) -> Ordering {
    let by = match sort {
        // Unnamed items last.
        Sort::Name => match (&a.name, &b.name) {
            (Some(x), Some(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
            (x, y) => y.is_some().cmp(&x.is_some()),
        },
        Sort::Price => a.price.cmp(&b.price),
        Sort::Quantity => a.quantity.cmp(&b.quantity),
        Sort::Scans => a.scans.cmp(&b.scans),
    };
    by.then(a.item_id.cmp(&b.item_id))
}
