//! Item sheet: header (name in quality color, category, levels, favorite) and tabs:
//! Market (auction prices and chart), Recipes (made by, used in, teaches), Merchants, Drops (`drops.rs`).

use eframe::egui::{self, RichText};

use crate::state::State;
use scout_core::gamedata::GameData;
use scout_core::time::format_datetime;

use super::{drops, merchants};
use crate::ui::chart;
use crate::ui::theme;
use crate::ui::widgets::{money_or_dash, quality_color, star_button};
use scout_worker::WorkerHandle;

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
    /// Drops tab: sort and sections shown in full.
    drops_view: drops::DropsView,
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
        Self {
            item_id,
            tab,
            drops_view: drops::DropsView::default(),
        }
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
        let drops = state.item_drops(id, worker).map(drops::count);
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
            let title = drops.map_or_else(|| "💀 Butin".to_owned(), |n| format!("💀 Butin ({n})"));
            ui.selectable_value(&mut self.tab, Tab::Drops, title);
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
            Tab::Merchants => merchants::show(ui, state, info.map_or(0, |i| i.sell_price), id),
            Tab::Drops => match state.item_drops(id, worker) {
                Some(d) => {
                    link = drops::show(ui, d, &mut self.drops_view, palette).map(Link::Item);
                }
                None => {
                    ui.spinner();
                }
            },
        }
        link
    }
}

fn market(ui: &mut egui::Ui, state: &State, id: i64) {
    let Some(item) = state.item(id) else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.weak("Jamais vu à l'hôtel des ventes (ni par Scout ni par le serveur de prix).");
        });
        return;
    };
    ui.horizontal(|ui| {
        ui.label("Dernier prix");
        money_or_dash(ui, item.price);
        ui.add_space(16.0);
        ui.label("Volume");
        ui.label(item.quantity.map_or_else(|| "—".into(), |q| q.to_string()));
    });
    ui.weak(format!(
        "Dernier relevé : {} · Relevés : {}",
        format_datetime(item.ts),
        item.scans
    ));
    if let Some(rate) = state.sales.rate(id) {
        ui.horizontal(|ui| {
            let per_day = format!("{:.1}", rate.per_day).replace('.', ",");
            ui.label(format!(
                "Se vend ~{per_day} / jour ({} j)",
                state.sales.days()
            ));
            if let Some(mean) = rate.mean_unit {
                ui.add_space(16.0);
                ui.label("Prix moyen des ventes :");
                money_or_dash(ui, Some(mean.round() as i64));
            }
        })
        .response
        .on_hover_text(
            "Ventes vues par le serveur de prix (les ventes estimées comptent pour moitié : \
             une annonce disparue peut être annulée). Le prix moyen est sous-estimé quand des \
             ventes n'ont pas de prix.",
        );
    }
    ui.add_space(8.0);
    match &state.history {
        Some((history_id, history)) if *history_id == id => chart::plots(ui, id, history, false),
        _ => {
            ui.spinner();
        }
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
