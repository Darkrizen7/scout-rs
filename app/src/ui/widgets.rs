//! Small widgets and colors shared by several panels.

use eframe::egui::{self, Color32, RichText};

use crate::ui::{coins, theme};
use scout_core::drops::{Observed, TRUSTED_KILLS, format_percent};

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

/// Forever Guide's chance of a drop, "?" when unknown.
pub fn chance_label(ui: &mut egui::Ui, chance: Option<f64>) {
    match chance {
        Some(p) => {
            ui.label(format_percent(p));
        }
        None => {
            ui.weak("?")
                .on_hover_text("Chance inconnue (liste de Questie, de Forever ou objet garanti)");
        }
    }
}

/// The user's own rate (Scout), "30 % (3/10)", highlighted once there are enough corpses; interval on hover.
pub fn observed_label(ui: &mut egui::Ui, observed: Option<Observed>, palette: &theme::Palette) {
    let Some(o) = observed else {
        ui.weak("—").on_hover_text("Jamais enregistré par Scout");
        return;
    };
    let text = format!("{} ({}/{})", format_percent(o.rate()), o.drops, o.total);
    let label = if o.total >= TRUSTED_KILLS {
        RichText::new(text).color(palette.gain)
    } else {
        RichText::new(text)
    };
    let (low, high) = o.interval();
    let mut hint = format!(
        "Intervalle à 95 % : {} – {}",
        format_percent(low),
        format_percent(high)
    );
    if o.drops > 0 {
        hint += &format!("\nQuantité moyenne : {:.1}", o.average_quantity()).replace('.', ",");
    }
    if o.total < TRUSTED_KILLS {
        hint += &format!("\nPeu de corps : moins de {TRUSTED_KILLS}, la base sert au tri.");
    }
    ui.label(label).on_hover_text(hint);
}

/// A position: with a map pin command, a link that copies it for the game's chat; plain dim text otherwise.
pub fn place_label(ui: &mut egui::Ui, text: &str, waypoint: Option<String>) {
    let Some(command) = waypoint else {
        ui.weak(text);
        return;
    };
    let hint = format!(
        "Clic : copie « {command} »\nColle-le dans le chat du jeu (Ctrl+V, Entrée) pour mettre un point sur la carte."
    );
    if ui.link(text).on_hover_text(hint).clicked() {
        ui.ctx().copy_text(command);
    }
}

/// A search field whose id does not depend on where it is drawn: the landing page and the list header share
/// it, so the focus stays while the first letters are typed.
pub fn search_box(
    ui: &mut egui::Ui,
    id: &str,
    hint: &str,
    text: &mut String,
    width: f32,
) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::unique(id))
            .hint_text(hint)
            .desired_width(width),
    )
}

/// Arrival page of a module: a big icon, the search in the middle of the page (focused), a caption under it.
pub fn landing_search(
    ui: &mut egui::Ui,
    icon: &str,
    id: &str,
    hint: &str,
    text: &mut String,
    caption: &str,
) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.35 - 40.0).max(0.0));
        ui.label(RichText::new(icon).size(40.0));
        ui.add_space(4.0);
        let search = search_box(ui, id, hint, text, 420.0);
        if ui.memory(|m| m.focused().is_none()) {
            search.request_focus();
        }
        ui.add_space(4.0);
        ui.weak(caption);
    });
}
