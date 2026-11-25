//! Objects module: every game item, by category, with prices; a click opens the item sheet.

mod drops;
mod list;
mod merchants;
mod rows;
mod sheet;
mod sidebar;

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui;

use crate::state::State;
use crate::ui::theme;
use crate::ui::widgets::data_missing;
use scout_core::gamedata::{GameData, GameItem};
use scout_worker::WorkerHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Level,
    Auction,
}

#[derive(Clone, PartialEq, Eq)]
struct Filter {
    favorites: bool,
    class_id: Option<i64>,
    subclass_id: Option<i64>,
}

impl Filter {
    const ALL: Self = Self {
        favorites: false,
        class_id: None,
        subclass_id: None,
    };
}

pub struct Items {
    search: String,
    filter: Filter,
    sort: Sort,
    descending: bool,
    expanded: HashSet<i64>,
    /// False on arrival: the search sits in the middle of the page until a category is chosen or text typed.
    browsing: bool,
    /// Item ids shown, filtered and sorted.
    rows: Vec<i64>,
    built_for: Option<(usize, u64, Filter, String, Sort, bool)>,
    /// Open sheet, and the sheets opened before it (back button).
    sheet: Option<sheet::Sheet>,
    back: Vec<i64>,
}

impl Default for Items {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: Filter::ALL,
            sort: Sort::Name,
            descending: false,
            expanded: HashSet::new(),
            browsing: false,
            rows: Vec::new(),
            built_for: None,
            sheet: None,
            back: Vec::new(),
        }
    }
}

impl Items {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
        let game = Arc::clone(&state.game);
        if game.is_empty() {
            return data_missing(ui, state, worker, ("📦", "Objets"));
        }
        if let Some(item) = state.open_item.take() {
            self.open(item, state, worker);
        }
        let palette = theme::current(ui);
        egui::Panel::left("items-sidebar")
            .exact_size(230.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(8, 10)),
            )
            .show(ui, |ui| self.sidebar(ui, &game, state));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.view)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                if let Some(sheet) = &mut self.sheet {
                    let back = if self.back.is_empty() {
                        "← Liste"
                    } else {
                        "← Retour"
                    };
                    if ui.button(back).clicked() {
                        match self.back.pop() {
                            Some(previous) => {
                                self.sheet = Some(sheet::Sheet::new(previous, state, worker))
                            }
                            None => self.sheet = None,
                        }
                        return;
                    }
                    match sheet.show(ui, &game, state, worker) {
                        Some(sheet::Link::Item(id)) => self.open(id, state, worker),
                        Some(sheet::Link::Recipe(i)) => state.show_recipe(i),
                        None => {}
                    }
                } else {
                    self.list(ui, &game, state, worker);
                }
            });
    }

    /// Shows the list of a side bar entry.
    fn choose(&mut self, filter: Filter) {
        self.filter = filter;
        self.sheet = None;
        self.browsing = true;
    }

    /// Opens an item sheet, remembering the one shown before.
    fn open(&mut self, item_id: i64, state: &mut State, worker: &WorkerHandle) {
        if let Some(current) = &self.sheet {
            if current.item_id == item_id {
                return;
            }
            self.back.push(current.item_id);
        }
        self.sheet = Some(sheet::Sheet::new(item_id, state, worker));
    }
}

/// "Epées à deux mains", else the class name.
pub fn category(game: &GameData, item: &GameItem) -> String {
    let Some(class) = item.class_id else {
        return String::new();
    };
    game.category_name(class, item.subclass_id)
        .or_else(|| game.category_name(class, None))
        .unwrap_or_default()
        .to_owned()
}
