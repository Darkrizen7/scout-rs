//! Required-skill filter and sort of the recipe list (the skill a recipe asks for, not the character's).

use eframe::egui;

/// Parsed filter and sort, part of the cache key of the hit list.
pub(super) type SkillKey = (Option<i64>, Option<i64>, bool);

#[derive(Default)]
pub(super) struct SkillFilter {
    min: String,
    max: String,
    /// Highest required skill first.
    pub descending: bool,
}

impl SkillFilter {
    /// Bounds typed by the user; an empty or invalid field is no bound.
    pub fn range(&self) -> (Option<i64>, Option<i64>) {
        let parse = |s: &str| s.trim().parse::<u32>().ok().map(i64::from);
        (parse(&self.min), parse(&self.max))
    }

    pub fn key(&self) -> SkillKey {
        let (min, max) = self.range();
        (min, max, self.descending)
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Compétence de");
            field(ui, &mut self.min);
            ui.label("à");
            field(ui, &mut self.max);
            let arrow = if self.descending { "⏷" } else { "⏶" };
            if ui
                .button(arrow)
                .on_hover_text("Trier par compétence requise (croissante / décroissante)")
                .clicked()
            {
                self.descending = !self.descending;
            }
        });
    }
}

fn field(ui: &mut egui::Ui, text: &mut String) {
    ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(36.0)
            .char_limit(3)
            .hint_text("0"),
    );
}
