//! Loot module: pick a zone or dungeon, see its monsters (filtered by name or by an item they drop, sorted by
//! level or by value per kill) and what the selected one drops.

mod npc;

use std::sync::Arc;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::State;
use crate::ui::sidebar::{entry, rgb, tint};
use crate::ui::{coins, theme};
use scout_core::drops::{DropKind, Zone, ZoneLoot, ZoneNpc};
use scout_core::gamedata::GameData;
use scout_core::pricing;
use scout_worker::WorkerHandle;

#[derive(Default)]
pub struct Loot {
    /// Zone search (side bar).
    search: String,
    zone: Option<String>,
    /// Monster or item search (monster list).
    filter: String,
    by_value: bool,
    /// Selected monster (NPC id).
    selection: Option<i64>,
    rows: Vec<Row>,
    /// What the rows were built for: zone data address, filter, sort, items revision, game data address.
    built_for: Option<(usize, String, bool, u64, usize)>,
}

/// A monster of the list.
struct Row {
    /// Index in `ZoneLoot::npcs`.
    index: usize,
    value: Option<f64>,
    /// Item matching the filter (not the monster's name) and its best chance.
    matched: Option<(String, Option<f64>)>,
}

/// What one unit of an item brings once looted (copper): auction price after the cut, or the merchant's.
pub fn unit_value(state: &State, game: &GameData, item_id: i64) -> Option<f64> {
    let sell = game.items.get(&item_id).map_or(0, |i| i.sell_price);
    pricing::sale_value(state.auction_price(item_id), sell)
}

