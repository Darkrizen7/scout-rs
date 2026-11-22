//! Reagents table of a recipe: bordered cells, each priced at the cheaper of merchant and auction
//! house, and a bold total row.

use eframe::egui::{self, Rect, Stroke, vec2};
use egui_extras::{Column, TableBuilder};

use scout_core::gamedata::{GameData, Recipe};

use super::detail::{item_link, source_tag};
use crate::state::State;
use crate::ui::{coins, fonts, theme, widgets};

const PAD: f32 = 8.0;
const ROW: f32 = 28.0;

/// Draws the table; returns the total cost and whether some reagent has no known price.
pub fn table(
    ui: &mut egui::Ui,
    game: &GameData,
    state: &State,
    r: &Recipe,
    next: &mut Option<i64>,
) -> (i64, bool) {
    let palette = theme::current(ui);
    let border = Stroke::new(1.0, palette.border);
    let mut total = 0.0;
    let mut missing = false;
    egui::Frame::new()
        .stroke(border)
        .corner_radius(4.0)
        .show(ui, |ui| {
            // Zero spacing so the cell borders touch; cells pad themselves.
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            TableBuilder::new(ui)
                .id_salt("reagents")
                .vscroll(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::remainder().at_least(180.0).clip(true))
                .column(Column::exact(60.0))
                .column(Column::exact(200.0))
                .column(Column::exact(130.0))
                .header(26.0, |mut h| {
                    let titles = [
                        "Objet",
                        "Qté",
                        "Prix unitaire (le moins cher)",
                        "Sous-total",
                    ];
                    for (i, title) in titles.into_iter().enumerate() {
                        h.col(|ui| {
                            cell(ui, border, i < 3, true, Some(palette.widget));
                            ui.weak(title);
                        });
                    }
                })
                .body(|mut body| {
                    for reagent in &r.reagents {
                        let best = state.buy_price(reagent.item_id);
                        match best {
                            Some((unit, _)) => total += unit * reagent.count as f64,
                            None => missing = true,
                        }
                        body.row(ROW, |mut row| {
                            row.col(|ui| {
                                cell(ui, border, true, true, None);
                                item_link(ui, game, reagent.item_id, next);
                            });
                            row.col(|ui| {
                                cell(ui, border, true, true, None);
                                ui.label(format!("× {}", reagent.count));
                            });
                            row.col(|ui| {
                                cell(ui, border, true, true, None);
                                match best {
                                    Some((unit, source)) => {
                                        coins::money(ui, unit.round() as i64, None);
                                        ui.add_space(6.0);
                                        source_tag(ui, state, reagent.item_id, source);
                                    }
                                    None => {
                                        ui.weak("—");
                                    }
                                }
                            });
                            row.col(|ui| {
                                cell(ui, border, false, true, None);
                                let sub = best
                                    .map(|(unit, _)| (unit * reagent.count as f64).round() as i64);
                                widgets::money_or_dash(ui, sub);
                            });
                        });
                    }
                    body.row(ROW, |mut row| {
                        let fill = Some(palette.widget);
                        row.col(|ui| {
                            cell(ui, border, true, false, fill);
                            ui.label(fonts::bold("Total"));
                        });
                        row.col(|ui| cell(ui, border, true, false, fill));
                        row.col(|ui| {
                            cell(ui, border, true, false, fill);
                            if missing {
                                ui.weak("sans les prix inconnus");
                            }
                        });
                        row.col(|ui| {
                            cell(ui, border, false, false, fill);
                            coins::money_bold(ui, total.round() as i64);
                        });
                    });
                });
        });
    (total.round() as i64, missing)
}

/// Paints the cell background and its right / bottom borders, then pads the content.
fn cell(ui: &mut egui::Ui, border: Stroke, right: bool, bottom: bool, fill: Option<egui::Color32>) {
    let rect: Rect = ui.max_rect();
    let painter = ui.painter();
    if let Some(color) = fill {
        painter.rect_filled(rect, 0.0, color);
    }
    if right {
        painter.vline(rect.right(), rect.y_range(), border);
    }
    if bottom {
        painter.hline(rect.x_range(), rect.bottom(), border);
    }
    ui.add_space(PAD);
}
