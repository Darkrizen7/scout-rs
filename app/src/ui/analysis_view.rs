//! Analysis tab: market price level per day, weekend effect, and items priced abnormally today.

use std::collections::HashMap;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

mod charts;

use crate::state::{Filter, State};
use crate::ui::{coins, theme};
use scout_core::analysis::{self, Analysis, ItemAnalysis};
use scout_core::search::Query;
use scout_worker::{Command, WorkerHandle};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Weekend,
    Today,
}

pub struct AnalysisView {
    table: Table,
    search: String,
    significant_only: bool,
    /// Ignore items with fewer units usually listed (thin markets are noisy).
    min_quantity: i64,
    /// Cheapest first (buy) or most expensive first (sell).
    cheapest_first: bool,
}

impl Default for AnalysisView {
    fn default() -> Self {
        Self {
            table: Table::Today,
            search: String::new(),
            significant_only: true,
            min_quantity: 5,
            cheapest_first: true,
        }
    }
}

impl AnalysisView {
    /// Whether anything narrows the table (side bar or this view's own filters).
    fn filtered(&self, state: &State) -> bool {
        let defaults = Self::default();
        !state.filter.is_all()
            || !self.search.is_empty()
            || self.significant_only != defaults.significant_only
            || self.min_quantity != defaults.min_quantity
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        if state.analysis_for != Some(state.revision) && !state.importing() {
            state.analysis_for = Some(state.revision);
            worker.send(Command::Analyze);
        }
        let Some(analysis) = &state.analysis else {
            ui.spinner();
            return;
        };
        charts::summary(ui, analysis);
        charts::day_chart(ui, analysis);
        ui.separator();
        let (clicked, clear) = self.items(ui, analysis, state);
        if clear {
            state.filter = Filter::default();
        }
        if let Some(id) = clicked {
            state.open(id, worker);
        }
    }

