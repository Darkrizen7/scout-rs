//! `prices`: one row per item for every new content of a listing (what the app downloads).

use std::sync::Arc;

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
    quantity    INTEGER NOT NULL,  -- units on sale, bid-only auctions included; 0 = no longer listed
    auctions    INTEGER NOT NULL,
    buyout_less INTEGER NOT NULL,  -- bid-only auctions, left out of the prices
    PRIMARY KEY (house, ts, item_id)
) WITHOUT ROWID;
-- One item's history (the app's market sheet).
CREATE INDEX IF NOT EXISTS prices_item ON prices (house, item_id, ts);
";

/// A raw snapshot written before its prices were (written by an older version, or interrupted).
#[derive(Debug, sqlx::FromRow)]
pub struct Pending {
    pub house: u32,
    pub kind: String,
    pub fetched_at: i64,
    pub path: String,
}

/// One row as served to the app.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct PriceRow {
    pub ts: i64,
    pub item_id: i64,
    pub min_unit: Option<i64>,
    pub p25_unit: Option<i64>,
    pub median_unit: Option<i64>,
    pub quantity: i64,
    pub auctions: i64,
    pub buyout_less: i64,
}

/// A page of prices: whole snapshots only, so that the next page starts after `until`.
#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<PriceRow>,
    /// Newest `ts` of the page (`since` when it is empty).
    pub until: i64,
    /// Newer snapshots remain.
    pub more: bool,
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
    /// The prices of `house` after `since`, at most about `max_rows` (but always one whole snapshot).
    pub async fn prices_after(&self, house: u32, since: i64, max_rows: i64) -> Result<Page> {
        let until: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(MAX(CASE WHEN total <= ? THEN ts END), MIN(ts)) FROM (
                 SELECT ts, SUM(COUNT(*)) OVER (ORDER BY ts) AS total
                 FROM prices WHERE house = ? AND ts > ? GROUP BY ts)",
        )
        .bind(max_rows)
        .bind(house)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        let Some(until) = until else {
            return Ok(Page {
                rows: Vec::new(),
                until: since,
                more: false,
            });
        };
        let rows = sqlx::query_as(
            "SELECT ts, item_id, min_unit, p25_unit, median_unit, quantity, auctions, buyout_less
             FROM prices WHERE house = ? AND ts > ? AND ts <= ? ORDER BY ts, item_id",
        )
        .bind(house)
        .bind(since)
        .bind(until)
        .fetch_all(&self.pool)
        .await?;
        let more: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM prices WHERE house = ? AND ts > ?)")
                .bind(house)
                .bind(until)
                .fetch_one(&self.pool)
                .await?;
        Ok(Page { rows, until, more })
    }

    /// The newest version of both listings of `house`: every item on sale, plus those that just left.
    ///
    /// Cached until a write changes it (`insert_snapshot`, `add_prices`).
    pub async fn latest(&self, house: u32) -> Result<Arc<[PriceRow]>> {
        let generation = match self.latest.get(house) {
            Ok(rows) => return Ok(rows),
            Err(generation) => generation,
        };
        let rows = sqlx::query_as(
            "SELECT ts, item_id, min_unit, p25_unit, median_unit, quantity, auctions, buyout_less
             FROM prices WHERE house = ? AND ts IN (
                 SELECT MAX(fetched_at) FROM raw_snapshots
                 WHERE house = ? AND aggregated = 1 GROUP BY kind)
             ORDER BY item_id",
        )
        .bind(house)
        .bind(house)
        .fetch_all(&self.pool)
        .await?;
        Ok(self.latest.put(house, generation, rows))
    }

    /// Every row of one item after `since`, oldest first.
    pub async fn item_history(
        &self,
        house: u32,
        item_id: u32,
        since: i64,
    ) -> Result<Vec<PriceRow>> {
        Ok(sqlx::query_as(
            "SELECT ts, item_id, min_unit, p25_unit, median_unit, quantity, auctions, buyout_less
             FROM prices WHERE house = ? AND item_id = ? AND ts > ? ORDER BY ts",
        )
        .bind(house)
        .bind(item_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Items on sale in the version of `house`'s `kind` listing just before `ts`.
    pub async fn listed_before(&self, house: u32, kind: &str, ts: i64) -> Result<Vec<u32>> {
        Ok(sqlx::query_scalar(
            "SELECT item_id FROM prices WHERE house = ? AND quantity > 0 AND ts = (
                 SELECT MAX(fetched_at) FROM raw_snapshots
                 WHERE house = ? AND kind = ? AND fetched_at < ?)",
        )
        .bind(house)
        .bind(house)
        .bind(kind)
        .bind(ts)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Newest snapshot time of `house`, any listing: a new one must come after it, so that `ts`
    /// tells the two listings apart.
    pub async fn newest_fetch(&self, house: u32) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT MAX(fetched_at) FROM raw_snapshots WHERE house = ?")
                .bind(house)
                .fetch_one(&self.pool)
                .await?,
        )
    }

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
        self.latest.invalidate(p.house);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(item_id: u32) -> ItemPrices {
        ItemPrices {
            item_id,
            min_unit: Some(10),
            p25_unit: Some(11),
            median_unit: None,
            quantity: 3,
            auctions: 2,
            buyout_less: 1,
        }
    }

    #[tokio::test]
    async fn latest_is_cached_until_a_new_snapshot() {
        let db = Db::memory().await.unwrap();
        let snapshot = |fetched_at| super::super::NewSnapshot {
            house: 114,
            kind: "auctions",
            fetched_at,
            last_modified: None,
            sha256: &[1],
            path: None,
            raw_bytes: 0,
            bytes: 0,
            auctions: 1,
        };
        db.insert_snapshot(&snapshot(100), &[row(1)]).await.unwrap();
        let first = db.latest(114).await.unwrap();
        assert_eq!(first.len(), 1);
        assert!(Arc::ptr_eq(&first, &db.latest(114).await.unwrap()));
        // Seeing the same content again changes nothing.
        db.touch_snapshot(114, "auctions", 100, 200).await.unwrap();
        assert!(Arc::ptr_eq(&first, &db.latest(114).await.unwrap()));

        db.insert_snapshot(&snapshot(300), &[row(1), row(2)])
            .await
            .unwrap();
        let second = db.latest(114).await.unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.iter().map(|r| r.ts).collect::<Vec<_>>(), [300, 300]);
    }

    #[tokio::test]
    async fn pages_hold_whole_snapshots() {
        let db = Db::memory().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        insert(&mut tx, 114, 100, &[row(1), row(2)]).await.unwrap();
        insert(&mut tx, 114, 200, &[row(1), row(2), row(3)])
            .await
            .unwrap();
        insert(&mut tx, 114, 300, &[row(1)]).await.unwrap();
        insert(&mut tx, 113, 150, &[row(9)]).await.unwrap();
        tx.commit().await.unwrap();

        // 4 rows allowed: the first snapshot (2) fits, the second (5 in all) does not.
        let page = db.prices_after(114, 0, 4).await.unwrap();
        assert_eq!((page.rows.len(), page.until, page.more), (2, 100, true));
        assert_eq!(page.rows[0].median_unit, None);
        assert_eq!(page.rows[1].item_id, 2);
        // A snapshot bigger than the page still comes whole.
        let page = db.prices_after(114, 100, 1).await.unwrap();
        assert_eq!((page.rows.len(), page.until, page.more), (3, 200, true));
        let page = db.prices_after(114, 200, 1000).await.unwrap();
        assert_eq!((page.rows.len(), page.until, page.more), (1, 300, false));
        let page = db.prices_after(114, 300, 1000).await.unwrap();
        assert_eq!((page.rows.len(), page.until, page.more), (0, 300, false));
    }
}
