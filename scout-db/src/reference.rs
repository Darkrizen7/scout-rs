//! The reference database shipped with the tool: game tables, Wowhead items, Forever Guide's loot and the
//! merchants, so that the tool works without downloading the game data or installing Forever Guide.
//! Personal data (prices, item names seen by Scout, favorites, recorded loot) never goes in it.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use scout_core::GAME_FORMAT;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};

use super::{Db, drops, vendor};

/// Game tables, replaced as a whole (with `game_meta`: build, date, format).
const COPY_GAME: &str = "
DELETE FROM main.game_meta;
DELETE FROM main.game_items;
DELETE FROM main.game_categories;
DELETE FROM main.game_professions;
DELETE FROM main.game_recipes;
DELETE FROM main.game_reagents;
DELETE FROM main.game_recipe_items;
DELETE FROM main.game_maps;
DELETE FROM main.game_zone_factions;
INSERT INTO main.game_meta (key, value) SELECT key, value FROM seed.game_meta;
INSERT INTO main.game_items (item_id, name, quality, item_level, required_level, class_id, subclass_id,
                             buy_price, vendor_stack, sell_price)
    SELECT item_id, name, quality, item_level, required_level, class_id, subclass_id,
           buy_price, vendor_stack, sell_price FROM seed.game_items;
INSERT INTO main.game_categories (class_id, subclass_id, name)
    SELECT class_id, subclass_id, name FROM seed.game_categories;
INSERT INTO main.game_professions (id, name) SELECT id, name FROM seed.game_professions;
INSERT INTO main.game_recipes (spell_id, name, profession, required, yellow, grey, creates_item, creates_count)
    SELECT spell_id, name, profession, required, yellow, grey, creates_item, creates_count
    FROM seed.game_recipes;
INSERT INTO main.game_reagents (spell_id, slot, item_id, count)
    SELECT spell_id, slot, item_id, count FROM seed.game_reagents;
INSERT INTO main.game_recipe_items (spell_id, item_id) SELECT spell_id, item_id FROM seed.game_recipe_items;
INSERT INTO main.game_maps (id, name, parent, kind, x_min, x_max, y_min, y_max)
    SELECT id, name, parent, kind, x_min, x_max, y_min, y_max FROM seed.game_maps;
INSERT INTO main.game_zone_factions (name, faction) SELECT name, faction FROM seed.game_zone_factions;
";

/// The reference loot (derived from Forever Guide), replaced as a whole (with `drop_meta`: fingerprint, date).
const COPY_DROPS: &str = "
DELETE FROM main.drop_sources;
DELETE FROM main.drop_npcs;
DELETE FROM main.drop_names;
DELETE FROM main.drop_meta;
INSERT INTO main.drop_sources (item_id, kind, source_id, chance, quest_only, more)
    SELECT item_id, kind, source_id, chance, quest_only, more FROM seed.drop_sources;
INSERT INTO main.drop_npcs (npc_id, name, min_level, max_level, rank, zone, x, y, dungeon, ui_map)
    SELECT npc_id, name, min_level, max_level, rank, zone, x, y, dungeon, ui_map FROM seed.drop_npcs;
INSERT INTO main.drop_names (kind, id, name) SELECT kind, id, name FROM seed.drop_names;
INSERT INTO main.drop_meta (key, value) SELECT key, value FROM seed.drop_meta;
";

/// Added to what is there, which wins: items Wowhead named.
const MERGE: &str = "
INSERT OR IGNORE INTO main.wowhead_items (item_id, name, quality, item_level, required_level, class_id,
                                          subclass_id, buy_price, vendor_stack, sell_price)
    SELECT item_id, name, quality, item_level, required_level, class_id, subclass_id,
           buy_price, vendor_stack, sell_price FROM seed.wowhead_items;
";

/// Merchants, added to what is there, which wins (those Scout saw).
const MERGE_VENDORS: &str = "
INSERT OR IGNORE INTO main.vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)
    SELECT item_id, source, npc_id, price, npc, zone, x, y, limited FROM seed.vendor_prices;
";

