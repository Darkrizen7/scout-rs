//! Analysis tab: market price level per day, weekend effect, and items priced abnormally today.

use std::collections::HashMap;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use egui_plot::{Bar, BarChart, Plot};

use crate::analysis::{self, Analysis, ItemAnalysis};
use crate::state::{Filter, State};
use crate::ui::{coins, theme};
use crate::worker::{Command, WorkerHandle};

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
        if state.analysis_for != Some(state.revision) && !state.busy {
            state.analysis_for = Some(state.revision);
            worker.send(Command::Analyze);
        }
        let Some(analysis) = &state.analysis else {
            ui.spinner();
            return;
        };
        summary(ui, analysis);
        day_chart(ui, analysis);
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
        let needle = self.search.trim().to_lowercase();
        let mut rows: Vec<(&ItemAnalysis, f64)> = analysis
            .items
            .iter()
            .filter(|i| i.median_quantity.is_none_or(|q| q >= self.min_quantity))
            .filter(|i| state.shown(i.item_id))
            .filter(|i| {
                needle.is_empty()
                    || i.item_id.to_string() == needle
                    || names
                        .get(&i.item_id)
                        .is_some_and(|n| n.to_lowercase().contains(&needle))
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

fn summary(ui: &mut egui::Ui, analysis: &Analysis) {
    ui.label(format!(
        "{} jours de données : {} de week-end, {} de semaine. Prix d'un jour = médiane des prix min relevés ce jour-là.",
        analysis.days.len(),
        analysis.weekend_days,
        analysis.weekday_days
    ));
    let palette = theme::current(ui);
    match &analysis.market_weekend {
        None => {
            ui.label(format!(
                "Effet week-end du marché : pas assez de données (il faut au moins {} jours de week-end et {} de semaine).",
                analysis::MIN_DAYS_EACH,
                analysis::MIN_DAYS_EACH
            ));
        }
        Some(w) => {
            let verdict = if w.significant() {
                "significatif"
            } else {
                "pas significatif (peut être du hasard)"
            };
            ui.horizontal(|ui| {
                ui.label("Effet week-end du marché :");
                let color = if w.diff_pct < 0.0 {
                    palette.gain
                } else {
                    palette.loss
                };
                ui.colored_label(
                    color,
                    egui::RichText::new(format!("{:+.1} %", w.diff_pct)).strong(),
                );
                ui.label(format!("le week-end, p = {:.3} → {verdict}.", w.p_value));
            });
        }
    }
}

/// Market price index per day, relative to each item's usual price.
fn day_chart(ui: &mut egui::Ui, analysis: &Analysis) {
    let palette = theme::current(ui);
    let first = analysis.days.first().map(|d| d.date);
    let bars: Vec<Bar> = analysis
        .days
        .iter()
        .map(|d| {
            let x = first.map_or(0, |f| (d.date - f).num_days()) as f64;
            let kind = if d.weekend { "week-end" } else { "semaine" };
            Bar::new(x, (d.index - 1.0) * 100.0)
                .width(0.7)
                .name(format!(
                    "{} ({kind}, {} objets)",
                    d.date.format("%a %d/%m"),
                    d.items
                ))
                .fill(if d.weekend {
                    palette.series[0]
                } else {
                    palette.series[1]
                })
        })
        .collect();
    let dates: Vec<_> = analysis.days.iter().map(|d| d.date).collect();
    Plot::new("days")
        .height(170.0)
        .show_grid([false, true])
        .grid_spacing(egui::Rangef::new(40.0, 300.0))
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .x_axis_formatter(move |mark, _| {
            let day = first.map(|f| f + chrono::Duration::days(mark.value.round() as i64));
            match day {
                Some(d) if (mark.value - mark.value.round()).abs() < 1e-6 && dates.contains(&d) => {
                    d.format("%d/%m").to_string()
                }
                _ => String::new(),
            }
        })
        .y_axis_formatter(|mark, _| format!("{:+.0} %", mark.value))
        .y_axis_label("vs prix habituel")
        .show(ui, |plot| {
            plot.bar_chart(BarChart::new("Niveau des prix", bars))
        });
    ui.small("Jaune : week-end · bleu : semaine. Barre = écart médian des objets ce jour-là par rapport à leur prix habituel.");
}