impl Loot {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        let palette = theme::current(ui);
        let zones = state.zones(worker);
        egui::Panel::left("loot-zones")
            .exact_size(230.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(8, 10)),
            )
            .show(ui, |ui| self.sidebar(ui, zones.as_deref()));
        let loot = match &self.zone {
            Some(zone) => state.zone_loot(zone, worker),
            None => None,
        };
        let game = Arc::clone(&state.game);
        egui::Panel::left("loot-npcs")
            .exact_size(340.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| self.npcs(ui, loot.as_deref(), &game, state));
        let mut clicked = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                let npc = loot
                    .as_deref()
                    .and_then(|l| l.npcs.iter().find(|n| Some(n.npc.npc_id) == self.selection));
                match npc {
                    Some(npc) => clicked = npc::show(ui, npc, &game, state, palette),
                    None => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.weak("Choisis un monstre pour voir son butin.");
                        });
                    }
                }
            });
        if let Some(item) = clicked {
            state.show_item(item);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, zones: Option<&Vec<Zone>>) {
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("🔍 Zone ou donjon…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        let Some(zones) = zones else {
            ui.spinner();
            return;
        };
        if zones.is_empty() {
            ui.weak("Aucun butin en base : installe Forever Guide, puis ⚙ Réglages → Importer depuis Scout.");
            return;
        }
        let needle = self.search.trim().to_lowercase();
        let dark = theme::current(ui).dark;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (dungeon, title, icon, color) in [
                    (false, "Zones", "🗺", rgb(0x26a269)),
                    (true, "Donjons et raids", "🏰", rgb(0x9141ac)),
                ] {
                    ui.add_space(4.0);
                    ui.weak(title);
                    for z in zones.iter().filter(|z| {
                        z.dungeon == dungeon
                            && (needle.is_empty() || z.name.to_lowercase().contains(&needle))
                    }) {
                        let selected = self.zone.as_deref() == Some(z.name.as_str());
                        let response = entry(
                            ui,
                            Some((icon, tint(color, dark))),
                            &z.name,
                            Some(z.npcs as usize),
                            selected,
                            0,
                        )
                        .on_hover_text(zone_hint(z));
                        if response.clicked() && !selected {
                            self.zone = Some(z.name.clone());
                            self.selection = None;
                        }
                    }
                }
            });
    }

    fn npcs(&mut self, ui: &mut egui::Ui, loot: Option<&ZoneLoot>, game: &GameData, state: &State) {
        let Some(zone) = &self.zone else {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.weak("Choisis une zone ou un donjon à gauche.");
            });
            return;
        };
        ui.heading(zone);
        let Some(loot) = loot else {
            ui.spinner();
            return;
        };
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("🔍 Monstre ou objet…")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.weak("Trier par");
            ui.selectable_value(&mut self.by_value, false, "Niveau");
            ui.selectable_value(&mut self.by_value, true, "Valeur / kill")
                .on_hover_text(VALUE_HINT);
        });
        self.rebuild(loot, game, state);
        ui.weak(format!("{} monstres", self.rows.len()));
        let mut clicked = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().clip(true))
            .column(Column::exact(90.0))
            .body(|body| {
                body.rows(40.0, self.rows.len(), |mut row| {
                    let r = &self.rows[row.index()];
                    let npc = &loot.npcs[r.index];
                    row.set_selected(self.selection == Some(npc.npc.npc_id));
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.label(npc.name());
                            ui.weak(subtitle(npc, r));
                        });
                    });
                    row.col(|ui| {
                        if let Some(v) = r.value {
                            coins::money(ui, v.round() as i64, None);
                        }
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        clicked = Some(npc.npc.npc_id);
                    }
                });
            });
        if clicked.is_some() {
            self.selection = clicked;
        }
    }

    /// Monsters whose name or one of whose items matches the filter, in the chosen order.
    fn rebuild(&mut self, loot: &ZoneLoot, game: &GameData, state: &State) {
        let key = (
            loot as *const ZoneLoot as usize,
            self.filter.clone(),
            self.by_value,
            state.revision,
            game as *const GameData as usize,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let needle = self.filter.trim().to_lowercase();
        self.rows = loot
            .npcs
            .iter()
            .enumerate()
            .filter_map(|(index, npc)| {
                let matched = if needle.is_empty() || npc.name().to_lowercase().contains(&needle) {
                    None
                } else {
                    let drop = npc
                        .drops
                        .iter()
                        .find(|d| game.item_name(d.item_id).to_lowercase().contains(&needle))?;
                    Some((game.item_name(drop.item_id), drop.best_chance()))
                };
                Some(Row {
                    index,
                    value: npc.value_per_kill(|id| unit_value(state, game, id)),
                    matched,
                })
            })
            .collect();
        if self.by_value {
            // Highest value first, monsters without a priced drop last (stable: by level among them).
            self.rows.sort_by(|a, b| match (a.value, b.value) {
                (Some(x), Some(y)) => y.total_cmp(&x),
                (x, y) => y.is_some().cmp(&x.is_some()),
            });
        }
        self.built_for = Some(key);
    }
}

/// Explains the value per kill (sort button and monster sheet).
pub const VALUE_HINT: &str = "Estimation par corps fouillé : chance (réelle à partir de 20 corps, sinon \
Forever Guide) × quantité moyenne × prix (HdV après commission, ou marchand si plus cher). \
Sans l'argent ramassé, le dépeçage ni le butin de quête.";

/// Second line of a monster: the matching item and its chance, or levels and observed kills.
fn subtitle(npc: &ZoneNpc, row: &Row) -> String {
    if let Some((item, chance)) = &row.matched {
        let chance = chance.map_or_else(|| "?".to_owned(), scout_core::drops::format_percent);
        return format!("{item} · {chance}");
    }
    let mut text = npc.npc.levels();
    if text.is_empty() {
        text = "niveau inconnu".to_owned();
    }
    if npc.kills > 0 {
        text += &format!(" · {} kills", npc.kills);
    }
    let skinnable = npc.of(DropKind::Skinning).next().is_some();
    if skinnable {
        text += " · dépeçable";
    }
    text
}

fn zone_hint(z: &Zone) -> String {
    let mut text = format!("{} monstres", z.npcs);
    if let (Some(a), Some(b)) = (z.min_level, z.max_level) {
        text = format!("Niveaux {a}-{b} · {text}");
    }
    if z.kills > 0 {
        text += &format!(" · {} corps fouillés", z.kills);
    }
    text
}
