//! Files-like side bar: views and favorites on top, then item categories (folders).

use std::collections::{BTreeMap, HashSet};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Sense};

use crate::state::{Filter, State, View};
use crate::ui::theme;

/// subclass id → (name, item count)
type Subclasses = BTreeMap<i64, (String, usize)>;

struct Category {
    class_id: i64,
    name: String,
    count: usize,
    /// (subclass id, name, count)
    subclasses: Vec<(i64, String, usize)>,
}

#[derive(Default)]
pub struct Sidebar {
    categories: Vec<Category>,
    /// Categories whose subcategories are shown.
    expanded: HashSet<i64>,
    built_for: Option<u64>,
}

impl Sidebar {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State) {
        self.rebuild(state);
        let all = state.view == View::List && state.filter.is_all();
        let favorites = state.view == View::List && state.filter.favorites;
        let palette = theme::current(ui);

        if entry(
            ui,
            Some(("📋", palette.accent)),
            "Tous les objets",
            Some(state.items.len()),
            all,
            0,
        )
        .clicked()
        {
            state.filter = Filter::default();
            state.view = View::List;
        }
        if entry(
            ui,
            Some(("★", palette.star)),
            "Favoris",
            Some(state.favorites.len()),
            favorites,
            0,
        )
        .clicked()
        {
            state.filter = Filter {
                favorites: true,
                ..Filter::default()
            };
            state.view = View::List;
        }
        if entry(
            ui,
            Some(("📊", palette.gain)),
            "Analyse",
            None,
            state.view == View::Analysis,
            0,
        )
        .clicked()
        {
            state.view = View::Analysis;
        }
        if entry(
            ui,
            Some(("💰", tint(GOLD, palette.dark))),
            "Rentabilité",
            None,
            state.view == View::Calculator,
            0,
        )
        .clicked()
        {
            state.view = View::Calculator;
        }
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if self.categories.is_empty() {
                    ui.weak("Catégories : importe depuis Scout (Réglages).");
                }
                let filtering = matches!(state.view, View::List | View::Analysis);
                for category in &self.categories {
                    let id = category.class_id;
                    let chosen = state.filter.class_id == Some(id);
                    let selected = filtering && chosen && state.filter.subclass_id.is_none();
                    let foldable = category.subclasses.len() >= 2;
                    let open = foldable && self.expanded.contains(&id);
                    let icon = Some(class_icon(id, palette.dark));
                    let response =
                        entry(ui, icon, &category.name, Some(category.count), selected, 0);
                    if foldable {
                        // Fold indicator, left of the count.
                        ui.painter().text(
                            egui::pos2(response.rect.right() - 48.0, response.rect.center().y),
                            Align2::RIGHT_CENTER,
                            if open { "⏷" } else { "⏵" },
                            FontId::proportional(11.0),
                            theme::current(ui).dim_text,
                        );
                    }
                    if response.clicked() {
                        // Each click selects the category and folds / unfolds its subcategories.
                        if !self.expanded.remove(&id) {
                            self.expanded.insert(id);
                        }
                        choose(state, Some(id), None);
                    }
                    if !open {
                        continue;
                    }
                    for (subclass_id, name, count) in &category.subclasses {
                        let selected = filtering && state.filter.subclass_id == Some(*subclass_id);
                        if entry(ui, None, name, Some(*count), selected, 1).clicked() {
                            choose(state, Some(category.class_id), Some(*subclass_id));
                        }
                    }
                }
            });
    }

    fn rebuild(&mut self, state: &State) {
        if self.built_for == Some(state.revision) {
            return;
        }
        let mut classes: BTreeMap<i64, (String, usize, Subclasses)> = BTreeMap::new();
        for item in &state.items {
            let (Some(class_id), Some(class_name)) = (item.class_id, &item.class_name) else {
                continue;
            };
            let class = classes
                .entry(class_id)
                .or_insert_with(|| (class_name.clone(), 0, BTreeMap::new()));
            class.1 += 1;
            if let (Some(sub_id), Some(sub_name)) = (item.subclass_id, &item.subclass_name) {
                class
                    .2
                    .entry(sub_id)
                    .or_insert_with(|| (sub_name.clone(), 0))
                    .1 += 1;
            }
        }
        self.categories = classes
            .into_iter()
            .map(|(class_id, (name, count, subs))| Category {
                class_id,
                name,
                count,
                subclasses: subs
                    .into_iter()
                    .map(|(id, (name, count))| (id, name, count))
                    .collect(),
            })
            .collect();
        self.categories.sort_by_key(|c| c.name.to_lowercase());
        for c in &mut self.categories {
            c.subclasses.sort_by_key(|(_, name, _)| name.to_lowercase());
        }
        self.built_for = Some(state.revision);
    }
}

/// A side bar icon: glyph and color.
pub type Icon = (&'static str, Color32);

pub const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const GOLD: Color32 = rgb(0xd4a20b);

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

/// Filters by category; the analysis stays open, any other view goes to the list.
fn choose(state: &mut State, class_id: Option<i64>, subclass_id: Option<i64>) {
    state.filter = Filter {
        favorites: false,
        class_id,
        subclass_id,
    };
    if state.view != View::Analysis {
        state.view = View::List;
    }
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
    let palette = theme::current(ui);
    let height = if indent == 0 { 34.0 } else { 28.0 };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
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
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(size),
            palette.text,
        );
        if let Some(count) = count {
            painter.text(
                egui::pos2(rect.right() - 10.0, rect.center().y),
                Align2::RIGHT_CENTER,
                count.to_string(),
                FontId::proportional(12.0),
                palette.dim_text,
            );
        }
    }
    response
}
