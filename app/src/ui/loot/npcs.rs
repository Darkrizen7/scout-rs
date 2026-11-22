//! Monster list of the Loot module: search by monster or item, sort by level or value per kill.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::{Loot, VALUE_HINT, unit_value};
use crate::state::State;
use crate::ui::coins;
use scout_core::drops::{DropKind, ZoneLoot, ZoneNpc};
use scout_core::gamedata::GameData;

/// A monster of the list.
pub(super) struct Row {
    /// Index in `ZoneLoot::npcs`.
    index: usize,
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
        ui.checkbox(&mut self.normal_only, "Masquer élites, rares et boss");
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
            self.normal_only,
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
            .filter(|(_, npc)| !self.normal_only || npc.npc.is_normal())
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
