//! Skill-up bar of a recipe: orange / yellow / green / grey segments from 0 to the profession cap,
//! with the skill level written under each boundary.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, vec2};

use scout_core::gamedata::Recipe;

use crate::ui::theme;

/// Highest skill of a profession on WoW Forever.
const MAX_SKILL: i64 = 300;
const BAR_HEIGHT: f32 = 12.0;
const LABEL_HEIGHT: f32 = 16.0;

const ORANGE: Color32 = Color32::from_rgb(0xe6, 0x61, 0x00);
const YELLOW: Color32 = Color32::from_rgb(0xe0, 0xb4, 0x00);
const GREEN: Color32 = Color32::from_rgb(0x3d, 0xa5, 0x3d);
const GREY: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8e);

/// Draws the bar next to the profession name; hovering it explains the colors.
pub fn show(ui: &mut egui::Ui, r: &Recipe) {
    let palette = theme::current(ui);
    let width = ui.available_width().min(300.0);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, BAR_HEIGHT + LABEL_HEIGHT), Sense::hover());
    let bar = Rect::from_min_size(rect.min, vec2(width, BAR_HEIGHT));
    let x =
        |skill: i64| bar.left() + bar.width() * skill.clamp(0, MAX_SKILL) as f32 / MAX_SKILL as f32;

    let start = r.required.unwrap_or(0);
    let steps = [
        (start, r.yellow, ORANGE),
        (r.yellow, r.green(), YELLOW),
        (r.green(), r.grey, GREEN),
        (r.grey, MAX_SKILL, GREY),
    ];
    let painter = ui.painter();
    painter.rect_filled(bar, 3.0, palette.widget);
    // Before the required skill the recipe cannot be learned: left empty.
    for (from, to, color) in steps {
        if to > from {
            let segment = Rect::from_x_y_ranges(x(from)..=x(to), bar.y_range());
            painter.rect_filled(segment, 0.0, color);
        }
    }
    painter.rect_stroke(
        bar,
        3.0,
        Stroke::new(1.0, palette.border),
        StrokeKind::Inside,
    );

    let font = FontId::proportional(11.0);
    let mut last_label = f32::NEG_INFINITY;
    for skill in [start, r.yellow, r.green(), r.grey] {
        let px = x(skill);
        painter.line_segment(
            [
                egui::pos2(px, bar.top()),
                egui::pos2(px, bar.bottom() + 3.0),
            ],
            Stroke::new(1.0, palette.text),
        );
        // Skip a label that would overlap the previous one.
        if px - last_label >= 24.0 {
            painter.text(
                egui::pos2(px, bar.bottom() + 3.0),
                Align2::CENTER_TOP,
                skill,
                font.clone(),
                palette.dim_text,
            );
            last_label = px;
        }
    }
    response.on_hover_text(format!(
        "Niveau de métier nécessaire\n\
         orange : {start}+ (montée garantie)\n\
         jaune : {}+ (probable)\n\
         vert : {}+ (peu probable)\n\
         gris : {}+ (aucune montée)",
        r.yellow,
        r.green(),
        r.grey
    ));
}
