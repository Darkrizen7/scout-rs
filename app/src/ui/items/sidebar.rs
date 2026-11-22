//! Objects side bar: all items, favorites, then categories (classes with their subclasses).

use std::collections::BTreeMap;

use eframe::egui;

use super::{Filter, Items};
use crate::state::State;
use crate::ui::sidebar::{class_icon, entry};
use crate::ui::theme;
use scout_core::gamedata::GameData;

impl Items {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, game: &GameData, state: &State) {
        let palette = theme::current(ui);
        let all = Filter::ALL;
        if entry(
            ui,
            Some(("📦", palette.accent)),
            "Tous les objets",
            Some(
                game.items
                    .values()
                    .filter(|i| !state.hides(&i.name))
                    .count(),
            ),
            self.browsing && self.filter == all,
            0,
        )
        .clicked()
        {
            self.choose(all);
        }
        let favorites = self.filter.favorites;
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
            self.choose(Filter {
                favorites: true,
                class_id: None,
                subclass_id: None,
            });
        }
        ui.add_space(4.0);
        ui.separator();
        // class → (count, subclass → count)
        let mut counts: BTreeMap<i64, (usize, BTreeMap<i64, usize>)> = BTreeMap::new();
        for item in game.items.values().filter(|i| !state.hides(&i.name)) {
            if let (Some(class), Some(sub)) = (item.class_id, item.subclass_id) {
                let entry = counts.entry(class).or_default();
                entry.0 += 1;
                *entry.1.entry(sub).or_default() += 1;
            }
        }
        let mut classes: Vec<(i64, &str, usize, BTreeMap<i64, usize>)> = counts
            .into_iter()
            .filter_map(|(class, (count, subs))| {
                Some((class, game.category_name(class, None)?, count, subs))
            })
            .collect();
        classes.sort_by_key(|(_, name, _, _)| name.to_lowercase());
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (class, name, count, subs) in classes {
                    let chosen = !self.filter.favorites && self.filter.class_id == Some(class);
                    let selected = chosen && self.filter.subclass_id.is_none();
                    let open = subs.len() >= 2 && self.expanded.contains(&class);
                    if entry(
                        ui,
                        Some(class_icon(class, palette.dark)),
                        name,
                        Some(count),
                        selected,
                        0,
                    )
                    .clicked()
                    {
                        if !self.expanded.remove(&class) {
                            self.expanded.insert(class);
                        }
                        self.choose(Filter {
                            favorites: false,
                            class_id: Some(class),
                            subclass_id: None,
                        });
                    }
                    if !open {
                        continue;
                    }
                    let mut subs: Vec<(i64, &str, usize)> = subs
                        .into_iter()
                        .map(|(sub, n)| {
                            (sub, game.category_name(class, Some(sub)).unwrap_or("?"), n)
                        })
                        .collect();
                    subs.sort_by_key(|(_, name, _)| name.to_lowercase());
                    for (sub, sub_name, n) in subs {
                        let selected = chosen && self.filter.subclass_id == Some(sub);
                        if entry(ui, None, sub_name, Some(n), selected, 1).clicked() {
                            self.choose(Filter {
                                favorites: false,
                                class_id: Some(class),
                                subclass_id: Some(sub),
                            });
                        }
                    }
                }
            });
    }
}
