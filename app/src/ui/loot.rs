//! Loot module: pick a zone or dungeon, see its monsters (filtered by name or by an item they drop, sorted by
//! level or by value per kill) and what the selected one drops.

mod map;
mod npc;
mod npcs;

use std::sync::Arc;

use eframe::egui;

use crate::state::State;
use crate::ui::sidebar::{entry, rgb, tint};
use crate::ui::theme;
use scout_core::drops::Zone;
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
    /// Elites, rares and bosses hidden.
    normal_only: bool,
    /// Selected monster (NPC id).
    selection: Option<i64>,
    rows: Vec<npcs::Row>,
    /// Map shown instead of the monsters (always when no zone is chosen).
    show_map: bool,
    map: map::WorldMap,
    /// What the rows were built for: zone data address, filter, sort, items revision, game data address.
    built_for: Option<(usize, String, bool, bool, u64, usize)>,
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
            .show(ui, |ui| self.sidebar(ui, zones.as_deref(), &state.game));
        let game = Arc::clone(&state.game);
        if self.show_map || self.zone.is_none() {
            let masks = state.map_masks(worker);
            let mut picked = None;
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(palette.view)
                        .inner_margin(egui::Margin::same(16)),
                )
                .show(ui, |ui| match (&zones, &masks) {
                    (Some(zones), Some(masks)) => {
                        picked =
                            self.map
                                .show(ui, &game, zones, masks, self.zone.as_deref(), palette);
                    }
                    _ => {
                        ui.spinner();
                    }
                });
            if let Some(zone) = picked {
                self.pick(zone);
            }
            return;
        }
        let loot = match &self.zone {
            Some(zone) => state.zone_loot(zone, worker),
            None => None,
        };
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

    /// Opens a zone's monsters (from the list or the map).
    fn pick(&mut self, zone: String) {
        if self.zone.as_ref() != Some(&zone) {
            self.selection = None;
        }
        self.zone = Some(zone);
        self.show_map = false;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, zones: Option<&Vec<Zone>>, game: &GameData) {
        let on_map = self.show_map || self.zone.is_none();
        let map = ui.add_sized(
            [ui.available_width(), 28.0],
            egui::Button::selectable(on_map, "🗺 Carte"),
        );
        if map.clicked() && self.zone.is_some() {
            self.show_map = !self.show_map;
            let ui_map = zones.and_then(|z| z.iter().find(|z| Some(&z.name) == self.zone.as_ref()));
            self.map.focus(game, ui_map.and_then(|z| z.ui_map));
        }
        ui.add_space(6.0);
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
                        if response.clicked() {
                            self.map.focus(game, z.ui_map);
                            self.pick(z.name.clone());
                        }
                    }
                }
            });
    }
}

/// Explains the value per kill (sort button and monster sheet).
pub const VALUE_HINT: &str = "Estimation par corps fouillé : chance (réelle à partir de 20 corps, sinon \
Forever Guide) × quantité moyenne × prix (HdV après commission, ou marchand si plus cher). \
Sans l'argent ramassé, le dépeçage ni le butin de quête.";

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
