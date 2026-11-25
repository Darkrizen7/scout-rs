//! Where items come from: the reference loot (from the embedded reference database, now frozen) and the loot Scout
//! recorded in game (replaced at each Scout import: the addon keeps running totals).

mod zones;

use anyhow::Result;

use super::Db;
use scout_core::drops::{BaseRow, ItemDrops, ObservedLoot, ObservedRow};

/// The reference loot tables (also the schema of the package the server serves, see `reference`).
pub(super) const REFERENCE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS drop_sources (
    item_id    INTEGER NOT NULL,
    kind       TEXT    NOT NULL,  -- npc, skinning, pickpocket, object, container, quest, quest_choice
    source_id  INTEGER NOT NULL,  -- NPC, world object, container item or quest; 0 = `more` unlisted sources
    chance     REAL,              -- percent, NULL when unknown
    quest_only INTEGER NOT NULL DEFAULT 0,
    more       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (item_id, kind, source_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS drop_npcs (
    npc_id    INTEGER PRIMARY KEY,
    name      TEXT,
    min_level INTEGER,
    max_level INTEGER,
    rank      INTEGER,
    zone      TEXT,
    x         REAL,
    y         REAL,
    dungeon   TEXT,
    ui_map    INTEGER  -- map of its zone (UiMap id)
);
CREATE TABLE IF NOT EXISTS drop_names (
    kind TEXT    NOT NULL,  -- object, quest
    id   INTEGER NOT NULL,
    name TEXT    NOT NULL,
    PRIMARY KEY (kind, id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS drop_meta (
    key   TEXT PRIMARY KEY,  -- fingerprint: version of the reference loot (frozen); updated: Unix seconds
    value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS drop_sources_source ON drop_sources (source_id);
";

/// What Scout recorded in game (personal: never in a reference package).
pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS loot_npcs (
    npc_id INTEGER PRIMARY KEY,
    name   TEXT,
    zone   TEXT,
    kills  INTEGER NOT NULL,  -- corpses looted
    skins  INTEGER NOT NULL   -- corpses skinned, mined or gathered
);
CREATE TABLE IF NOT EXISTS loot_observed (
    npc_id   INTEGER NOT NULL,
    kind     TEXT    NOT NULL,  -- loot, skin
    item_id  INTEGER NOT NULL,
    drops    INTEGER NOT NULL,  -- corpses it dropped from
    quantity INTEGER NOT NULL,
    PRIMARY KEY (npc_id, kind, item_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS loot_observed_item ON loot_observed (item_id);
";

const BASE_QUERY: &str = "
SELECT s.kind, s.source_id, s.chance, s.quest_only != 0 AS quest_only, s.more,
       COALESCE(n.name, d.name, g.name) AS name,
       n.min_level, n.max_level, n.rank, n.zone, n.x, n.y, n.dungeon, n.ui_map,
       l.kills, l.skins
FROM drop_sources s
LEFT JOIN drop_npcs n ON n.npc_id = s.source_id AND s.kind IN ('npc', 'skinning', 'pickpocket')
LEFT JOIN drop_names d ON d.id = s.source_id
     AND d.kind = CASE s.kind WHEN 'object' THEN 'object' WHEN 'quest' THEN 'quest'
                              WHEN 'quest_choice' THEN 'quest' END
LEFT JOIN game_items g ON g.item_id = s.source_id AND s.kind = 'container'
LEFT JOIN loot_npcs l ON l.npc_id = s.source_id AND s.kind IN ('npc', 'skinning')
WHERE s.item_id = ?";

const OBSERVED_QUERY: &str = "
SELECT o.npc_id, o.kind, o.drops, o.quantity, n.name, n.zone, n.kills, n.skins
FROM loot_observed o JOIN loot_npcs n ON n.npc_id = o.npc_id
WHERE o.item_id = ?";

impl Db {
    /// Adds `drop_npcs.ui_map` to databases made before it (filled by the reference database).
    pub(super) async fn migrate_drops(pool: &sqlx::SqlitePool) -> Result<()> {
        let columns: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('drop_npcs')")
                .fetch_all(pool)
                .await?;
        if !columns.iter().any(|(c,)| c == "ui_map") {
            sqlx::raw_sql("ALTER TABLE drop_npcs ADD COLUMN ui_map INTEGER")
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    /// Fingerprint of the reference loot the stored drops come from.
    pub async fn drops_fingerprint(&self) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM drop_meta WHERE key = 'fingerprint'")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    /// Replaces the loot Scout recorded (running totals). Returns the number of kills.
    pub async fn save_observed_loot(&self, rows: &[ObservedLoot]) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM loot_observed")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM loot_npcs")
            .execute(&mut *tx)
            .await?;
        let mut kills = 0;
        let mut last_npc = None;
        for r in rows {
            if last_npc != Some(r.npc_id) {
                last_npc = Some(r.npc_id);
                kills += r.kills;
                sqlx::query(
                    "INSERT OR REPLACE INTO loot_npcs (npc_id, name, zone, kills, skins) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(r.npc_id)
                .bind(r.name.as_deref().filter(|n| !n.is_empty()))
                .bind(r.zone.as_deref().filter(|z| !z.is_empty()))
                .bind(r.kills)
                .bind(r.skins)
                .execute(&mut *tx)
                .await?;
            }
            let (Some(kind), Some(item_id)) = (r.kind.as_deref(), r.item_id) else {
                continue;
            };
            sqlx::query(
                "INSERT OR REPLACE INTO loot_observed (npc_id, kind, item_id, drops, quantity) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(r.npc_id)
            .bind(kind)
            .bind(item_id)
            .bind(r.drops.unwrap_or(0))
            .bind(r.quantity.unwrap_or(0))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(kills)
    }

    /// Every known source of an item, reference and observed, sorted.
    pub async fn item_drops(&self, item_id: i64) -> Result<ItemDrops> {
        let base: Vec<BaseRow> = sqlx::query_as(BASE_QUERY)
            .bind(item_id)
            .fetch_all(&self.pool)
            .await?;
        let observed: Vec<ObservedRow> = sqlx::query_as(OBSERVED_QUERY)
            .bind(item_id)
            .fetch_all(&self.pool)
            .await?;
        let (total_kills,): (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(kills), 0) FROM loot_npcs")
                .fetch_one(&self.pool)
                .await?;
        Ok(ItemDrops::new(base, observed, total_kills))
    }
}
