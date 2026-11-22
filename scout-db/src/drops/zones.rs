//! Loot by map zone. A monster's zone is its dungeon, else Forever Guide's zone; a monster only Scout knows
//! (e.g. added by Forever) is placed in the zone where the user looted it.

use anyhow::Result;

use crate::Db;
use scout_core::drops::{Zone, ZoneDropRow, ZoneLoot, ZoneNpcRow, ZoneObservedRow};

/// Monsters by zone (Forever Guide's, then those only Scout knows), counted per zone.
const ZONES_QUERY: &str = "
WITH npc_zones AS (
    SELECT npc_id, COALESCE(dungeon, zone) AS zone, dungeon IS NOT NULL AS dungeon, min_level, max_level
    FROM drop_npcs WHERE COALESCE(dungeon, zone) IS NOT NULL
    UNION ALL
    SELECT npc_id, zone, 0, NULL, NULL
    FROM loot_npcs WHERE zone IS NOT NULL AND npc_id NOT IN (SELECT npc_id FROM drop_npcs)
)
SELECT z.zone AS name, MAX(z.dungeon) != 0 AS dungeon, COUNT(*) AS npcs,
       MIN(z.min_level) AS min_level, MAX(z.max_level) AS max_level, COALESCE(SUM(l.kills), 0) AS kills
FROM npc_zones z LEFT JOIN loot_npcs l ON l.npc_id = z.npc_id
GROUP BY z.zone ORDER BY z.zone";

const NPCS_QUERY: &str = "
SELECT n.npc_id, COALESCE(n.name, l.name) AS name, n.min_level, n.max_level, n.rank, n.zone, n.x, n.y,
       n.dungeon, l.kills, l.skins
FROM drop_npcs n LEFT JOIN loot_npcs l ON l.npc_id = n.npc_id
WHERE COALESCE(n.dungeon, n.zone) = ?1
UNION ALL
SELECT npc_id, name, NULL, NULL, NULL, zone, NULL, NULL, NULL, kills, skins
FROM loot_npcs WHERE zone = ?1 AND npc_id NOT IN (SELECT npc_id FROM drop_npcs)";

const DROPS_QUERY: &str = "
SELECT s.source_id AS npc_id, s.kind, s.item_id, s.chance, s.quest_only != 0 AS quest_only
FROM drop_sources s JOIN drop_npcs n ON n.npc_id = s.source_id
WHERE s.kind IN ('npc', 'skinning', 'pickpocket') AND COALESCE(n.dungeon, n.zone) = ?";

const OBSERVED_QUERY: &str = "
SELECT o.npc_id, o.kind, o.item_id, o.drops, o.quantity
FROM loot_observed o JOIN loot_npcs l ON l.npc_id = o.npc_id
LEFT JOIN drop_npcs n ON n.npc_id = o.npc_id
WHERE COALESCE(n.dungeon, n.zone, l.zone) = ?";

impl Db {
    /// Zones and dungeons that have monsters with known loot, by name.
    pub async fn zones(&self) -> Result<Vec<Zone>> {
        Ok(sqlx::query_as(ZONES_QUERY).fetch_all(&self.pool).await?)
    }

    /// The monsters of a zone and what each one drops.
    pub async fn zone_loot(&self, zone: &str) -> Result<ZoneLoot> {
        let npcs: Vec<ZoneNpcRow> = sqlx::query_as(NPCS_QUERY)
            .bind(zone)
            .fetch_all(&self.pool)
            .await?;
        let drops: Vec<ZoneDropRow> = sqlx::query_as(DROPS_QUERY)
            .bind(zone)
            .fetch_all(&self.pool)
            .await?;
        let observed: Vec<ZoneObservedRow> = sqlx::query_as(OBSERVED_QUERY)
            .bind(zone)
            .fetch_all(&self.pool)
            .await?;
        Ok(ZoneLoot::new(zone.to_owned(), npcs, drops, observed))
    }
}
