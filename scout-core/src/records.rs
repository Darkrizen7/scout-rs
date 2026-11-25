//! What the Lua exporters hand over: one record per CSV line.

use serde::Deserialize;

/// Source name of the prices Scout itself recorded.
pub const SCOUT: &str = "scout";

/// One CSV line: an item seen (or missing) at one auction house scan.
#[derive(Debug, Deserialize)]
pub struct Row {
    /// Where the price comes from (`scout`; the price server's own readings are `scout-api`).
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
    /// Auction house: Battle.net region and house id (`""` / 0 until known: the import derives them
    /// from the settings' house and `faction`, see `scout_api::house_for_faction`).
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub house: i64,
    /// Realm and faction (`Alliance`, `Horde`, `Neutral`) of the scan, from the Scout export; empty
    /// (`None`) in older exports. Only used by the import, never stored.
    #[serde(default, deserialize_with = "empty_none")]
    pub realm: Option<String>,
    #[serde(default, deserialize_with = "empty_none")]
    pub faction: Option<String>,
}

/// A blank CSV field is no value.
fn empty_none<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = Option::<String>::deserialize(d)?;
    Ok(s.filter(|s| !s.trim().is_empty()))
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
    /// `scout` (seen in game) or `foreverguide` (the reference database's list of unlimited profession reagents).
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