/// The package served to every app: loot and merchants only, same schema as in the local database.
const COPY_LOOT_VENDORS: &str = "
INSERT INTO main.drop_sources (item_id, kind, source_id, chance, quest_only, more)
    SELECT item_id, kind, source_id, chance, quest_only, more FROM seed.drop_sources;
INSERT INTO main.drop_npcs (npc_id, name, min_level, max_level, rank, zone, x, y, dungeon, ui_map)
    SELECT npc_id, name, min_level, max_level, rank, zone, x, y, dungeon, ui_map FROM seed.drop_npcs;
INSERT INTO main.drop_names (kind, id, name) SELECT kind, id, name FROM seed.drop_names;
INSERT INTO main.drop_meta (key, value) SELECT key, value FROM seed.drop_meta;
INSERT INTO main.vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)
    SELECT item_id, source, npc_id, price, npc, zone, x, y, limited FROM seed.vendor_prices;
INSERT OR IGNORE INTO main.drop_meta (key, value)
    SELECT 'updated', CAST(unixepoch() AS TEXT) WHERE EXISTS (SELECT 1 FROM main.drop_sources);
";

const LOCAL_GAME: &str = "SELECT key, value FROM main.game_meta WHERE key IN ('updated', 'format')";
const SEED_GAME: &str = "SELECT key, value FROM seed.game_meta WHERE key IN ('updated', 'format')";
const LOCAL_DROPS: &str = "SELECT key, value FROM main.drop_meta WHERE key = 'updated'";
const SEED_DROPS: &str = "SELECT key, value FROM seed.drop_meta WHERE key = 'updated'";

/// What a reference database replaced (merchants and Wowhead items are always added).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReferenceUpdate {
    pub game: bool,
    pub drops: bool,
}

/// `updated` and `format` of a meta table (Unix seconds, format number).
#[derive(Debug, Default)]
struct Stamp {
    updated: Option<i64>,
    format: Option<i64>,
}

async fn stamp(conn: &mut SqliteConnection, sql: &'static str) -> Result<Stamp> {
    let rows: Vec<(String, String)> = sqlx::query_as(sql).fetch_all(conn).await?;
    let get = |k: &str| {
        rows.iter()
            .find(|(key, _)| key == k)
            .and_then(|(_, v)| v.parse().ok())
    };
    Ok(Stamp {
        updated: get("updated"),
        format: get("format"),
    })
}

/// Runs `job` with the database at `path` attached as `seed`, detached afterwards even on failure.
async fn with_seed<T>(
    conn: &mut SqliteConnection,
    path: &Path,
    job: impl AsyncFnOnce(&mut SqliteConnection) -> Result<T>,
) -> Result<T> {
    let path = path.to_str().context("chemin de la base de référence")?;
    sqlx::query("ATTACH DATABASE ? AS seed")
        .bind(path)
        .execute(&mut *conn)
        .await?;
    let result = job(conn).await;
    sqlx::query("DETACH DATABASE seed")
        .execute(&mut *conn)
        .await?;
    result
}

/// Copies the attached `seed` into `main`: each part when `force` or newer than the local one.
async fn copy(conn: &mut SqliteConnection, force: bool) -> Result<ReferenceUpdate> {
    let (local_game, seed_game) = (
        stamp(conn, LOCAL_GAME).await?,
        stamp(conn, SEED_GAME).await?,
    );
    let (local_drops, seed_drops) = (
        stamp(conn, LOCAL_DROPS).await?,
        stamp(conn, SEED_DROPS).await?,
    );
    // Game data of an older format is useless: the tool asks for a new download instead.
    let game = seed_game.format == Some(GAME_FORMAT)
        && (force
            || local_game.format != Some(GAME_FORMAT)
            || local_game.updated < seed_game.updated);
    // Drops imported before the date was kept count as older than any reference.
    let drops = force || (seed_drops.updated.is_some() && local_drops.updated < seed_drops.updated);
    let mut tx = conn.begin().await?;
    if game {
        sqlx::raw_sql(COPY_GAME).execute(&mut *tx).await?;
    }
    if drops {
        sqlx::raw_sql(COPY_DROPS).execute(&mut *tx).await?;
    }
    sqlx::raw_sql(MERGE).execute(&mut *tx).await?;
    sqlx::raw_sql(MERGE_VENDORS).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(ReferenceUpdate { game, drops })
}

