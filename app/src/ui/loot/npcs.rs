//! Monster list of the Loot module: search by monster or item, sort by level, value per kill or value per
//! level.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::{Loot, unit_value};
use crate::state::State;
use crate::ui::{coins, theme, widgets};
use scout_core::drops::{DropKind, ZoneLoot, ZoneNpc};
use scout_core::gamedata::GameData;
use scout_core::search::Query;

/// Order of the monster list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Sort {
    Level,
    Value,
    /// Value per kill divided by the level.
    ValuePerLevel,
}

/// Zone data address, filter, sort, hide normal flag, items revision, game data address.
pub(super) type BuiltFor = (usize, String, Option<(Sort, bool)>, bool, u64, usize);

/// A monster of the list.
pub(super) struct Row {
    /// Index in `ZoneLoot::npcs`.
    index: usize,
    /// Value per kill, or per level when sorted by it (copper).
    value: Option<f64>,
    /// Item matching the filter (not the monster's name) and its best chance.
    matched: Option<(String, Option<f64>)>,
}

impl Loot {
    pub(super) fn npcs(
        &mut self,
        ui: &mut egui::Ui,
        loot: Option<&ZoneLoot>,
        game: &GameData,
        state: &State,
    ) {
        self.heading(ui);
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
            self.sort_button(ui, Sort::Level, "Niveau");
            self.sort_button(ui, Sort::Value, "Valeur / kill");
            self.sort_button(ui, Sort::ValuePerLevel, "÷ niveau");
        });
        ui.checkbox(&mut self.normal_only, "Masquer élites, rares et boss");
        self.rebuild(loot, game, state);
        ui.weak(format!("{} monstres", self.rows.len()));
        let several = self.zones.len() > 1;
        let mut clicked = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().clip(true))
            .column(Column::exact(90.0))
            .body(|body| {
                body.rows(28.0, self.rows.len(), |mut row| {
                    let r = &self.rows[row.index()];
                    let npc = &loot.npcs[r.index];
                    row.set_selected(self.selection == Some(npc.npc.npc_id));
                    row.col(|ui| {
                        ui.label(npc.name());
                        details(ui, npc, r, several);
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

    /// The chosen zones: a title for one, removable chips for several.
    fn heading(&mut self, ui: &mut egui::Ui) {
        if let [zone] = self.zones.as_slice() {
            ui.heading(zone);
            ui.weak("Ctrl+clic sur une autre zone pour l'ajouter");
            return;
        }
        ui.heading(format!("{} zones", self.zones.len()));
        let mut removed = None;
        ui.horizontal_wrapped(|ui| {
            for (i, zone) in self.zones.iter().enumerate() {
                if ui
                    .small_button(format!("{zone}  ×"))
                    .on_hover_text("Retirer")
                    .clicked()
                {
                    removed = Some(i);
                }
            }
        });
        if let Some(i) = removed {
            self.zones.remove(i);
        }
    }

    /// Sort button: click cycles first direction, the opposite one, then off.
    fn sort_button(&mut self, ui: &mut egui::Ui, sort: Sort, label: &str) {
        // Value sorts start with the best first, the level with the lowest.
        let first = sort != Sort::Level;
        let text = match self.sort {
            Some((s, descending)) if s == sort => {
                format!("{label} {}", if descending { "⏷" } else { "⏶" })
            }
            _ => label.to_owned(),
        };
        let active = self.sort.is_some_and(|(s, _)| s == sort);
        if ui.selectable_label(active, text).clicked() {
            self.sort = match self.sort {
                Some((s, descending)) if s == sort => {
                    (descending == first).then_some((sort, !first))
                }
                _ => Some((sort, first)),
            };
        }
    }

    /// Monsters whose name or one of whose items matches the filter, in the chosen order.
    fn rebuild(&mut self, loot: &ZoneLoot, game: &GameData, state: &State) {
        let key = (
            loot as *const ZoneLoot as usize,
            self.filter.clone(),
            self.sort,
            self.normal_only,
            state.revision,
            game as *const GameData as usize,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let query = Query::new(&self.filter);
        self.rows = loot
            .npcs
            .iter()
            .enumerate()
            .filter(|(_, npc)| !self.normal_only || npc.npc.is_normal())
            .filter_map(|(index, npc)| {
                let matched = if query.matches(&npc.name()) {
                    None
                } else {
                    let drop = npc
                        .drops
                        .iter()
                        .find(|d| query.matches(&game.item_name(d.item_id)))?;
                    Some((game.item_name(drop.item_id), drop.best_chance()))
                };
                let price = |id| unit_value(state, game, id);
                let value = match self.sort.map(|(sort, _)| sort) {
                    Some(Sort::ValuePerLevel) => npc.value_per_level(price),
                    _ => npc.value_per_kill(price),
                };
                Some(Row {
                    index,
                    value,
                    matched,
                })
            })
            .collect();
        match self.sort {
            Some((Sort::Level, true)) => self.rows.reverse(),
            Some((Sort::Level, false)) | None => {}
            Some((_, descending)) => {
                // Monsters without a priced drop last (stable: by level among them).
                self.rows.sort_by(|a, b| match (a.value, b.value) {
                    (Some(x), Some(y)) if descending => y.total_cmp(&x),
                    (Some(x), Some(y)) => x.total_cmp(&y),
                    (x, y) => y.is_some().cmp(&x.is_some()),
                });
            }
        }
        self.built_for = Some(key);
    }
}

/// After the name, in grey: the matching item and its chance, or the level with rank and skinning icons.
fn details(ui: &mut egui::Ui, npc: &ZoneNpc, row: &Row, several: bool) {
    let zone = npc
        .npc
        .dungeon
        .as_ref()
        .or(npc.npc.zone.as_ref())
        .filter(|_| several);
    if let Some((item, chance)) = &row.matched {
        let chance = chance.map_or_else(|| "?".to_owned(), scout_core::drops::format_percent);
        ui.weak(match zone {
            Some(zone) => format!("{item} · {chance} · {zone}"),
            None => format!("{item} · {chance}"),
        });
        return;
    }
    ui.spacing_mut().item_spacing.x = 4.0;
    ui.spacing_mut().interact_size.y = 12.0;
    if npc.npc.level_range().is_empty() {
        ui.weak("?");
    }
    widgets::level_label(ui, &npc.npc, theme::current(ui));
    if npc.of(DropKind::Skinning).next().is_some() {
        widgets::skinning_icon(ui, theme::current(ui)).on_hover_text("Dépeçable");
    }
    if let Some(zone) = zone {
        ui.weak(format!("· {zone}"));
    }
}
