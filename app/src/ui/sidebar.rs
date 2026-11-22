//! Files-like side bar: views and favorites on top, then item categories (folders).

use std::collections::{BTreeMap, HashSet};

use eframe::egui::{self, Align2, FontId};

use crate::state::{Filter, State, View};
use crate::ui::theme;

mod entry;
pub use entry::{GOLD, Icon, Marker, class_icon, entry, entry_marked, rgb, tint};

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
        #[cfg(feature = "analysis")]
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
                #[cfg(feature = "analysis")]
                let filtering = matches!(state.view, View::List | View::Analysis);
                #[cfg(not(feature = "analysis"))]
                let filtering = state.view == View::List;
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

/// Filters by category; the analysis stays open, any other view goes to the list.
fn choose(state: &mut State, class_id: Option<i64>, subclass_id: Option<i64>) {
    state.filter = Filter {
        favorites: false,
        class_id,
        subclass_id,
    };
    #[cfg(feature = "analysis")]
    if state.view == View::Analysis {
        return;
    }
    state.view = View::List;
}
