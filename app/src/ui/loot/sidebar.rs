//! Loot side bar: map button, search, then folders like the Objects categories: favorites, one per
//! continent (zones with the faction they belong to), dungeons and raids.

use std::sync::{Arc, Weak};

use eframe::egui;

use super::{Loot, zone_hint};
use crate::state::State;
use crate::ui::sidebar::{Marker, entry, entry_marked, rgb, tint};
use crate::ui::theme;
use scout_core::drops::Zone;
use scout_core::gamedata::GameData;
use scout_core::map::{self, Faction};
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

/// A zone of the side bar, with what its row needs worked out once.
struct ListedZone {
    /// Index in the zone list.
    index: usize,
    /// Continent rank (`map::continent_rank`).
    rank: usize,
    faction: Option<Faction>,
}

/// The zones matching the search, by continent then name, and what they were made from: the
/// zone list, the search and the game data (weak references: they free the data but keep the
/// addresses from being reused, so a new list is never taken for the old one).
#[derive(Default)]
pub struct Listed {
    zones: Vec<ListedZone>,
    built_for: Option<(Weak<Vec<Zone>>, String, Weak<GameData>)>,
}

impl Listed {
    fn rebuild(&mut self, zones: &Arc<Vec<Zone>>, game: &Arc<GameData>, search: &str) {
        if self.built_for.as_ref().is_some_and(|(z, s, g)| {
            Weak::ptr_eq(z, &Arc::downgrade(zones))
                && s == search
                && Weak::ptr_eq(g, &Arc::downgrade(game))
        }) {
            return;
        }
        let query = Query::new(search);
        let mut listed: Vec<(ListedZone, String)> = zones
            .iter()
            .enumerate()
            .filter(|(_, z)| query.matches(&z.name))
            .map(|(index, z)| {
                let zone = ListedZone {
                    index,
                    rank: map::continent_rank(&game.maps, z.ui_map),
                    faction: game.faction_of(&z.name),
                };
                (zone, z.name.to_lowercase())
            })
            .collect();
        listed.sort_by(|(a, an), (b, bn)| (a.rank, an).cmp(&(b.rank, bn)));
        self.zones = listed.into_iter().map(|(zone, _)| zone).collect();
        self.built_for = Some((
            Arc::downgrade(zones),
            search.to_owned(),
            Arc::downgrade(game),
        ));
    }
}

/// Dot after a zone: blue for the Alliance, red for the Horde, none when contested.
fn faction_dot(faction: Option<Faction>, dark: bool) -> Marker {
    match faction {
        Some(Faction::Alliance) => Marker::Dot(tint(rgb(0x1c71d8), dark)),
        Some(Faction::Horde) => Marker::Dot(tint(rgb(0xc01c28), dark)),
        None => Marker::Blank,
    }
}

impl Loot {
    pub(super) fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        zones: Option<&Arc<Vec<Zone>>>,
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
        let game = Arc::clone(&state.game);
        self.listed.rebuild(zones, &game, &self.search);
        let palette = theme::current(ui);
        let searching = !self.search.trim().is_empty();
        let listed = std::mem::take(&mut self.listed.zones);
        let zone = |l: &ListedZone| &zones[l.index];
        let mut groups: Vec<(Folder, &str, String, Vec<&ListedZone>)> = vec![(
            Folder::Favorites,
            "★",
            "Favoris".to_owned(),
            listed
                .iter()
                .filter(|l| state.zone_favorites.contains(&zone(l).name))
                .collect(),
        )];
        for rank in 0..map::RANKS {
            let rows: Vec<&ListedZone> = listed
                .iter()
                .filter(|l| !zone(l).dungeon && l.rank == rank)
                .collect();
            groups.push((
                Folder::Continent(rank),
                "🗺",
                map::continent_name(&game.maps, rank),
                rows,
            ));
        }
        groups.push((
            Folder::Dungeons,
            "🏰",
            "Donjons et raids".to_owned(),
            listed.iter().filter(|l| zone(l).dungeon).collect(),
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
                    let open = searching || self.opened.contains(&folder);
                    if entry(ui, Some((glyph, color)), &title, Some(rows.len()), false, 0).clicked()
                        && !self.opened.remove(&folder)
                    {
                        self.opened.insert(folder);
                    }
                    if open {
                        for l in rows {
                            self.zone_row(ui, zone(l), l.faction, state, worker);
                        }
                    }
                }
            });
        self.listed.zones = listed;
    }

    /// One zone of a folder: click chooses it, Ctrl+click adds it to the chosen ones, right click
    /// adds it to / removes it from the favorites.
    fn zone_row(
        &mut self,
        ui: &mut egui::Ui,
        z: &Zone,
        faction: Option<Faction>,
        state: &mut State,
        worker: &WorkerHandle,
    ) {
        let dark = theme::current(ui).dark;
        let selected = self.zones.contains(&z.name);
        let count = Some(z.npcs as usize);
        // Dungeons belong to no faction: no dot column.
        let response = if z.dungeon {
            entry(ui, None, &z.name, count, selected, 1)
        } else {
            let dot = faction_dot(faction, dark);
            entry_marked(ui, &z.name, count, selected, 1, dot)
        }
        .on_hover_text(zone_hint(z, faction));
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