impl Db {
    /// Copies from the reference database at `path` the game data and loot newer than the local ones,
    /// and adds its merchants and Wowhead items.
    pub async fn apply_reference(&self, path: &Path) -> Result<ReferenceUpdate> {
        let mut conn = self.pool.acquire().await?;
        with_seed(&mut conn, path, async |conn| copy(conn, false).await).await
    }

    /// Replaces the loot (`drop_*`) by that of the server package at `path` and adds its merchants (those
    /// already there win). A package without loot is refused: the local loot is never emptied.
    pub async fn apply_loot_vendors(&self, path: &Path) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        with_seed(&mut conn, path, async |conn| {
            let (loot,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM seed.drop_sources")
                .fetch_one(&mut *conn)
                .await?;
            ensure!(loot > 0, "le paquet de butin du serveur est vide");
            let mut tx = conn.begin().await?;
            sqlx::raw_sql(COPY_DROPS).execute(&mut *tx).await?;
            sqlx::raw_sql(MERGE_VENDORS).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    /// Version (format, updated) of the server package last applied (see `set_server_reference`).
    pub async fn server_reference(&self) -> Result<Option<(i64, i64)>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM meta WHERE key = 'server_reference'")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(v,)| {
            let (format, updated) = v.split_once(':')?;
            Some((format.parse().ok()?, updated.parse().ok()?))
        }))
    }

    pub async fn set_server_reference(&self, format: i64, updated: i64) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES ('server_reference', ?)")
            .bind(format!("{format}:{updated}"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Id of the reference database last applied (see `set_reference_id`).
    pub async fn reference_id(&self) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM meta WHERE key = 'reference'")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    pub async fn set_reference_id(&self, id: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES ('reference', ?)")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Makes a new reference database at `dest` from the database at `source` (which needs its game data).
    pub async fn export_reference(source: &Path, dest: &Path) -> Result<ReferenceUpdate> {
        ensure!(!dest.exists(), "{} existe déjà", dest.display());
        let db = Self::open(dest).await?;
        let result = async {
            let mut conn = db.pool.acquire().await?;
            let copied = with_seed(&mut conn, source, async |conn| copy(conn, true).await).await?;
            ensure!(
                copied.game,
                "pas de données du jeu au format {GAME_FORMAT} dans {}",
                source.display()
            );
            // Drops imported before their date was kept: dated now.
            sqlx::query(
                "INSERT OR IGNORE INTO drop_meta (key, value)
                 SELECT 'updated', CAST(unixepoch() AS TEXT) WHERE EXISTS (SELECT 1 FROM drop_sources)",
            )
            .execute(&mut *conn)
            .await?;
            sqlx::raw_sql("VACUUM").execute(&mut *conn).await?;
            Ok(copied)
        }
        .await;
        db.pool.close().await;
        result
    }

    /// Makes a new database at `dest` with only the loot (`drop_*`) and the merchants of the database at
    /// `source`, which is only read: the package the server serves. No price, favorite or recorded loot.
    pub async fn export_loot_vendors(source: &Path, dest: &Path) -> Result<()> {
        ensure!(!dest.exists(), "{} existe déjà", dest.display());
        let options = SqliteConnectOptions::new()
            .filename(dest)
            .create_if_missing(true);
        let mut conn = SqliteConnection::connect_with(&options).await?;
        let result = async {
            sqlx::raw_sql(drops::REFERENCE_SCHEMA)
                .execute(&mut conn)
                .await?;
            sqlx::raw_sql(vendor::SCHEMA).execute(&mut conn).await?;
            with_seed(&mut conn, source, async |conn| {
                let mut tx = conn.begin().await?;
                sqlx::raw_sql(COPY_LOOT_VENDORS).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(())
            })
            .await?;
            let (loot,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM drop_sources")
                .fetch_one(&mut conn)
                .await?;
            ensure!(loot > 0, "pas de butin dans {}", source.display());
            sqlx::raw_sql("VACUUM").execute(&mut conn).await?;
            Ok(())
        }
        .await;
        conn.close().await?;
        result
    }

    /// Closes the connection (a file still open cannot be deleted on Windows).
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests;
