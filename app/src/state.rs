//! What the panels read: worker data plus the user's current view, filter and selection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(feature = "analysis")]
use scout_core::analysis::Analysis;
use scout_core::drops::{ItemDrops, Zone, ZoneLoot};
use scout_core::gamedata::{self, GameData};
use scout_core::map::Mask;
use scout_core::pricing::{self, Source};
use scout_core::scout_api::ServerChoice;
use scout_core::update::{AddonInstall, Release};
use scout_core::{ItemSummary, Snapshot, VendorData};
use scout_worker::Config;
use scout_worker::{Command, Task, WorkerHandle};

mod apply;
mod loads;

/// Top-level tab: one module of the tool.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Module {
    /// Every game item, each with a sheet (market, recipes, merchants, drops).
    Items,
    /// Game recipes and items: what crafts what, what is used where.
    Recipes,
    /// Auction house prices (list, charts, analysis, calculator).
    Market,
    /// Loot by map zone: the monsters of a zone and what they drop.
    Loot,
}

/// Screen inside the Market module.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Chart,
    #[cfg(feature = "analysis")]
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
    /// Long tasks the worker is running (several at once).
    running: HashSet<Task>,
    pub status: String,
    pub error: Option<String>,
    pub items: Vec<ItemSummary>,
    /// item id → index in `items`.
    index: HashMap<i64, usize>,
    /// Bumped whenever `items` or `favorites` change, so panels can refresh their caches.
    pub revision: u64,
    pub selected: Option<i64>,
    pub history: Option<(i64, Vec<Snapshot>)>,
    /// Where the item of the open sheet comes from, and the item asked for (None once answered).
    pub drops: Option<(i64, ItemDrops)>,
    drops_asked: Option<i64>,
    /// Zones with known loot, and whether they were asked for.
    zones: Option<Arc<Vec<Zone>>>,
    zones_asked: bool,
    /// Monsters of the zones opened in the Loot module (kept until the next import), and those asked for.
    zone_loot: HashMap<String, Arc<ZoneLoot>>,
    zone_asked: HashSet<String>,
    /// Zone shapes for the map (UiMap id → mask), and whether they were asked for.
    masks: Option<Arc<HashMap<i64, Mask>>>,
    masks_asked: bool,
    /// Latest analysis and the items revision it was asked for.
    #[cfg(feature = "analysis")]
    pub analysis: Option<Analysis>,
    #[cfg(feature = "analysis")]
    pub analysis_for: Option<u64>,
    pub favorites: HashSet<i64>,
    /// Favorite zones of the Loot module (by name).
    pub zone_favorites: HashSet<String>,
    /// Game reference data for the Recipes module (empty until downloaded).
    pub game: Arc<GameData>,
    /// Item sheet to open in the Objects module (asked from another module).
    pub open_item: Option<i64>,
    /// Recipe (index in `game.recipes`) to open in the Recipes module.
    pub open_recipe: Option<usize>,
    /// Newest Forever build on wago.tools (checked at start), or the error.
    pub latest_game_data: Option<Result<Option<scout_worker::Manifest>, String>>,
    /// The price server's regions and houses (Settings, header), or why they could not be read.
    pub server_houses: Option<Result<ServerChoice, String>>,
    /// The house shown changed: the open sheet asks for its history again.
    pub house_changed: bool,
    /// The Scout addon in the WoW folder (None until the worker has looked).
    pub scout_addon: Option<AddonInstall>,
    /// Newest release of the Scout addon on GitHub (checked at start).
    pub addon_release: Option<Release>,
    /// Items merchants really sell: prices and NPCs.
    pub vendor: Arc<VendorData>,
    /// Newer version of the tool on GitHub (checked at start).
    pub update: Option<Release>,
    /// Version installed by an update, used after a restart.
    pub update_installed: Option<String>,
    /// The new version is running: close this window.
    pub quit: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            module: Module::Items,
            view: View::List,
            filter: Filter::default(),
            config: None,
            running: HashSet::new(),
            status: String::new(),
            error: None,
            items: Vec::new(),
            index: HashMap::new(),
            revision: 0,
            selected: None,
            history: None,
            drops: None,
            drops_asked: None,
            zones: None,
            zones_asked: false,
            zone_loot: HashMap::new(),
            zone_asked: HashSet::new(),
            masks: None,
            masks_asked: false,
            #[cfg(feature = "analysis")]
            analysis: None,
            #[cfg(feature = "analysis")]
            analysis_for: None,
            favorites: HashSet::new(),
            zone_favorites: HashSet::new(),
            game: Arc::default(),
            vendor: Arc::default(),
            update: None,
            update_installed: None,
            quit: false,
            latest_game_data: None,
            server_houses: None,
            house_changed: false,
            scout_addon: None,
            addon_release: None,
            open_item: None,
            open_recipe: None,
        }
    }
}

