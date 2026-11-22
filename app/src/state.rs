//! What the panels read: worker data plus the user's current view, filter and selection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use scout_core::analysis::Analysis;
use scout_core::drops::{ItemDrops, Zone, ZoneLoot};
use scout_core::gamedata::{self, GameData};
use scout_core::map::Mask;
use scout_core::pricing::{self, Source};
use scout_core::{ItemSummary, Snapshot, VendorData};
use scout_worker::Config;
use scout_worker::{Command, Event, Task, WorkerHandle};

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
    pub analysis: Option<Analysis>,
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
            analysis: None,
            analysis_for: None,
            favorites: HashSet::new(),
            zone_favorites: HashSet::new(),
            game: Arc::default(),
            vendor: Arc::default(),
            latest_build: None,
            open_item: None,
            open_recipe: None,
        }
    }
}

impl State {
    pub fn busy(&self, task: Task) -> bool {
        self.running.contains(&task)
    }

    pub fn any_busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// An import is writing prices: the analysis would be stale, the WoW folder is in use.
    pub fn importing(&self) -> bool {
        self.busy(Task::Import) || self.busy(Task::ImportLegacy)
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Config(config) => {
                if self.config.as_ref().map(|c| c.hide_obsolete) != Some(config.hide_obsolete) {
                    self.revision += 1;
                }
                self.config = Some(config);
            }
            Event::Busy(task, true) => {
                self.running.insert(task);
            }
            Event::Busy(task, false) => {
                self.running.remove(&task);
            }
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
                // Sent after each import: the drops may have changed, the open sheet asks again.
                self.drops = None;
                self.drops_asked = None;
                self.zones = None;
                self.zones_asked = false;
                self.zone_loot.clear();
                self.zone_asked.clear();
            }
            Event::History(id, history) => self.history = Some((id, history)),
            Event::Drops(id, drops) => {
                self.drops = Some((id, *drops));
                self.drops_asked = None;
            }
            Event::Zones(zones) => self.zones = Some(Arc::new(zones)),
            Event::ZoneLoot(loot) => {
                self.zone_asked.remove(&loot.zone);
                self.zone_loot.insert(loot.zone.clone(), Arc::from(loot));
            }
            Event::MapMasks(masks) => self.masks = Some(masks),
            Event::Analysis(analysis) => self.analysis = Some(*analysis),
            Event::GameData(game) => self.game = game,
            Event::Vendor(vendor) => self.vendor = vendor,
            Event::LatestBuild(build) => self.latest_build = Some(build),
            Event::ZoneFavorites(zones) => self.zone_favorites = zones.into_iter().collect(),
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

    /// The sources of an item once loaded; asks the worker for them (once) otherwise.
    pub fn item_drops(&mut self, item_id: i64, worker: &WorkerHandle) -> Option<&ItemDrops> {
        let loaded = self.drops.as_ref().is_some_and(|(id, _)| *id == item_id);
        if !loaded && self.drops_asked != Some(item_id) {
            self.drops_asked = Some(item_id);
            worker.send(Command::LoadDrops(item_id));
        }
        self.drops
            .as_ref()
            .filter(|(id, _)| *id == item_id)
            .map(|(_, d)| d)
    }

    /// Zones with known loot once loaded; asks the worker for them (once) otherwise.
    pub fn zones(&mut self, worker: &WorkerHandle) -> Option<Arc<Vec<Zone>>> {
        if self.zones.is_none() && !self.zones_asked {
            self.zones_asked = true;
            worker.send(Command::LoadZones);
        }
        self.zones.clone()
    }

    /// The monsters of a zone once loaded; asks the worker for them (once) otherwise.
    pub fn zone_loot(&mut self, zone: &str, worker: &WorkerHandle) -> Option<Arc<ZoneLoot>> {
        if !self.zone_loot.contains_key(zone) && self.zone_asked.insert(zone.to_owned()) {
            worker.send(Command::LoadZone(zone.to_owned()));
        }
        self.zone_loot.get(zone).cloned()
    }

    /// Zone shapes once loaded (empty without OpenMap); asks the worker for them (once) otherwise.
    pub fn map_masks(&mut self, worker: &WorkerHandle) -> Option<Arc<HashMap<i64, Mask>>> {
        if self.masks.is_none() && !self.masks_asked {
            self.masks_asked = true;
            worker.send(Command::LoadMapMasks);
        }
        self.masks.clone()
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
