//! Reagents table of a recipe: bordered cells, each priced at the cheaper of merchant and auction
//! house (or crafted by the same profession, when cheaper), and a bold total row.

use eframe::egui::{self, Rect, Stroke, vec2};
use egui_extras::{Column, TableBuilder};

use scout_core::crafting::{self, Craft};
use scout_core::gamedata::{GameData, Recipe};
use scout_core::money;
use scout_core::pricing::Source;

use super::detail::{item_link, source_tag};
use crate::state::State;
use crate::ui::{coins, fonts, theme, widgets};

const PAD: f32 = 8.0;
const ROW: f32 = 28.0;

/// How a reagent is priced.
#[derive(Clone, Copy)]
enum Price {
    Buy(f64, Source),
    /// Crafted, cheaper than buying it (the cheapest purchase, if any).
    Craft(Craft, Option<(f64, Source)>),
}

impl Price {
    fn unit(self) -> f64 {
        match self {
            Price::Buy(unit, _) => unit,
            Price::Craft(craft, _) => craft.unit,
        }
    }
}

/// Cheapest price of a reagent: bought, or crafted by the recipe's profession.
fn price(game: &GameData, state: &State, profession: i64, item_id: i64) -> Option<Price> {
    let bought = state.buy_price(item_id);
    let crafted = crafting::craft_cost(game, item_id, profession, &|id| {
        state.buy_price(id).map(|(unit, _)| unit)
    });
    match (bought, crafted) {
        (Some(b), Some(c)) if c.unit < b.0 => Some(Price::Craft(c, Some(b))),
        (None, Some(c)) => Some(Price::Craft(c, None)),
        (b, _) => b.map(|(unit, source)| Price::Buy(unit, source)),
    }
}

/// "Craft" tag; the tooltip names the recipe and what buying would cost.
fn craft_tag(ui: &mut egui::Ui, game: &GameData, craft: Craft, bought: Option<(f64, Source)>) {
    let palette = theme::current(ui);
    let buy = match bought {
        Some((unit, Source::Merchant)) => {
            format!("{} chez un marchand", money::format(unit.round() as i64))
        }
        Some((unit, Source::Auction)) => format!("{} à l'HdV", money::format(unit.round() as i64)),
        None => "pas vu à l'HdV ni chez un marchand".to_owned(),
    };
    ui.label(egui::RichText::new("Craft").small().color(palette.star))
        .on_hover_text(format!(
            "Fabriqué : {}\nÀ l'achat : {buy}",
            game.recipes[craft.recipe].name
        ));
}

/// Draws the table; returns the total cost (of the reagents with a known price).
pub fn table(
    ui: &mut egui::Ui,
    game: &GameData,
    state: &State,
    r: &Recipe,
    next: &mut Option<i64>,
) -> i64 {
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
                        let best = price(game, state, r.profession, reagent.item_id);
                        match best {
                            Some(p) => total += p.unit() * reagent.count as f64,
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
                                    Some(p) => {
                                        coins::money(ui, p.unit().round() as i64, None);
                                        ui.add_space(6.0);
                                        match p {
                                            Price::Buy(_, source) => {
                                                source_tag(ui, state, reagent.item_id, source)
                                            }
                                            Price::Craft(c, bought) => {
                                                craft_tag(ui, game, c, bought)
                                            }
                                        }
                                    }
                                    None => {
                                        ui.weak("—");
                                    }
                                }
                            });
                            row.col(|ui| {
                                cell(ui, border, false, true, None);
                                let sub =
                                    best.map(|p| (p.unit() * reagent.count as f64).round() as i64);
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
    total.round() as i64
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