    /// Filters, then the table; returns the item clicked and whether to clear the side bar filter.
    fn items(
        &mut self,
        ui: &mut egui::Ui,
        analysis: &Analysis,
        state: &State,
    ) -> (Option<i64>, bool) {
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.table, Table::Today, "Prix du jour anormal");
            ui.selectable_value(&mut self.table, Table::Weekend, "Écart week-end / semaine");
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("🔍 Nom ou numéro")
                    .desired_width(180.0),
            );
            ui.checkbox(&mut self.significant_only, "Seulement significatifs");
            ui.label("En vente ≥");
            ui.add(egui::DragValue::new(&mut self.min_quantity).range(0..=10_000));
            let order = if self.cheapest_first {
                "⬇ Plus bas d'abord"
            } else {
                "⬆ Plus haut d'abord"
            };
            if ui.button(order).clicked() {
                self.cheapest_first = !self.cheapest_first;
            }
            if ui
                .add_enabled(
                    self.filtered(state),
                    egui::Button::new("✖ Effacer les filtres"),
                )
                .clicked()
            {
                let defaults = Self::default();
                self.search.clear();
                self.significant_only = defaults.significant_only;
                self.min_quantity = defaults.min_quantity;
                clear = true;
            }
        });

        let names: HashMap<i64, &str> = state
            .items
            .iter()
            .filter_map(|i| Some((i.item_id, i.name.as_deref()?)))
            .collect();
        let query = Query::new(&self.search);
        let mut rows: Vec<(&ItemAnalysis, f64)> = analysis
            .items
            .iter()
            .filter(|i| i.median_quantity.is_none_or(|q| q >= self.min_quantity))
            .filter(|i| state.shown(i.item_id))
            .filter(|i| {
                query.is_empty()
                    || query.is_id(i.item_id)
                    || names.get(&i.item_id).is_some_and(|n| query.matches(n))
            })
            .filter_map(|i| {
                let (diff, significant) = match self.table {
                    Table::Weekend => i.weekend.as_ref().map(|w| (w.diff_pct, w.significant()))?,
                    Table::Today => i.latest.as_ref().map(|o| (o.diff_pct, o.significant()))?,
                };
                (significant || !self.significant_only).then_some((i, diff))
            })
            .collect();
        rows.sort_by(|a, b| a.1.total_cmp(&b.1));
        if !self.cheapest_first {
            rows.reverse();
        }

        ui.label(match self.table {
            Table::Today => format!(
                "Dernier jour de chaque objet contre la médiane de ses autres jours (au moins {} jours). \
                 Significatif : |z robuste| ≥ {} et écart ≥ {} %. {} objets.",
                analysis::MIN_BASELINE_DAYS,
                analysis::OUTLIER_Z,
                analysis::MIN_DIFF_PCT,
                rows.len()
            ),
            Table::Weekend => format!(
                "Prix médian samedi-dimanche contre les autres jours (au moins {} jours de chaque). \
                 Significatif : test de permutation p ≤ {} et écart ≥ {} %. {} objets.",
                analysis::MIN_DAYS_EACH,
                analysis::SIGNIFICANCE,
                analysis::MIN_DIFF_PCT,
                rows.len()
            ),
        });
        (self.table(ui, &rows, &names, state.selected), clear)
    }

    fn table(
        &self,
        ui: &mut egui::Ui,
        rows: &[(&ItemAnalysis, f64)],
        names: &HashMap<i64, &str>,
        selected: Option<i64>,
    ) -> Option<i64> {
        let palette = theme::current(ui);
        let headers: [&str; 6] = match self.table {
            Table::Today => ["Nom", "Jour", "Prix", "Habituel", "Écart", "z"],
            Table::Weekend => [
                "Nom",
                "Week-end",
                "Semaine",
                "Écart",
                "Jours (WE / sem.)",
                "p",
            ],
        };
        let mut clicked = None;
        TableBuilder::new(ui)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::remainder().at_least(180.0).clip(true))
            .columns(Column::exact(105.0), 5)
            .header(22.0, |mut header| {
                for h in headers {
                    header.col(|ui| {
                        ui.strong(h);
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, rows.len(), |mut row| {
                    let (item, diff) = rows[row.index()];
                    row.set_selected(selected == Some(item.item_id));
                    let name = names
                        .get(&item.item_id)
                        .map_or_else(|| format!("objet #{}", item.item_id), |n| n.to_string());
                    let color = if diff < 0.0 {
                        palette.gain
                    } else {
                        palette.loss
                    };
                    let diff = Cell::Colored(format!("{diff:+.1} %"), color);
                    let cells: [Cell; 5] = match (self.table, &item.latest, &item.weekend) {
                        (Table::Today, Some(o), _) => [
                            Cell::Text(o.date.format("%d/%m").to_string()),
                            Cell::Money(o.price),
                            Cell::Money(o.baseline),
                            diff,
                            Cell::Text(format!("{:+.1}", o.z)),
                        ],
                        (Table::Weekend, _, Some(w)) => [
                            Cell::Money(w.weekend_price),
                            Cell::Money(w.weekday_price),
                            diff,
                            Cell::Text(format!("{} / {}", w.weekend_days, w.weekday_days)),
                            Cell::Text(format!("{:.3}", w.p_value)),
                        ],
                        _ => Default::default(),
                    };
                    row.col(|ui| {
                        ui.label(name);
                    });
                    for cell in cells {
                        row.col(|ui| match cell {
                            Cell::Text(text) => {
                                ui.label(text);
                            }
                            Cell::Colored(text, color) => {
                                ui.colored_label(color, text);
                            }
                            Cell::Money(copper) => {
                                coins::money(ui, copper, None);
                            }
                            Cell::Empty => {}
                        });
                    }
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        clicked = Some(item.item_id);
                    }
                });
            });
        clicked
    }
}

#[derive(Default)]
enum Cell {
    #[default]
    Empty,
    Text(String),
    Colored(String, egui::Color32),
    Money(i64),
}
