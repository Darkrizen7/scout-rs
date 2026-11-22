//! Loot module: pick zones or dungeons (Ctrl+click adds one), see their monsters together (filtered by name or
//! by an item they drop, sorted by level, value per kill or value per level) and what the selected one drops.

mod map;
mod npc;
mod npcs;
mod sidebar;

use std::sync::Arc;

use eframe::egui;

use crate::state::State;
use crate::ui::theme;
use scout_core::drops::{Zone, ZoneLoot};
use scout_core::gamedata::GameData;
use scout_core::pricing;
use scout_worker::WorkerHandle;

#[derive(Default)]
pub struct Loot {
    /// Zone search (side bar).
    search: String,
    /// Side bar folders opened (all folded by default).
    opened: std::collections::HashSet<sidebar::Folder>,
    /// Chosen zones, in the order they were added.
    zones: Vec<String>,
    /// Their monsters together, and the zone data it was made from.
    merged: Option<(Vec<usize>, Arc<ZoneLoot>)>,
    /// Monster or item search (monster list).
    filter: String,
    /// Active sort and whether it is descending; `None` keeps the natural (level) order.
    sort: Option<(npcs::Sort, bool)>,
    /// Elites, rares and bosses hidden.
    normal_only: bool,
    /// Selected monster (NPC id).
    selection: Option<i64>,
    rows: Vec<npcs::Row>,
    /// Map shown instead of the monsters (always when no zone is chosen).
    show_map: bool,
    map: map::WorldMap,
    /// What the rows were built for: zone data address, filter, sort, items revision, game data address.
    built_for: Option<npcs::BuiltFor>,
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
            .show(ui, |ui| self.sidebar(ui, zones.as_deref(), state, worker));
        let game = Arc::clone(&state.game);
        if self.show_map || self.zones.is_empty() {
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
                        picked = self.map.show(ui, &game, zones, masks, &self.zones, palette);
                    }
                    _ => {
                        ui.spinner();
                    }
                });
            if let Some(zone) = picked {
                // Ctrl+click picks several zones and stays on the map.
                let add = ui.input(|i| i.modifiers.command);
                self.pick(zone, add);
                self.show_map = add;
            }
            return;
        }
        let loot = self.loot(state, worker);
        egui::Panel::left("loot-npcs")
            .exact_size(375.0)
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

    /// Chooses a zone (from the list or the map): alone, or added to / removed from the chosen ones.
    fn pick(&mut self, zone: String, add: bool) {
        if !add {
            self.zones.clear();
        }
        match self.zones.iter().position(|z| *z == zone) {
            Some(i) if add => {
                self.zones.remove(i);
            }
            Some(_) => {}
            None => self.zones.push(zone),
        }
        self.show_map = false;
    }

    /// Monsters of the chosen zones once all are loaded (asked from the worker otherwise).
    fn loot(&mut self, state: &mut State, worker: &WorkerHandle) -> Option<Arc<ZoneLoot>> {
        let parts: Option<Vec<Arc<ZoneLoot>>> = self
            .zones
            .iter()
            .map(|z| state.zone_loot(z, worker))
            .collect();
        let parts = parts?;
        if let [one] = parts.as_slice() {
            return Some(Arc::clone(one));
        }
        let key: Vec<usize> = parts.iter().map(|p| Arc::as_ptr(p) as usize).collect();
        if self.merged.as_ref().is_none_or(|(k, _)| *k != key) {
            let merged = ZoneLoot::merge(parts.iter().map(|p| p.as_ref()));
            self.merged = Some((key, Arc::new(merged)));
        }
        self.merged.as_ref().map(|(_, m)| Arc::clone(m))
    }
}

/// Explains the value per kill (sort button and monster sheet).
pub const VALUE_HINT: &str = "Estimation par corps fouillé : chance (réelle à partir de 20 corps, sinon \
Forever Guide) × quantité moyenne × prix (HdV après commission, ou marchand si plus cher). \
Sans l'argent ramassé, le dépeçage ni le butin de quête.";

fn zone_hint(z: &Zone, faction: Option<scout_core::map::Faction>) -> String {
    let mut text = format!("{} monstres", z.npcs);
    if !z.dungeon {
        text = format!("{} · {text}", faction.map_or("Contestée", |f| f.name()));
    }
    if let (Some(a), Some(b)) = (z.min_level, z.max_level) {
        text = format!("Niveaux {a}-{b} · {text}");
    }
    if z.kills > 0 {
        text += &format!(" · {} corps fouillés", z.kills);
    }
    text + "\nCtrl+clic : ajouter à la sélection · Clic droit : favori"
}
