//! Merchants tab of the item sheet: buy and sell prices, and the NPCs known to sell it.

use eframe::egui;

use crate::state::State;
use crate::ui::coins;
use crate::ui::widgets::{self, money_or_dash};
use scout_core::map;

pub fn show(ui: &mut egui::Ui, state: &State, sell_price: i64, id: i64) {
    egui::Grid::new("sheet-merchants")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Achat chez un marchand");
            match state.vendor.price(id) {
                Some(p) => {
                    coins::money(ui, p.round() as i64, None);
                }
                None => {
                    ui.weak("pas vendu par un PNJ connu");
                }
            }
            ui.end_row();
            ui.label("Revente au marchand");
            money_or_dash(ui, Some(sell_price).filter(|&p| p > 0));
            ui.end_row();
            ui.label("Prix HdV");
            money_or_dash(ui, state.auction_price(id));
            ui.end_row();
        });
    ui.add_space(10.0);
    let merchants = state.vendor.merchants(id);
    if merchants.is_empty() {
        if state.vendor.price(id).is_some() {
            ui.weak("Vendu par un marchand, mais on ne sait pas encore lequel : ouvre-le en jeu avec Scout chargé.");
        }
        return;
    }
    ui.strong(format!("Vendu par ({})", merchants.len()));
    for m in merchants {
        ui.horizontal(|ui| {
            let ui_map = m
                .zone
                .as_deref()
                .and_then(|z| map::find_zone(&state.game.maps, z));
            let name = m.npc.as_deref().unwrap_or("");
            widgets::place_label(ui, &m.place(), map::waypoint(ui_map, m.x, m.y, name));
            coins::money(ui, m.price.round() as i64, None);
            if m.limited {
                ui.weak("· stock limité");
            }
        });
    }
}
