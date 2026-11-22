//! Look of the map: colors by level, zone names.

use eframe::egui::{self, Color32, Pos2};

use crate::ui::theme::Palette;
use scout_core::drops::Zone;

pub(super) fn levels(z: &Zone) -> String {
    match (z.min_level, z.max_level) {
        (Some(a), Some(b)) if a != b => format!("Niveaux {a}-{b}"),
        (Some(a), _) => format!("Niveau {a}"),
        _ => "Niveau inconnu".to_owned(),
    }
}

/// Green for low levels, through yellow, orange and red, to purple at 60.
pub(super) fn level_color(z: &Zone, alpha: u8) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 5] = [
        (1.0, [0x33, 0xd1, 0x7a]),
        (20.0, [0xf6, 0xd3, 0x2d]),
        (35.0, [0xff, 0x78, 0x00]),
        (48.0, [0xe0, 0x1b, 0x24]),
        (60.0, [0x91, 0x41, 0xac]),
    ];
    let level = match (z.min_level, z.max_level) {
        (Some(a), Some(b)) => (a + b) as f32 / 2.0,
        (Some(a), None) | (None, Some(a)) => a as f32,
        (None, None) => return Color32::from_white_alpha(alpha / 2),
    };
    let i = STOPS.iter().rposition(|(l, _)| level >= *l).unwrap_or(0);
    let (l0, c0) = STOPS[i];
    let (l1, c1) = STOPS[(i + 1).min(STOPS.len() - 1)];
    let t = if l1 > l0 {
        ((level - l0) / (l1 - l0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(c0[0], c1[0]),
        mix(c0[1], c1[1]),
        mix(c0[2], c1[2]),
        alpha,
    )
}

/// Zone name and levels, with a halo so they read over any color.
pub(super) fn label(
    painter: &egui::Painter,
    at: Pos2,
    name: &str,
    zone: Option<&Zone>,
    palette: &Palette,
) {
    let text = match zone.map(|z| (z.min_level, z.max_level)) {
        Some((Some(a), Some(b))) => format!("{name}\n{a}-{b}"),
        _ => name.to_owned(),
    };
    let font = egui::FontId::proportional(11.0);
    let halo = palette.view.gamma_multiply(0.8);
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        painter.text(
            at + egui::vec2(dx, dy),
            egui::Align2::CENTER_CENTER,
            &text,
            font.clone(),
            halo,
        );
    }
    painter.text(at, egui::Align2::CENTER_CENTER, &text, font, palette.text);
}
