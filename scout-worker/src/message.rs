//! What the UI asks (`Command`) and what the worker answers (`Event`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use scout_core::analysis::Analysis;
use scout_core::drops::{ItemDrops, Zone, ZoneLoot};
use scout_core::gamedata::GameData;
use scout_core::map::Mask;
use scout_core::{ItemSummary, Snapshot, Theme, VendorData};

use crate::Config;

pub enum Command {
    Import,
    /// One-off import of the prices Auctionator and Forever Guide kept.
    ImportLegacy,
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
    /// Downloads the newest Forever client tables (unless already stored).
    UpdateGameData,
    SetWowDir(PathBuf),
    SetTheme(Theme),
    SetHideObsolete(bool),
}

/// Long tasks, each on its own thread: several can run at once, but not the same one twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Task {
    /// Prices, names, merchants and loot recorded by Scout.
    Import,
    /// Older prices kept by Auctionator and Forever Guide.
    ImportLegacy,
    /// Forever client tables (and Wowhead).
    GameData,
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
    GameData(Arc<GameData>),
    /// Newest Forever build on wago.tools, or why it could not be checked.
    LatestBuild(Result<String, String>),
    /// Merchant prices and which NPCs sell what.
    Vendor(Arc<VendorData>),
}
