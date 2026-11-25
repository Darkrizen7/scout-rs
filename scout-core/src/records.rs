//! What the Lua exporters hand over: one record per CSV line.

use serde::Deserialize;

/// Source name of the prices Scout itself recorded.
pub const SCOUT: &str = "scout";

/// One CSV line: an item seen (or missing) at one auction house scan.
#[derive(Debug, Deserialize)]
pub struct Row {
    /// Addon the price comes from (`scout`, `auctionator`, `foreverguide`).
    #[serde(default = "scout_source")]
    pub source: String,
    pub item_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    pub timestamp: i64,
    /// Unit buyout in copper; empty when the item was not listed.
    pub price: Option<i64>,
    /// Units listed; empty when the source did not record it.
    pub quantity: Option<i64>,
    /// Auction house: Battle.net region and house id (`""` / 0 until known: the addons do not say,
    /// the import gives the house chosen in the settings).
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub house: i64,
}

fn scout_source() -> String {
    SCOUT.to_owned()
}

/// An item name Scout saw at a scan (any item, not only the tracked ones).
#[derive(Debug, Deserialize)]
pub struct ItemName {
    pub item_id: i64,
    pub name: String,
    /// Item class / subclass ids and their names in the game client's language (empty with older exports).
    #[serde(default)]
    pub class_id: Option<i64>,
    #[serde(default)]
    pub subclass_id: Option<i64>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub subclass: Option<String>,
}

/// An item a merchant sells, with its unit price (copper, may be fractional) and, when Scout saw it, the NPC.
#[derive(Debug, Deserialize)]
pub struct VendorPrice {
    /// `scout` (seen in game) or `foreverguide` (its list of unlimited profession reagents).
    pub source: String,
    pub item_id: i64,
    pub price: f64,
    pub npc_id: Option<i64>,
    pub npc: Option<String>,
    pub zone: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// 1 when the merchant has a limited stock.
    pub limited: u8,
}
