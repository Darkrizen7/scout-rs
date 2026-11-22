//! Loot side bar: map button, search, then folders like the Objects categories: favorites, one per
//! continent (zones with the faction they belong to), dungeons and raids.

use eframe::egui;

use super::{Loot, map, zone_hint};
use crate::state::State;
use crate::ui::sidebar::{entry, entry_marked, rgb, tint};
use crate::ui::theme;
use scout_core::drops::Zone;
use scout_core::map::Faction;
use scout_core::search::Query;
use scout_worker::WorkerHandle;

/// A folder of the side bar.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Folder {
    Favorites,
    /// Continent, by rank (`map::continent_rank`).
    Continent(usize),
    Dungeons,
}

/// Dot after a zone: blue for the Alliance, red for the Horde, none when contested.
fn faction_dot(faction: Option<Faction>, dark: bool) -> Option<egui::Color32> {
    match faction? {
        Faction::Alliance => Some(tint(rgb(0x1c71d8), dark)),
        Faction::Horde => Some(tint(rgb(0xc01c28), dark)),
    }
}

impl Loot {
    pub(super) fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        zones: Option<&Vec<Zone>>,
        state: &mut State,
        worker: &WorkerHandle,
    ) {
        let on_map = self.show_map || self.zones.is_empty();
        let map_button = ui.add_sized(
            [ui.available_width(), 28.0],
            egui::Button::selectable(on_map, "🗺 Carte"),
        );
        if map_button.clicked() && !self.zones.is_empty() {
            self.show_map = !self.show_map;
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
        let game = std::sync::Arc::clone(&state.game);
        let query = Query::new(&self.search);
        let mut sorted: Vec<&Zone> = zones.iter().filter(|z| query.matches(&z.name)).collect();
        sorted
            .sort_by_cached_key(|z| (map::continent_rank(&game, z.ui_map), z.name.to_lowercase()));
        let palette = theme::current(ui);
        let searching = !self.search.trim().is_empty();
        let mut groups: Vec<(Folder, &str, String, Vec<&Zone>)> = vec![(
            Folder::Favorites,
            "★",
            "Favoris".to_owned(),
            sorted
                .iter()
                .copied()
                .filter(|z| state.zone_favorites.contains(&z.name))
                .collect(),
        )];
        for rank in 0..map::RANKS {
            let rows: Vec<&Zone> = sorted
                .iter()
                .copied()
                .filter(|z| !z.dungeon && map::continent_rank(&game, z.ui_map) == rank)
                .collect();
            groups.push((
                Folder::Continent(rank),
                "🗺",
                map::continent_name(&game, rank),
                rows,
            ));
        }
        groups.push((
            Folder::Dungeons,
            "🏰",
            "Donjons et raids".to_owned(),
            sorted.iter().copied().filter(|z| z.dungeon).collect(),
        ));
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (folder, glyph, title, rows) in groups {
                    // Empty folders stay out of the way, except favorites (their count tells how to start).
                    if rows.is_empty() && (folder != Folder::Favorites || searching) {
                        continue;
                    }
                    let color = match folder {
                        Folder::Favorites => palette.star,
                        _ => palette.accent,
                    };
                    let open = searching || !self.folded.contains(&folder);
                    if entry(ui, Some((glyph, color)), &title, Some(rows.len()), false, 0).clicked()
                        && !self.folded.remove(&folder)
                    {
                        self.folded.insert(folder);
                    }
                    if open {
                        for z in rows {
                            self.zone_row(ui, z, &game, state, worker);
                        }
                    }
                }
            });
    }

    /// One zone of a folder: click chooses it, Ctrl+click adds it to the chosen ones, right click
    /// adds it to / removes it from the favorites.
    fn zone_row(
        &mut self,
        ui: &mut egui::Ui,
        z: &Zone,
        game: &scout_core::gamedata::GameData,
        state: &mut State,
        worker: &WorkerHandle,
    ) {
        let dark = theme::current(ui).dark;
        let marker = (!z.dungeon).then(|| faction_dot(game.faction_of(&z.name), dark));
        let selected = self.zones.contains(&z.name);
        let response = entry_marked(
            ui,
            None,
            &z.name,
            Some(z.npcs as usize),
            selected,
            1,
            marker,
        )
        .on_hover_text(zone_hint(z, game.faction_of(&z.name)));
        if response.clicked() {
            let add = ui.input(|i| i.modifiers.command);
            let map_open = self.show_map;
            self.pick(z.name.clone(), add);
            self.show_map = map_open && add;
        }
        response.context_menu(|ui| {
            let favorite = state.zone_favorites.contains(&z.name);
            let label = if favorite {
                "★ Retirer des favoris"
            } else {
                "★ Ajouter aux favoris"
            };
            if ui.button(label).clicked() {
                state.toggle_zone_favorite(&z.name, worker);
                ui.close();
            }
        });
    }
}
