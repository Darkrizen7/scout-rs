//! What the UI asks (`Command`) and what the worker answers (`Event`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use scout_core::analysis::Analysis;
use scout_core::drops::{ItemDrops, Zone, ZoneLoot};
use scout_core::gamedata::GameData;
use scout_core::map::Mask;
use scout_core::sales::Sales;
use scout_core::scout_api::ServerChoice;
use scout_core::update::{AddonInstall, Release};
use scout_core::{ItemSummary, Snapshot, Theme, VendorData};
pub use scout_gamedata::Manifest;

use crate::Config;

pub enum Command {
    Import,
    /// An item's price history: what the database has at once, then (`Event::History` again) the
    /// rest of it from the price server when the cache is an hour old.
    LoadHistory(i64),
    /// Where an item comes from (item sheet, Drops tab).
    LoadDrops(i64),
    /// Zones and dungeons with known loot (Loot module).
    LoadZones,
    /// The monsters of a zone and their loot.
    LoadZone(String),
    /// Zone shapes for the map (OpenMap's masks; none without it).
    LoadMapMasks,
    Analyze,
    SetFavorite(i64, bool),
    SetZoneFavorite(String, bool),
    /// Downloads the newest Forever client tables (unless already stored).
    UpdateGameData,
    SetWowDir(PathBuf),
    /// What Scout addon the WoW folder holds (answered by `Event::ScoutAddon`).
    CheckScoutAddon,
    SetTheme(Theme),
    SetHideObsolete(bool),
    /// Skill of the character in a profession (id); None clears it.
    SetProfessionSkill(i64, Option<i64>),
    /// Downloads this release and replaces the executable (answered by `Event::UpdateInstalled`).
    InstallUpdate(Release),
    /// Starts the new executable, then `Event::Quit`.
    Restart,
    /// Installs (or updates) the Scout addon from this release.
    InstallAddon(Release),
    /// Price server address and auction house (saved, then the houses and prices follow).
    SetPriceServer {
        url: String,
        region: String,
        house: Option<u32>,
    },
    /// The auction houses of the price server (answered by `Event::ServerHouses`).
    LoadServerHouses,
    /// Latest prices of every item from the price server now, however old they are.
    SyncPrices,
}

/// Long tasks, each on its own thread: several can run at once, but not the same one twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Task {
    /// Prices, names, merchants and loot recorded by Scout.
    Import,
    /// Forever client tables (and Wowhead).
    GameData,
    /// New version of the tool.
    Update,
    /// Scout addon download and install.
    Addon,
    /// Latest prices from the price server.
    Prices,
    /// Prices of the last days written for the addon (`History.lua`).
    AddonHistory,
}

pub enum Event {
    Config(Config),
    /// A long task started (true) or ended (false).
    Busy(Task, bool),
    Status(String),
    Error(String),
    Items(Vec<ItemSummary>),
    History(i64, Vec<Snapshot>),
    Drops(i64, Box<ItemDrops>),
    Zones(Vec<Zone>),
    ZoneLoot(Box<ZoneLoot>),
    /// UiMap id → shape of the zone.
    MapMasks(Arc<HashMap<i64, Mask>>),
    Analysis(Box<Analysis>),
    Favorites(Vec<i64>),
    /// What the house shown sells per day (unknown until it has a few days of sales).
    Sales(Arc<Sales>),
    /// Names of the favorite zones (Loot module).
    ZoneFavorites(Vec<String>),
    GameData(Arc<GameData>),
    /// Newest game data package on the price server (None: none for this version of the tool),
    /// or why it could not be checked.
    LatestGameData(Result<Option<Manifest>, String>),
    /// The Scout addon in the WoW folder (missing, dev install, version).
    ScoutAddon(AddonInstall),
    /// Newest release of the Scout addon (checked at start).
    AddonRelease(Release),
    /// `History.lua` written: how many items and bytes.
    AddonHistory {
        items: usize,
        bytes: usize,
    },
    /// Merchant prices and which NPCs sell what.
    Vendor(Arc<VendorData>),
    /// A newer version of the tool is published (checked at start).
    UpdateAvailable(Release),
    /// That version replaced the executable: it runs after a restart.
    UpdateInstalled(String),
    /// The new executable is running: the UI closes.
    Quit,
    /// The price server's regions and the houses of the chosen region, or why they could not be read.
    ServerHouses(Result<ServerChoice, String>),
}
