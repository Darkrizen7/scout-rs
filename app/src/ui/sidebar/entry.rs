//! Side bar rows and icons, shared by the side bars of every module.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Sense};

use crate::ui::theme;

/// A side bar icon: glyph and color.
pub type Icon = (&'static str, Color32);

pub const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const GOLD: Color32 = rgb(0xd4a20b);

/// Lightens a color on the dark theme so it stays readable.
pub fn tint(color: Color32, dark: bool) -> Color32 {
    if !dark {
        return color;
    }
    let lift = |c: u8| c + (255 - c) / 3;
    Color32::from_rgb(lift(color.r()), lift(color.g()), lift(color.b()))
}

/// Icon of an item class (game class ids, same in every client language).
/// egui draws glyphs in a single color, hence one color per category.
/// Only glyphs present in egui's bundled fonts (checked with fontTools).
pub fn class_icon(class_id: i64, dark: bool) -> Icon {
    let (glyph, color) = match class_id {
        0 => ("🍗", rgb(0xe66100)),  // Consumable
        1 => ("👜", rgb(0x986a44)),  // Container
        2 => ("⚔", rgb(0xc01c28)),   // Weapon
        3 => ("💎", rgb(0x9141ac)),  // Gem
        4 => ("🛡", rgb(0x1c71d8)),   // Armor
        5 => ("🔮", rgb(0xc061cb)),  // Reagent
        6 => ("➶", rgb(0x865e3c)),   // Projectile
        7 => ("🔨", rgb(0x5e5c64)),  // Trade goods
        9 => ("📜", rgb(0x26a269)),  // Recipe
        11 => ("🎒", rgb(0xa5714a)), // Quiver
        12 => ("❗", rgb(0xe5a50a)), // Quest
        13 => ("🔑", GOLD),          // Key
        15 => ("🎲", rgb(0x2190a4)), // Miscellaneous
        _ => ("📦", rgb(0x77767b)),
    };
    (glyph, tint(color, dark))
}

/// One side bar row: icon, label, count on the right; rounded highlight when selected or hovered.
pub fn entry(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    count: Option<usize>,
    selected: bool,
    indent: u8,
) -> egui::Response {
    row(ui, icon, label, count, selected, indent, None)
}

/// What sits in the column at the far right of a row (`entry_marked`).
#[derive(Clone, Copy)]
pub enum Marker {
    /// Nothing, but the column is kept so the counts of the rows stay aligned.
    Blank,
    Dot(Color32),
}

/// Like `entry`, with a column for a colored dot at the far right.
pub fn entry_marked(
    ui: &mut egui::Ui,
    label: &str,
    count: Option<usize>,
    selected: bool,
    indent: u8,
    marker: Marker,
) -> egui::Response {
    row(ui, None, label, count, selected, indent, Some(marker))
}

fn row(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    count: Option<usize>,
    selected: bool,
    indent: u8,
    marker: Option<Marker>,
) -> egui::Response {
    let palette = theme::current(ui);
    let height = if indent == 0 { 34.0 } else { 28.0 };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut elided = false;
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            Some(palette.sidebar_selected)
        } else if response.hovered() {
            Some(palette.sidebar_hover)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        }
        let painter = ui.painter_at(rect);
        let size = if indent == 0 { 15.0 } else { 13.5 };
        let mut x = rect.left() + 12.0 + f32::from(indent) * 26.0;
        if let Some((glyph, color)) = icon {
            painter.text(
                egui::pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                glyph,
                FontId::proportional(size + 1.0),
                color,
            );
            x += 26.0;
        }
        // Count first: the label gets the room left and is shortened with "…" when too long.
        let mut right = rect.right() - 10.0;
        if let Some(marker) = marker {
            if let Marker::Dot(color) = marker {
                painter.circle_filled(egui::pos2(right - 4.0, rect.center().y), 4.0, color);
            }
            right -= 16.0;
        }
        if let Some(count) = count {
            let count = painter.text(
                egui::pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                count.to_string(),
                FontId::proportional(12.0),
                palette.dim_text,
            );
            right = count.left() - 8.0;
        }
        let mut job = egui::text::LayoutJob::simple_singleline(
            label.to_owned(),
            FontId::proportional(size),
            palette.text,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(0.0));
        let galley = painter.layout_job(job);
        elided = galley.elided;
        let at = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
        painter.galley(at, galley, palette.text);
    }
    if elided {
        response.on_hover_text(label)
    } else {
        response
    }
}
