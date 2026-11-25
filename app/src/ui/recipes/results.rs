//! Results of the Recipes module: search field, recipes matching, then items made or used by recipes.

use eframe::egui::{self, RichText, text::LayoutJob};
use egui_extras::{Column, TableBuilder};

use super::{Hit, Recipes, SEARCH_HINT, SEARCH_ID, profession_icon};
use crate::state::State;
use crate::ui::sidebar::class_icon;
use crate::ui::widgets::search_box;
use crate::ui::{coins, theme};
use scout_core::crafting;
use scout_core::gamedata::GameData;
use scout_core::search::Query;

impl Recipes {
    pub(super) fn results(&mut self, ui: &mut egui::Ui, game: &GameData, state: &mut State) {
        ui.horizontal(|ui| {
            let width = ui.available_width() - 40.0;
            search_box(ui, SEARCH_ID, SEARCH_HINT, &mut self.search, width);
            if ui
                .button("✖")
                .on_hover_text("Effacer la recherche et le métier")
                .clicked()
            {
                self.search.clear();
                self.profession = None;
                self.skill = Default::default();
                self.selection = None;
                self.browsing = false;
            }
        });
        ui.add_space(4.0);
        // "Ma compétence" (APP-20) goes next to this row.
        self.skill.show(ui);
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);
        self.rebuild(game, state);
        ui.weak(format!("{} résultats", self.hits.len()));
        let dark = theme::current(ui).dark;
        let mut clicked = None;
        TableBuilder::new(ui)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(22.0))
            .column(Column::remainder().clip(true))
            .body(|body| {
                body.rows(40.0, self.hits.len(), |mut row| {
                    let index = row.index();
                    let hit = self.hits[index];
                    row.set_selected(matches!(hit, Hit::Recipe(i) if self.selection == Some(i)));
                    row.col(|ui| {
                        if matches!(hit, Hit::Recipe(_)) {
                            profit_icon(ui, self.profits[index]);
                        }
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| match hit {
                            Hit::Recipe(i) => {
                                let r = &game.recipes[i];
                                ui.label(icon_and_name(
                                    ui,
                                    profession_icon(r.profession, dark),
                                    &r.name,
                                ));
                                let skill =
                                    r.required.map_or_else(String::new, |s| format!(" · {s}"));
                                ui.weak(format!("{}{skill}", game.profession_name(r.profession)));
                            }
                            Hit::Item(id) => {
                                let class = game.items.get(&id).and_then(|i| i.class_id);
                                ui.label(icon_and_name(
                                    ui,
                                    class_icon(class.unwrap_or(-1), dark),
                                    &game.item_name(id),
                                ));
                                let made = game.made_by.get(&id).map_or(0, Vec::len);
                                let used = game.used_in.get(&id).map_or(0, Vec::len);
                                ui.weak(format!(
                                    "objet · produit par {made} · utilisé dans {used}"
                                ));
                            }
                        });
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        clicked = Some(hit);
                    }
                });
            });
        match clicked {
            Some(Hit::Recipe(i)) => {
                self.selection = Some(i);
            }
            Some(Hit::Item(id)) => state.show_item(id),
            None => {}
        }
    }

    /// Recipes whose name or product matches, then the reagents that match.
    fn rebuild(&mut self, game: &GameData, state: &State) {
        let key = (
            game as *const GameData as usize,
            self.search.clone(),
            self.profession,
            state.revision,
            self.skill.key(),
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let query = Query::new(&self.search);
        let (skill_min, skill_max) = self.skill.range();
        let matches = |name: &str| query.matches(name);
        let mut recipes: Vec<usize> = (0..game.recipes.len())
            .filter(|&i| {
                let r = &game.recipes[i];
                self.profession.is_none_or(|p| r.profession == p)
                    && crafting::skill_in_range(r, skill_min, skill_max)
                    && (matches(&r.name)
                        || r.creates
                            .is_some_and(|c| matches(&game.item_name(c.item_id))))
            })
            .collect();
        // Name stays the second criterion in both directions.
        let descending = self.skill.descending;
        recipes.sort_by_cached_key(|&i| {
            let skill = game.recipes[i].required.unwrap_or(0);
            (
                if descending { -skill } else { skill },
                game.recipes[i].name.to_lowercase(),
            )
        });
        self.hits = recipes.into_iter().map(Hit::Recipe).collect();
        if !query.is_empty() {
            // Reagents matching the search, limited to the selected profession. Items only made by
            // recipes are left out: their recipes are already listed (the product name matches).
            let mut items: Vec<i64> = game
                .used_in
                .iter()
                .filter(|(_, recipes)| {
                    self.profession
                        .is_none_or(|p| recipes.iter().any(|&i| game.recipes[i].profession == p))
                })
                .map(|(&id, _)| id)
                .filter(|&id| matches(&game.item_name(id)))
                .collect();
            items.sort_by_key(|id| game.item_name(*id).to_lowercase());
            self.hits.extend(items.into_iter().map(Hit::Item));
        }
        self.profits = self
            .hits
            .iter()
            .map(|hit| match *hit {
                Hit::Recipe(i) => crafting::profit(
                    game,
                    &game.recipes[i],
                    &|id| state.auction_price(id),
                    &|id| game.items.get(&id).map(|i| i.sell_price),
                    &|id| state.buy_price(id).map(|(unit, _)| unit),
                ),
                Hit::Item(_) => None,
            })
            .collect();
        self.built_for = Some(key);
    }
}

/// "$" in green (profit), red (loss) or grey (unknown); the tooltip gives the profit.
fn profit_icon(ui: &mut egui::Ui, profit: Option<i64>) {
    let palette = theme::current(ui);
    let color = match profit {
        Some(p) if p >= 0 => palette.gain,
        Some(_) => palette.loss,
        None => ui.visuals().weak_text_color(),
    };
    ui.label(RichText::new("$").strong().color(color))
        .on_hover_ui(|ui| match profit {
            Some(p) => {
                ui.horizontal(|ui| {
                    ui.label("Bénéfice par craft :");
                    coins::money(ui, p, Some(color));
                });
            }
            None => {
                ui.label("Prix inconnu");
            }
        });
}

/// Tinted icon followed by a name, as one label: a nested `horizontal` would be taller than a
/// plain label and push the second line out of the fixed-height row.
fn icon_and_name(ui: &egui::Ui, (glyph, color): (&str, egui::Color32), name: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let style = ui.style();
    RichText::new(format!("{glyph} ")).color(color).append_to(
        &mut job,
        style,
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    RichText::new(name).append_to(
        &mut job,
        style,
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    job
}
