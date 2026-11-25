//! `prices`: one row per item for every new content of a listing (what the app downloads).

use anyhow::Result;
use sqlx::SqliteConnection;

use super::Db;
use crate::aggregate::ItemPrices;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS prices (
    house       INTEGER NOT NULL,
    ts          INTEGER NOT NULL,  -- fetched_at of the raw snapshot, Unix seconds
    item_id     INTEGER NOT NULL,  -- an item is either in auctions or in commodities, never both
    min_unit    INTEGER,           -- copper; NULL when every auction is bid-only
    p25_unit    INTEGER,           -- percentiles weighted by quantity
    median_unit INTEGER,
    quantity    INTEGER NOT NULL,  -- units on sale, bid-only auctions included
    auctions    INTEGER NOT NULL,
    buyout_less INTEGER NOT NULL,  -- bid-only auctions, left out of the prices
    PRIMARY KEY (house, ts, item_id)
) WITHOUT ROWID;
";

/// A raw snapshot written before its prices were (written by an older version, or interrupted).
#[derive(Debug, sqlx::FromRow)]
pub struct Pending {
    pub house: u32,
    pub kind: String,
    pub fetched_at: i64,
    pub path: String,
}

/// Inserts the prices of one snapshot (inside the caller's transaction).
pub(super) async fn insert(
    conn: &mut SqliteConnection,
    house: u32,
    ts: i64,
    rows: &[ItemPrices],
) -> Result<()> {
    for row in rows {
        sqlx::query(
            "INSERT OR REPLACE INTO prices (house, ts, item_id, min_unit, p25_unit, median_unit,
                                            quantity, auctions, buyout_less)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(house)
        .bind(ts)
        .bind(row.item_id)
        .bind(row.min_unit.map(|p| p as i64))
        .bind(row.p25_unit.map(|p| p as i64))
        .bind(row.median_unit.map(|p| p as i64))
        .bind(row.quantity as i64)
        .bind(row.auctions)
        .bind(row.buyout_less)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

impl Db {
    /// Raw snapshots on disk whose prices are not in `prices` yet.
    pub async fn pending_prices(&self) -> Result<Vec<Pending>> {
        Ok(sqlx::query_as(
            "SELECT house, kind, fetched_at, path FROM raw_snapshots
             WHERE aggregated = 0 AND path IS NOT NULL ORDER BY fetched_at",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Prices of an already indexed snapshot (see `pending_prices`).
    pub async fn add_prices(&self, p: &Pending, rows: &[ItemPrices]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        insert(&mut tx, p.house, p.fetched_at, rows).await?;
        sqlx::query(
            "UPDATE raw_snapshots SET aggregated = 1 WHERE house = ? AND kind = ? AND fetched_at = ?",
        )
        .bind(p.house)
        .bind(&p.kind)
        .bind(p.fetched_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
