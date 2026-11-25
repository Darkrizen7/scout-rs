//! The reference package of the server: loot (derived from Forever Guide) and merchants. Applied from a
//! downloaded database, exported for the server from a filled one. Item names and categories seen by Scout
//! are game data and go in it; personal data (prices, favorites, recorded loot) never does.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};

use super::{Db, drops, vendor};

/// The loot (derived from Forever Guide), replaced as a whole (with `drop_meta`: fingerprint, date).
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

/// Merchants, added to what is there, which wins (those Scout saw).
const MERGE_VENDORS: &str = "
INSERT OR IGNORE INTO main.vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)
    SELECT item_id, source, npc_id, price, npc, zone, x, y, limited FROM seed.vendor_prices;
";

/// The package served to every app: loot, merchants and item names only, same schema as in the local database.
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
INSERT INTO main.items (item_id, name, class_id, subclass_id, class_name, subclass_name)
    SELECT item_id, name, class_id, subclass_id, class_name, subclass_name FROM seed.items
    WHERE name IS NOT NULL AND name <> '';
";

/// Item names and categories: added when absent, otherwise only the empty columns are filled (never overwritten).
const MERGE_ITEMS: &str = "
INSERT INTO main.items (item_id, name, class_id, subclass_id, class_name, subclass_name)
    SELECT item_id, name, class_id, subclass_id, class_name, subclass_name FROM seed.items WHERE true
ON CONFLICT(item_id) DO UPDATE SET
    name = COALESCE(NULLIF(main.items.name, ''), excluded.name),
    class_id = COALESCE(main.items.class_id, excluded.class_id),
    subclass_id = COALESCE(main.items.subclass_id, excluded.subclass_id),
    class_name = COALESCE(NULLIF(main.items.class_name, ''), excluded.class_name),
    subclass_name = COALESCE(NULLIF(main.items.subclass_name, ''), excluded.subclass_name);
";

/// Same schema as the local `items` table (names and categories only, no price).
const ITEMS_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS items (
    item_id       INTEGER PRIMARY KEY,
    name          TEXT,
    class_id      INTEGER,
    subclass_id   INTEGER,
    class_name    TEXT,
    subclass_name TEXT
);
";

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

impl Db {
    /// Replaces the loot (`drop_*`) by that of the server package at `path` and adds its merchants (those
    /// already there win), and fills the item names and categories missing. A package without loot is refused: the local loot is never emptied.
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
            sqlx::raw_sql(MERGE_ITEMS).execute(&mut *tx).await?;
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

    /// Makes a new database at `dest` with only the loot (`drop_*`), the merchants and the item names and
    /// categories of the database at `source`, which is only read: the package the server serves.
    /// No price, favorite or recorded loot.
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
            sqlx::raw_sql(ITEMS_SCHEMA).execute(&mut conn).await?;
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
