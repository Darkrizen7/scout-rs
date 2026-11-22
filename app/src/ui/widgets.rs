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

/// Level range, then an icon for elite / rare (name on hover); bosses stay in text.
pub fn level_label(ui: &mut egui::Ui, npc: &scout_core::drops::DropNpc, palette: &theme::Palette) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.spacing_mut().interact_size.y = 12.0;
        let range = npc.level_range();
        if !range.is_empty() {
            ui.weak(range);
        }
        let (elite, rare) = match npc.rank {
            Some(1) => (true, false),
            Some(2) => (true, true),
            Some(4) => (false, true),
            Some(3) => {
                ui.weak("boss");
                return;
            }
            _ => return,
        };
        if elite {
            rank_icon(ui, true, palette).on_hover_text("Élite");
        }
        if rare {
            rank_icon(ui, false, palette).on_hover_text("Rare");
        }
    });
}

/// Elite: gold diamond. Rare: silver ring.
fn rank_icon(ui: &mut egui::Ui, elite: bool, palette: &theme::Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    if elite {
        let r = 6.0;
        let points = vec![
            c + egui::vec2(0.0, -r),
            c + egui::vec2(r, 0.0),
            c + egui::vec2(0.0, r),
            c + egui::vec2(-r, 0.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            points,
            palette.star,
            egui::Stroke::NONE,
        ));
    } else {
        painter.circle_stroke(c, 4.5, egui::Stroke::new(2.0, palette.dim_text));
    }
    response
}

/// Skinning knife: a small blade with a handle.
pub fn skinning_icon(ui: &mut egui::Ui, palette: &theme::Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    let painter = ui.painter();
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    painter.add(egui::Shape::convex_polygon(
        vec![p(1.0, 11.0), p(2.0, 3.0), p(9.0, 1.0), p(8.0, 8.0)],
        palette.dim_text,
        egui::Stroke::NONE,
    ));
    painter.line_segment(
        [p(6.0, 6.0), p(11.0, 11.0)],
        egui::Stroke::new(2.0, palette.dim_text),
    );
    response
}
