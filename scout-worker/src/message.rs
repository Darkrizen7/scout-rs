//! What the UI asks (`Command`) and what the worker answers (`Event`).

use std::path::PathBuf;
use std::sync::Arc;

use scout_core::analysis::Analysis;
use scout_core::gamedata::GameData;
use scout_core::{ItemSummary, Snapshot, Theme, VendorData};

use crate::Config;

pub enum Command {
    Import,
    /// One-off import of the prices Auctionator and Forever Guide kept.
    ImportLegacy,
    LoadHistory(i64),
    Analyze,
    SetFavorite(i64, bool),
    /// Downloads the newest Forever client tables (unless already stored).
    UpdateGameData,
    SetWowDir(PathBuf),
    SetTheme(Theme),
}

pub enum Event {
    Config(Config),
    Busy(bool),
    Status(String),
    Error(String),
    Items(Vec<ItemSummary>),
    History(i64, Vec<Snapshot>),
    Analysis(Box<Analysis>),
    Favorites(Vec<i64>),
    GameData(Arc<GameData>),
    /// Newest Forever build on wago.tools, or why it could not be checked.
    LatestBuild(Result<String, String>),
    /// Merchant prices and which NPCs sell what.
    Vendor(Arc<VendorData>),
}
