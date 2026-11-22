//! Small widgets and colors shared by several panels.

use eframe::egui::{self, Color32};

use crate::ui::{coins, theme};

/// Files-like star: dim outline, filled and colored when starred.
pub fn star_button(ui: &mut egui::Ui, favorite: bool, palette: &theme::Palette) -> egui::Response {
    let (icon, color, hint) = if favorite {
        ("★", palette.star, "Retirer des favoris")
    } else {
        ("☆", palette.dim_text, "Ajouter aux favoris")
    };
    ui.add(egui::Button::new(egui::RichText::new(icon).size(17.0).color(color)).frame(false))
        .on_hover_text(hint)
}

/// Item quality colors, darker on the light theme for contrast.
pub fn quality_color(quality: i64, dark: bool) -> Option<Color32> {
    let (light, dark_c) = match quality {
        0 => (0x77767b, 0x9d9d9d),
        2 => (0x1a8f00, 0x1eff00),
        3 => (0x0062c4, 0x3a9bff),
        4 => (0x8a2be2, 0xb36bff),
        5 => (0xc65f00, 0xff8000),
        _ => return None,
    };
    let hex: u32 = if dark { dark_c } else { light };
    Some(Color32::from_rgb(
        (hex >> 16) as u8,
        (hex >> 8) as u8,
        hex as u8,
    ))
}

pub fn money_or_dash(ui: &mut egui::Ui, copper: Option<i64>) {
    match copper {
        Some(c) => {
            coins::money(ui, c, None);
        }
        None => {
            ui.weak("—");
        }
    }
}