impl State {
    /// Whether the server has game data newer than the stored ones (or any, when none are
    /// stored). False while checking, on a network error or when the server has nothing.
    pub fn game_update_available(&self) -> bool {
        let game = &self.game;
        let stored = (!game.is_empty()).then_some((game.build.as_str(), game.format, game.updated));
        update_available(self.latest_game_data.as_ref(), stored)
    }

    pub fn busy(&self, task: Task) -> bool {
        self.running.contains(&task)
    }

    pub fn any_busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// An import is writing prices: the analysis would be stale, the WoW folder is in use.
    /// The addon release to offer: missing, or older than the newest one.
    pub fn addon_offer(&self) -> Option<&Release> {
        self.scout_addon
            .as_ref()?
            .offer(self.addon_release.as_ref())
    }

    pub fn importing(&self) -> bool {
        self.busy(Task::Import)
    }

    pub fn item(&self, item_id: i64) -> Option<&ItemSummary> {
        self.items.get(*self.index.get(&item_id)?)
    }

    /// Latest auction price (Market data) of an item, in copper.
    pub fn auction_price(&self, item_id: i64) -> Option<i64> {
        self.item(item_id)?.price
    }

    /// Cheapest way to buy one unit: a merchant or the auction house.
    pub fn buy_price(&self, item_id: i64) -> Option<(f64, Source)> {
        pricing::cheapest(self.vendor.price(item_id), self.auction_price(item_id))
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

    /// Toggles a favorite zone right away; the worker stores it.
    pub fn toggle_zone_favorite(&mut self, zone: &str, worker: &WorkerHandle) {
        let favorite = self.zone_favorites.insert(zone.to_owned());
        if !favorite {
            self.zone_favorites.remove(zone);
        }
        worker.send(Command::SetZoneFavorite(zone.to_owned(), favorite));
    }

    /// Whether an item is left out of the lists: obsolete name and the setting on (default).
    pub fn hides(&self, name: &str) -> bool {
        self.config.as_ref().is_none_or(|c| c.hide_obsolete) && gamedata::is_obsolete(name)
    }

    /// Changes the "hide obsolete items" setting; the lists are rebuilt.
    pub fn set_hide_obsolete(&mut self, hide: bool, worker: &WorkerHandle) {
        if let Some(config) = &mut self.config {
            config.hide_obsolete = hide;
        }
        self.revision += 1;
        worker.send(Command::SetHideObsolete(hide));
    }

    /// Whether the item passes the side bar filter (and is not a hidden obsolete one).
    pub fn shown(&self, item_id: i64) -> bool {
        if self
            .item(item_id)
            .and_then(|i| i.name.as_deref())
            .is_some_and(|n| self.hides(n))
        {
            return false;
        }
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

/// The rule behind `State::game_update_available`, on plain values.
fn update_available(
    latest: Option<&Result<Option<scout_worker::Manifest>, String>>,
    stored: Option<(&str, i64, i64)>,
) -> bool {
    matches!(latest, Some(Ok(Some(m))) if m.newer_than(stored))
}

#[cfg(test)]
mod update_tests {
    use super::*;
    use scout_worker::Manifest;

    fn manifest() -> Manifest {
        Manifest {
            format: 1,
            build: "b1".into(),
            updated: 100,
            complete: true,
            bytes: 5,
        }
    }

    #[test]
    fn badge_rule() {
        let ok = Ok(Some(manifest()));
        assert!(!update_available(None, None));
        assert!(!update_available(Some(&Err("x".into())), None));
        assert!(!update_available(Some(&Ok(None)), None));
        assert!(update_available(Some(&ok), None));
        assert!(update_available(Some(&ok), Some(("b0", 1, 100))));
        assert!(update_available(Some(&ok), Some(("b1", 1, 99))));
        assert!(!update_available(Some(&ok), Some(("b1", 1, 100))));
    }
}
