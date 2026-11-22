//! Records the exporters hand over and the database returns, before merging into [`super::ItemDrops`].

use serde::Deserialize;

use super::DropNpc;

/// `drops_export.lua sources` line. `source_id` 0 = "and `more` other sources of this kind".
#[derive(Debug, Deserialize)]
pub struct DropSource {
    pub kind: String,
    pub item_id: i64,
    pub source_id: i64,
    /// Percent per kill / opening; empty when unknown.
    pub chance: Option<f64>,
    /// 1 when it only drops while the quest is active.
    pub quest_only: u8,
    pub more: i64,
}

/// `drops_export.lua names` line: name of a world object or a quest.
#[derive(Debug, Deserialize)]
pub struct DropName {
    pub kind: String,
    pub id: i64,
    pub name: String,
}

/// Everything Forever Guide tells, stored together.
#[derive(Debug, Default)]
pub struct DropsExport {
    pub sources: Vec<DropSource>,
    pub npcs: Vec<DropNpc>,
    pub names: Vec<DropName>,
}

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
