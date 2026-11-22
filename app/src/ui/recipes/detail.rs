//! Recipe detail: product, skill, reagents priced at the cheaper of merchant and auction house, pattern.

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};

use scout_core::gamedata::{GameData, Recipe};
use scout_core::pricing::Source;

use crate::state::State;
use crate::ui::{coins, theme, widgets};

/// Auction house cut on sales, as in the calculator.
const AH_CUT: f64 = 0.05;

/// Shows a recipe; returns the item clicked (opened in the Objects module).
pub fn show(ui: &mut egui::Ui, game: &GameData, state: &State, recipe_index: usize) -> Option<i64> {
    let mut next = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            recipe(ui, game, state, &game.recipes[recipe_index], &mut next)
        });
    next
}

/// "PNJ" / "HdV" tag; for a merchant, the tooltip lists who sells it.
fn source_tag(ui: &mut egui::Ui, state: &State, item_id: i64, source: Source) {
    let palette = theme::current(ui);
    let (text, color, hint) = match source {
        Source::Merchant => (
            "PNJ",
            palette.gain,
            "Vendu par un marchand (prix vu par Scout ou liste de Forever Guide)",
        ),
        Source::Auction => (
            "HdV",
            palette.accent,
            "Dernier prix vu à l'hôtel des ventes",
        ),
    };
    let merchants = match source {
        Source::Merchant => state.vendor.merchants(item_id),
        Source::Auction => Vec::new(),
    };
    let hint = if merchants.is_empty() {
        hint.to_owned()
    } else {
        let list: Vec<String> = merchants
            .iter()
            .map(|m| format!("• {}", m.place()))
            .collect();
        format!("Vendu par :\n{}", list.join("\n"))
    };
    ui.label(RichText::new(text).small().color(color))
        .on_hover_text(hint);
}

/// An item name as a link, in its quality color.
fn item_link(ui: &mut egui::Ui, game: &GameData, item_id: i64, next: &mut Option<i64>) {
    let dark = ui.visuals().dark_mode;
    let mut text = RichText::new(game.item_name(item_id));
    if let Some(color) = game
        .items
        .get(&item_id)
        .and_then(|i| widgets::quality_color(i.quality, dark))
    {
        text = text.color(color);
    }
    if ui.link(text).on_hover_text("Voir l'objet").clicked() {
        *next = Some(item_id);
    }
}

fn recipe(ui: &mut egui::Ui, game: &GameData, state: &State, r: &Recipe, next: &mut Option<i64>) {
    let palette = theme::current(ui);
    ui.heading(&r.name);
    ui.horizontal(|ui| {
        ui.weak(game.profession_name(r.profession));
        ui.weak("·");
        if let Some(required) = r.required {
            ui.colored_label(
                Color32::from_rgb(0xe6, 0x61, 0x00),
                format!("requis {required}"),
            );
        }
        ui.colored_label(
            Color32::from_rgb(0xc8, 0x9b, 0x00),
            format!("jaune {}", r.yellow),
        );
        ui.colored_label(palette.dim_text, format!("gris {}", r.grey));
    });
    ui.add_space(10.0);

    if let Some(made) = r.creates {
        ui.horizontal(|ui| {
            ui.strong("Produit :");
            item_link(ui, game, made.item_id, next);
            if made.count > 1 {
                ui.label(format!("× {}", made.count));
            }
        });
    } else {
        ui.weak("Ne produit pas d'objet (enchantement ou effet).");
    }
    ui.add_space(8.0);

    ui.strong("Composants");
    let mut total = 0.0;
    let mut missing = false;
    TableBuilder::new(ui)
        .id_salt("reagents")
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(180.0).clip(true))
        .column(Column::exact(50.0))
        .column(Column::exact(150.0))
        .column(Column::exact(130.0))
        .header(22.0, |mut h| {
            for title in [
                "Objet",
                "Qté",
                "Prix unitaire (le moins cher)",
                "Sous-total",
            ] {
                h.col(|ui| {
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
                body.row(26.0, |mut row| {
                    row.col(|ui| item_link(ui, game, reagent.item_id, next));
                    row.col(|ui| {
                        ui.label(format!("× {}", reagent.count));
                    });
                    row.col(|ui| match best {
                        Some((unit, source)) => {
                            coins::money(ui, unit.round() as i64, None);
                            source_tag(ui, state, reagent.item_id, source);
                        }
                        None => {
                            ui.weak("—");
                        }
                    });
                    row.col(|ui| {
                        widgets::money_or_dash(
                            ui,
                            best.map(|(unit, _)| (unit * reagent.count as f64).round() as i64),
                        );
                    });
                });
            }
        });
    let total = total.round() as i64;
    ui.add_space(6.0);
    egui::Grid::new("recipe-cost")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("Coût des composants");
            ui.horizontal(|ui| {
                coins::money(ui, total, None);
                if missing {
                    ui.weak("(sans les composants sans prix connu)");
                }
            });
            ui.end_row();
            let Some(made) = r.creates else { return };
            let auction = state
                .auction_price(made.item_id)
                .map(|unit| (unit as f64 * made.count as f64 * (1.0 - AH_CUT)).round() as i64);
            let merchant = game
                .items
                .get(&made.item_id)
                .map(|i| i.sell_price * made.count)
                .filter(|&p| p > 0);
            if let Some(sale) = auction {
                ui.label("Vente à l'HdV (après 5 %)");
                coins::money(ui, sale, None);
                ui.end_row();
            }
            if let Some(sale) = merchant {
                ui.label("Revente au marchand");
                coins::money(ui, sale, None);
                ui.end_row();
            }
            if let Some(best) = auction.max(merchant) {
                let profit = best - total;
                let via = if auction == Some(best) {
                    "en vendant à l'HdV"
                } else {
                    "en revendant au marchand"
                };
                ui.strong("Bénéfice par craft");
                ui.horizontal(|ui| {
                    coins::money(
                        ui,
                        profit,
                        Some(if profit >= 0 {
                            palette.gain
                        } else {
                            palette.loss
                        }),
                    );
                    ui.weak(via);
                });
                ui.end_row();
            }
        });

    ui.add_space(12.0);
    ui.strong("Apprentissage");
    if r.taught_by.is_empty() {
        ui.weak("Pas d'objet de recette : maître de métier (ou autre source).");
    }
    for &taught in &r.taught_by {
        ui.horizontal(|ui| {
            item_link(ui, game, taught, next);
            match state.buy_price(taught) {
                Some((p, source)) => {
                    ui.weak("·");
                    coins::money(ui, p.round() as i64, None);
                    source_tag(ui, state, taught, source);
                }
                None => {
                    ui.weak("· pas vu à l'HdV ni chez un marchand");
                }
            }
        });
    }
}
