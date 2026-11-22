//! What the panels read: worker data plus the user's current view, filter and selection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::analysis::Analysis;
use crate::config::Config;
use crate::db::{ItemSummary, Snapshot, VendorData};
use crate::gamedata::GameData;
use crate::worker::{Command, Event, WorkerHandle};

/// Top-level tab: one module of the tool.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Module {
    /// Every game item, each with a sheet (market, recipes, merchants, drops).
    Items,
    /// Game recipes and items: what crafts what, what is used where.
    Recipes,
    /// Auction house prices (list, charts, analysis, calculator).
    Market,
}

/// Screen inside the Market module.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Chart,
    Analysis,
    Calculator,
}

/// Which items the list and the analysis show (chosen in the side bar).
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Filter {
    pub favorites: bool,
    pub class_id: Option<i64>,
    pub subclass_id: Option<i64>,
}

impl Filter {
    pub fn is_all(&self) -> bool {
        *self == Self::default()
    }
}

pub struct State {
    pub module: Module,
    pub view: View,
    pub filter: Filter,
    pub config: Option<Config>,
    pub busy: bool,
    pub status: String,
    pub error: Option<String>,
    pub items: Vec<ItemSummary>,
    /// item id → index in `items`.
    index: HashMap<i64, usize>,
    /// Bumped whenever `items` or `favorites` change, so panels can refresh their caches.
    pub revision: u64,
    pub selected: Option<i64>,
    pub history: Option<(i64, Vec<Snapshot>)>,
    /// Latest analysis and the items revision it was asked for.
    pub analysis: Option<Analysis>,
    pub analysis_for: Option<u64>,
    pub favorites: HashSet<i64>,
    /// Game reference data for the Recipes module (empty until downloaded).
    pub game: Arc<GameData>,
    /// Item sheet to open in the Objects module (asked from another module).
    pub open_item: Option<i64>,
    /// Recipe (index in `game.recipes`) to open in the Recipes module.
    pub open_recipe: Option<usize>,
    /// Newest Forever build on wago.tools (checked at start), or the error.
    pub latest_build: Option<Result<String, String>>,
    /// Items merchants really sell: prices and NPCs.
    pub vendor: Arc<VendorData>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            module: Module::Items,
            view: View::List,
            filter: Filter::default(),
            config: None,
            busy: false,
            status: String::new(),
            error: None,
            items: Vec::new(),
            index: HashMap::new(),
            revision: 0,
            selected: None,
            history: None,
            analysis: None,
            analysis_for: None,
            favorites: HashSet::new(),
            game: Arc::default(),
            vendor: Arc::default(),
            latest_build: None,
            open_item: None,
            open_recipe: None,
        }
    }
}

impl State {
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Config(config) => self.config = Some(config),
            Event::Busy(busy) => self.busy = busy,
            Event::Status(status) => {
                self.status = status;
                self.error = None;
            }
            Event::Error(error) => self.error = Some(error),
            Event::Items(items) => {
                self.index = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item.item_id, i))
                    .collect();
                self.items = items;
                self.revision += 1;
            }
            Event::History(id, history) => self.history = Some((id, history)),
            Event::Analysis(analysis) => self.analysis = Some(*analysis),
            Event::GameData(game) => self.game = game,
            Event::Vendor(vendor) => self.vendor = vendor,
            Event::LatestBuild(build) => self.latest_build = Some(build),
            Event::Favorites(favorites) => {
                let favorites: HashSet<i64> = favorites.into_iter().collect();
                if favorites != self.favorites {
                    self.favorites = favorites;
                    self.revision += 1;
                }
            }
        }
    }

    pub fn item(&self, item_id: i64) -> Option<&ItemSummary> {
        self.items.get(*self.index.get(&item_id)?)
    }

    pub fn is_favorite(&self, item_id: i64) -> bool {
        self.favorites.contains(&item_id)
    }

    /// Toggles a favorite right away; the worker stores it and sends back the saved list.
    pub fn toggle_favorite(&mut self, item_id: i64, worker: &WorkerHandle) {
        let favorite = self.favorites.insert(item_id);
        if !favorite {
            self.favorites.remove(&item_id);
        }
        self.revision += 1;
        worker.send(Command::SetFavorite(item_id, favorite));
    }

    /// Whether the item passes the side bar filter.
    pub fn shown(&self, item_id: i64) -> bool {
        let f = &self.filter;
        if f.favorites && !self.is_favorite(item_id) {
            return false;
        }
        if f.class_id.is_none() {
            return true;
        }
        self.item(item_id).is_some_and(|i| {
            i.class_id == f.class_id && (f.subclass_id.is_none() || i.subclass_id == f.subclass_id)
        })
    }

    pub fn selected_item(&self) -> Option<&ItemSummary> {
        self.item(self.selected?)
    }

    /// Selects an item and asks for its history if not already loaded.
    pub fn select(&mut self, item_id: i64, worker: &WorkerHandle) {
        self.selected = Some(item_id);
        if self.history.as_ref().is_none_or(|(id, _)| *id != item_id) {
            worker.send(Command::LoadHistory(item_id));
        }
    }

    /// Switches to the Objects module on this item's sheet.
    pub fn show_item(&mut self, item_id: i64) {
        self.module = Module::Items;
        self.open_item = Some(item_id);
    }

    /// Switches to the Recipes module on this recipe.
    pub fn show_recipe(&mut self, recipe: usize) {
        self.module = Module::Recipes;
        self.open_recipe = Some(recipe);
    }

    /// Opens an item's chart.
    pub fn open(&mut self, item_id: i64, worker: &WorkerHandle) {
        self.select(item_id, worker);
        self.view = View::Chart;
    }
}

pub fn item_name(item: &ItemSummary) -> String {
    item.name
        .clone()
        .unwrap_or_else(|| format!("objet #{}", item.item_id))
}
