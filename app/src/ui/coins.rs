//! Amounts drawn as `12 ● 34 ● 56 ●` with small painted gold / silver / copper coins
//! (our own drawing, in the spirit of the game's money display).

use eframe::egui::{self, Color32, Stroke};

/// (fill, rim, highlight) of a coin.
const GOLD: (Color32, Color32, Color32) = (
    Color32::from_rgb(0xe8, 0xb9, 0x23),
    Color32::from_rgb(0x8a, 0x6d, 0x0b),
    Color32::from_rgb(0xff, 0xf1, 0xa8),
);
const SILVER: (Color32, Color32, Color32) = (
    Color32::from_rgb(0xc4, 0xc9, 0xcf),
    Color32::from_rgb(0x6f, 0x76, 0x7d),
    Color32::from_rgb(0xff, 0xff, 0xff),
);
const COPPER: (Color32, Color32, Color32) = (
    Color32::from_rgb(0xc0, 0x6a, 0x32),
    Color32::from_rgb(0x6e, 0x3a, 0x17),
    Color32::from_rgb(0xf3, 0xb4, 0x8a),
);

/// One coin, sized to the current text height.
fn coin(ui: &mut egui::Ui, (fill, rim, highlight): (Color32, Color32, Color32)) {
    let size = ui.text_style_height(&egui::TextStyle::Body) * 0.72;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let center = rect.center();
    let radius = size / 2.0;
    painter.circle(center, radius - 0.5, fill, Stroke::new(1.0, rim));
    painter.circle_stroke(
        center,
        radius * 0.62,
        Stroke::new(0.8, rim.gamma_multiply(0.6)),
    );
    painter.circle_filled(
        center + egui::vec2(-radius * 0.32, -radius * 0.32),
        radius * 0.22,
        highlight,
    );
}

/// Width `money` takes for an amount in copper (to right-align it).
pub fn money_width(ui: &egui::Ui, copper: i64) -> f32 {
    let body = egui::TextStyle::Body;
    let coin = ui.text_style_height(&body) * 0.72;
    let c = copper.unsigned_abs();
    let digits = |v: u64| {
        let font = body.resolve(ui.style());
        ui.fonts_mut(|f| {
            f.layout_no_wrap(v.to_string(), font, Color32::WHITE)
                .size()
                .x
        })
    };
    let mut width = 0.0;
    for (i, v) in [c / 10_000, c / 100 % 100, c % 100].into_iter().enumerate() {
        if v == 0 && !(i == 2 && width == 0.0) {
            continue;
        }
        width += digits(v) + coin + 9.0;
    }
    if copper < 0 {
        width += 12.0;
    }
    width + 4.0
}

/// Draws an amount in copper with coins; `color` overrides the number color (gain / loss).
pub fn money(ui: &mut egui::Ui, copper: i64, color: Option<Color32>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let text = |ui: &mut egui::Ui, s: String| {
            let rich = egui::RichText::new(s);
            ui.label(if let Some(c) = color {
                rich.color(c)
            } else {
                rich
            });
        };
        if copper < 0 {
            text(ui, "−".into());
        }
        let c = copper.unsigned_abs();
        let parts = [
            (c / 10_000, GOLD),
            (c / 100 % 100, SILVER),
            (c % 100, COPPER),
        ];
        let mut shown = false;
        for (i, (value, kind)) in parts.into_iter().enumerate() {
            // Zero parts are skipped, except a lone 0 copper.
            if value == 0 && !(i == 2 && !shown) {
                continue;
            }
            if shown {
                ui.add_space(3.0);
            }
            text(ui, value.to_string());
            coin(ui, kind);
            shown = true;
        }
    })
    .response
}
