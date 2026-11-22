//! A monster of the Loot module: where it is, and what it drops (looted, skinned, pickpocketed).

use eframe::egui::{self, RichText};

use super::{VALUE_HINT, unit_value};
use crate::state::State;
use crate::ui::theme::Palette;
use crate::ui::{coins, widgets};
use scout_core::drops::{DropKind, ZoneNpc};
use scout_core::gamedata::GameData;
use scout_core::map;

const KINDS: [(DropKind, &str); 3] = [
    (DropKind::Npc, "Butin"),
    (DropKind::Skinning, "Dépeçage"),
    (DropKind::Pickpocket, "Vol à la tire"),
];

/// Shows the monster; returns the item clicked (opened in the Objects module).
pub fn show(
    ui: &mut egui::Ui,
    npc: &ZoneNpc,
    game: &GameData,
    state: &State,
    palette: &Palette,
) -> Option<i64> {
    let mut clicked = None;
    ui.heading(npc.name());
    ui.horizontal(|ui| {
        widgets::level_label(ui, &npc.npc, palette);
        ui.weak("·");
        let n = &npc.npc;
        let waypoint = map::waypoint(n.ui_map, n.x, n.y, &npc.name());
        widgets::place_label(ui, &n.place(), waypoint);
        ui.weak(match npc.kills {
            0 => "· jamais fouillé (Scout)".to_owned(),
            k => format!("· {k} corps fouillés (Scout)"),
        });
    });
    if let Some(value) = npc.value_per_kill(|id| unit_value(state, game, id)) {
        ui.horizontal(|ui| {
            ui.label("Valeur estimée par kill :");
            coins::money(ui, value.round() as i64, None);
        })
        .response
        .on_hover_text(VALUE_HINT);
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            if npc.drops.is_empty() {
                ui.weak("Aucun butin connu.");
            }
            for (kind, title) in KINDS {
                let drops: Vec<_> = npc.of(kind).collect();
                if drops.is_empty() {
                    continue;
                }
                ui.strong(format!("{title} ({})", drops.len()));
                let observed = kind != DropKind::Pickpocket;
                egui::Grid::new(("npc-loot", kind))
                    .num_columns(if observed { 4 } else { 3 })
                    .spacing([14.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for header in ["Objet", "Chance"] {
                            ui.label(RichText::new(header).color(palette.dim_text));
                        }
                        if observed {
                            ui.label(RichText::new("Réel").color(palette.dim_text));
                        }
                        ui.label(RichText::new("Prix unitaire").color(palette.dim_text));
                        ui.end_row();
                        for d in drops {
                            ui.horizontal(|ui| {
                                let quality = game.items.get(&d.item_id).map_or(1, |i| i.quality);
                                let mut name = RichText::new(game.item_name(d.item_id));
                                if let Some(color) = widgets::quality_color(quality, palette.dark) {
                                    name = name.color(color);
                                }
                                if ui.link(name).on_hover_text("Ouvrir sa fiche").clicked() {
                                    clicked = Some(d.item_id);
                                }
                                if d.quest_only {
                                    ui.weak("· quête en cours");
                                }
                            });
                            widgets::chance_label(ui, d.chance);
                            if observed {
                                widgets::observed_label(ui, d.observed, palette);
                            }
                            match unit_value(state, game, d.item_id) {
                                Some(v) => {
                                    coins::money(ui, v.round() as i64, None);
                                }
                                None => {
                                    ui.weak("—");
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
            }
        });
    clicked
}
