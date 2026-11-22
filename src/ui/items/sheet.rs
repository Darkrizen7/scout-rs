//! Item sheet: header (name in quality color, category, levels, favorite) and tabs:
//! Market (auction prices and chart), Recipes (made by, used in, teaches), Merchants, Drops (to come).

use eframe::egui::{self, RichText};

use crate::gamedata::GameData;
use crate::state::State;
use crate::ui::chart::{self, format_datetime};
use crate::ui::coins;
use crate::ui::item_list::star_button;
use crate::ui::recipes::detail::{merchant_place, money_or_dash, price, quality_color};
use crate::ui::theme;
use crate::worker::WorkerHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Market,
    Recipes,
    Merchants,
    Drops,
}

/// A link clicked in the sheet.
pub enum Link {
    Item(i64),
    Recipe(usize),
}

pub struct Sheet {
    pub item_id: i64,
    tab: Tab,
}

impl Sheet {
    /// Opens on the most useful tab, and asks for the auction history if the item has one.
    pub fn new(item_id: i64, state: &mut State, worker: &WorkerHandle) -> Self {
        let game = &state.game;
        let tab = if state.item(item_id).is_some() {
            Tab::Market
        } else if game.made_by.contains_key(&item_id)
            || game.used_in.contains_key(&item_id)
            || game.teaches.contains_key(&item_id)
        {
            Tab::Recipes
        } else if state.vendor.price(item_id).is_some() {
            Tab::Merchants
        } else {
            Tab::Market
        };
        if state.item(item_id).is_some() {
            state.select(item_id, worker);
        }
        Self { item_id, tab }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        state: &mut State,
        worker: &WorkerHandle,
    ) -> Option<Link> {
        let id = self.item_id;
        let palette = theme::current(ui);
        let info = game.items.get(&id);

        let mut toggle = false;
        ui.horizontal(|ui| {
            toggle = star_button(ui, state.is_favorite(id), palette).clicked();
            let mut title = RichText::new(game.item_name(id)).heading();
            if let Some(color) = info.and_then(|i| quality_color(i.quality, palette.dark)) {
                title = title.color(color);
            }
            ui.label(title);
        });
        if toggle {
            state.toggle_favorite(id, worker);
        }
        ui.horizontal(|ui| {
            ui.weak(format!("n° {id}"));
            if let Some(item) = info {
                let category = super::category(game, item);
                if !category.is_empty() {
                    ui.weak(format!("· {category}"));
                }
                if item.item_level > 0 {
                    ui.weak(format!("· niveau d'objet {}", item.item_level));
                }
                if item.required_level > 0 {
                    ui.weak(format!("· niveau requis {}", item.required_level));
                }
            }
        });
        ui.add_space(8.0);

        let recipes = [&game.made_by, &game.used_in, &game.teaches]
            .iter()
            .map(|m| m.get(&id).map_or(0, Vec::len))
            .sum::<usize>();
        let merchants = state.vendor.merchants(id).len();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Market, "📈 Market");
            ui.selectable_value(
                &mut self.tab,
                Tab::Recipes,
                format!("🔨 Recettes ({recipes})"),
            );
            ui.selectable_value(
                &mut self.tab,
                Tab::Merchants,
                format!("🛒 PNJ ({merchants})"),
            );
            ui.selectable_value(&mut self.tab, Tab::Drops, "💀 Butin");
        });
        ui.separator();

        let mut link = None;
        match self.tab {
            Tab::Market => market(ui, state, id),
            Tab::Recipes => {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        if recipes == 0 {
                            ui.weak(
                                "Aucune recette ne produit, n'utilise ni n'enseigne cet objet.",
                            );
                        }
                        recipe_list(
                            ui,
                            game,
                            id,
                            "Produit par",
                            game.made_by.get(&id),
                            false,
                            &mut link,
                        );
                        recipe_list(
                            ui,
                            game,
                            id,
                            "Utilisé dans",
                            game.used_in.get(&id),
                            true,
                            &mut link,
                        );
                        recipe_list(
                            ui,
                            game,
                            id,
                            "Enseigne",
                            game.teaches.get(&id),
                            false,
                            &mut link,
                        );
                    });
            }
            Tab::Merchants => merchants_tab(ui, state, info.map_or(0, |i| i.sell_price), id),
            Tab::Drops => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.weak("Bientôt : les monstres qui lâchent cet objet, et avec quelle chance.");
                });
            }
        }
        link
    }
}

fn market(ui: &mut egui::Ui, state: &State, id: i64) {
    let Some(item) = state.item(id) else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.weak("Jamais vu à l'hôtel des ventes (ni par Scout, ni par Auctionator ou Forever Guide).");
        });
        return;
    };
    egui::Grid::new("sheet-market")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Dernier prix");
            money_or_dash(ui, item.price);
            ui.end_row();
            ui.label("En vente");
            ui.label(item.quantity.map_or_else(|| "—".into(), |q| q.to_string()));
            ui.end_row();
            ui.label("Dernier relevé");
            ui.label(format_datetime(item.ts));
            ui.end_row();
            ui.label("Relevés");
            ui.label(item.scans.to_string());
            ui.end_row();
        });
    ui.add_space(8.0);
    match &state.history {
        Some((history_id, history)) if *history_id == id => chart::plots(ui, history),
        _ => {
            ui.spinner();
        }
    }
}

fn merchants_tab(ui: &mut egui::Ui, state: &State, sell_price: i64, id: i64) {
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
            money_or_dash(ui, price(state, id));
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
            ui.label(merchant_place(m));
            coins::money(ui, m.price.round() as i64, None);
            if m.limited {
                ui.weak("· stock limité");
            }
        });
    }
}

/// Recipes as links, with what they make; with `reagent`, how many of the item each one needs.
fn recipe_list(
    ui: &mut egui::Ui,
    game: &GameData,
    item_id: i64,
    title: &str,
    list: Option<&Vec<usize>>,
    reagent: bool,
    link: &mut Option<Link>,
) {
    let Some(list) = list.filter(|l| !l.is_empty()) else {
        return;
    };
    ui.strong(format!("{title} ({})", list.len()));
    let mut sorted = list.clone();
    sorted.sort_by_key(|&i| {
        (
            game.recipes[i].profession,
            game.recipes[i].required.unwrap_or(0),
        )
    });
    for i in sorted {
        let r = &game.recipes[i];
        ui.horizontal(|ui| {
            if ui
                .link(&r.name)
                .on_hover_text("Ouvrir dans l'onglet Recettes")
                .clicked()
            {
                *link = Some(Link::Recipe(i));
            }
            if let Some(reagent) = r.reagents.iter().find(|x| reagent && x.item_id == item_id) {
                ui.label(format!("× {}", reagent.count));
            }
            // What the recipe makes, when it is not this item (used in / teaches).
            if let Some(made) = r.creates.filter(|c| c.item_id != item_id)
                && ui
                    .link(format!("→ {}", game.item_name(made.item_id)))
                    .clicked()
            {
                *link = Some(Link::Item(made.item_id));
            }
            let skill = r.required.map_or_else(String::new, |s| format!(" {s}"));
            ui.weak(format!("{}{skill}", game.profession_name(r.profession)));
        });
    }
    ui.add_space(8.0);
}
