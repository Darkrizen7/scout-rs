//! "Monter le métier": the recipes that still give a skill point, cheapest point first.

use eframe::egui::{self, Color32, RichText, Sense, vec2};
use egui_extras::{Column, TableBuilder};

use super::skill_bar::{GREEN, GREY, ORANGE, YELLOW};
use crate::state::State;
use crate::ui::{coins, theme};
use scout_core::gamedata::GameData;
use scout_core::skillup::{self, SkillColor, SkillUpRow};
use scout_worker::WorkerHandle;

/// Highest skill of a profession on WoW Forever.
const MAX_SKILL: i64 = 300;

#[derive(Default)]
pub(super) struct SkillUp {
    /// Text of the "Ma compétence" field.
    text: String,
    /// Profession the text was loaded for.
    loaded_for: Option<i64>,
    /// List shown instead of the search results.
    enabled: bool,
    rows: Vec<SkillUpRow>,
    /// (data address, profession, skill, items revision) the rows were computed for.
    built_for: Option<(usize, i64, i64, u64)>,
}

fn color_of(color: SkillColor) -> Color32 {
    match color {
        SkillColor::Orange => ORANGE,
        SkillColor::Yellow => YELLOW,
        SkillColor::Green => GREEN,
        SkillColor::Grey => GREY,
    }
}

impl SkillUp {
    /// Whether the list replaces the search results.
    pub fn active(&self) -> bool {
        self.enabled
    }

    /// Skill field and list toggle of a profession; returns the skill once entered.
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut State,
        worker: &WorkerHandle,
        profession: i64,
    ) -> Option<i64> {
        if self.loaded_for != Some(profession) {
            self.loaded_for = Some(profession);
            self.text = state
                .profession_skill(profession)
                .map_or_else(String::new, |s| s.to_string());
        }
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Ma compétence");
            // Wide enough for the hint (and so three digits) in the current font, plus the
            // text edit margins (4 px each side) and the cursor.
            let font = egui::TextStyle::Body.resolve(ui.style());
            let hint =
                ui.painter()
                    .layout_no_wrap("1-300".to_owned(), font, egui::Color32::PLACEHOLDER);
            changed = ui
                .add(
                    egui::TextEdit::singleline(&mut self.text)
                        .desired_width(hint.size().x + 12.0)
                        .char_limit(3)
                        .hint_text("1-300"),
                )
                .changed();
        });
        let skill = self
            .text
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|s| (1..=MAX_SKILL).contains(s));
        if changed && (skill.is_some() || self.text.trim().is_empty()) {
            state.set_profession_skill(profession, skill, worker);
        }
        if skill.is_some() {
            ui.checkbox(&mut self.enabled, "Monter le métier (point le moins cher)");
        }
        skill
    }

    /// The list; `selection` is the recipe clicked.
    pub fn list(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        state: &State,
        (profession, skill): (i64, i64),
        selection: &mut Option<usize>,
    ) {
        let key = (
            game as *const GameData as usize,
            profession,
            skill,
            state.revision,
        );
        if self.built_for != Some(key) {
            let in_profession =
                (0..game.recipes.len()).filter(|&i| game.recipes[i].profession == profession);
            self.rows = skillup::rank(
                game,
                in_profession,
                skill,
                &|id| state.auction_price(id),
                &|id| game.items.get(&id).map(|i| i.sell_price),
                &|id| state.buy_price(id).map(|(unit, _)| unit),
            );
            self.built_for = Some(key);
        }
        ui.weak(format!(
            "{} recettes donnent encore un point",
            self.rows.len()
        ));
        let palette = theme::current(ui);
        let mut clicked = None;
        TableBuilder::new(ui)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(18.0))
            .column(Column::remainder().clip(true))
            .body(|body| {
                body.rows(44.0, self.rows.len(), |mut row| {
                    let item = self.rows[row.index()];
                    row.set_selected(*selection == Some(item.recipe));
                    row.col(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 5.0, color_of(item.color));
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            let name = &game.recipes[item.recipe].name;
                            ui.label(format!("{name} · {:.0} %", item.chance * 100.0));
                            ui.horizontal(|ui| match (item.net_cost, item.per_point) {
                                (Some(net), Some(point)) => {
                                    ui.weak("craft");
                                    coins::money(ui, net, Some(gain_or_loss(palette, net)));
                                    ui.weak("· point");
                                    coins::money(ui, point, Some(gain_or_loss(palette, point)));
                                }
                                _ => {
                                    ui.label(RichText::new("prix inconnu").weak());
                                }
                            });
                        });
                    });
                    if row
                        .response()
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        clicked = Some(item.recipe);
                    }
                });
            });
        if clicked.is_some() {
            *selection = clicked;
        }
    }
}

/// Cost in red, earnings in green.
fn gain_or_loss(palette: &theme::Palette, cost: i64) -> Color32 {
    if cost <= 0 {
        palette.gain
    } else {
        palette.loss
    }
}
