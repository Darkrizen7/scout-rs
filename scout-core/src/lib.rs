//! Scout's domain: plain data and pure logic, with no disk, network or UI.

pub mod analysis;
pub mod crafting;
pub mod credits;
pub mod drops;
pub mod gamedata;
pub mod map;
pub mod merge;
pub mod money;
pub mod pricing;
pub mod scout_api;
pub mod search;
pub mod time;
pub mod update;

mod market;
mod records;
mod theme;
mod vendor;

pub use gamedata::GAME_FORMAT;
pub use market::{ItemSummary, Snapshot};
pub use records::{ItemName, Row, SCOUT, VendorPrice};
pub use theme::Theme;
pub use vendor::{VendorData, VendorOffer};
