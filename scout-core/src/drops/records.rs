//! Records the Scout exporter hands over and the database returns, before merging into [`super::ItemDrops`].

use serde::Deserialize;

/// `export.lua --loot` line (Scout addon): one monster and one item it gave (or none).
#[derive(Debug, Deserialize)]
pub struct ObservedLoot {
    pub npc_id: i64,
    pub name: Option<String>,
    pub zone: Option<String>,
    pub kills: i64,
    pub skins: i64,
    /// `loot` (looted corpse) or `skin` (skinned / mined / gathered); empty on a line without item.
    pub kind: Option<String>,
    pub item_id: Option<i64>,
    /// Corpses the item dropped from.
    pub drops: Option<i64>,
    pub quantity: Option<i64>,
}

/// A reference source of one item, as the database returns it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct BaseRow {
    pub kind: String,
    pub source_id: i64,
    pub chance: Option<f64>,
    pub quest_only: bool,
    pub more: i64,
    pub name: Option<String>,
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    pub rank: Option<i64>,
    pub zone: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub dungeon: Option<String>,
    pub ui_map: Option<i64>,
    /// Corpses looted / skinned by the user (Scout), when that NPC was ever killed.
    pub kills: Option<i64>,
    pub skins: Option<i64>,
}

/// What Scout recorded for one monster and this item.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ObservedRow {
    pub npc_id: i64,
    /// `loot` or `skin`.
    pub kind: String,
    pub drops: i64,
    pub quantity: i64,
    pub name: Option<String>,
    pub zone: Option<String>,
    pub kills: i64,
    pub skins: i64,
}
