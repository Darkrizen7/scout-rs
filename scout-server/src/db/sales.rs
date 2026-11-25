//! `sales`: what sold per house, day (UTC) and item, summed over the day's comparisons.

use anyhow::Result;
use sqlx::SqliteConnection;

use super::Db;
use super::prices::Pending;
use crate::sales::ItemSales;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sales (
    house          INTEGER NOT NULL,
    day            TEXT    NOT NULL,  -- YYYY-MM-DD, UTC, of the newer snapshot
    item_id        INTEGER NOT NULL,
    sold_proven    INTEGER NOT NULL,  -- units: quantity that dropped on a listed auction
    sold_estimated INTEGER NOT NULL,  -- units: auctions with hours left that vanished (or were cancelled)
    sold_value     INTEGER NOT NULL,  -- copper: sum of units x unit price, priced sales only
    PRIMARY KEY (house, day, item_id)
) WITHOUT ROWID;
";

/// One row as served to the app.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct SalesRow {
    pub day: String,
    pub item_id: i64,
    pub sold_proven: i64,
    pub sold_estimated: i64,
    pub sold_value: i64,
}

/// Adds the sales of the snapshot made at `ts` to its day (inside the caller's transaction).
pub(super) async fn add(
    conn: &mut SqliteConnection,
    house: u32,
    ts: i64,
    sales: &[ItemSales],
) -> Result<()> {
    for s in sales {
        sqlx::query(
            "INSERT INTO sales (house, day, item_id, sold_proven, sold_estimated, sold_value)
             VALUES (?, date(?, 'unixepoch'), ?, ?, ?, ?)
             ON CONFLICT (house, day, item_id) DO UPDATE SET
                 sold_proven = sold_proven + excluded.sold_proven,
                 sold_estimated = sold_estimated + excluded.sold_estimated,
                 sold_value = sold_value + excluded.sold_value",
        )
        .bind(house)
        .bind(ts)
        .bind(s.item_id)
        .bind(s.sold_proven as i64)
        .bind(s.sold_estimated as i64)
        .bind(s.sold_value as i64)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

impl Db {
    /// The snapshot of `house`'s `kind` listing just before `ts`: its time and file (`None`: not kept).
    pub async fn previous_snapshot(
        &self,
        house: u32,
        kind: &str,
        ts: i64,
    ) -> Result<Option<(i64, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT fetched_at, path FROM raw_snapshots
             WHERE house = ? AND kind = ? AND fetched_at < ? ORDER BY fetched_at DESC LIMIT 1",
        )
        .bind(house)
        .bind(kind)
        .bind(ts)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Raw snapshots on disk whose sales were not computed yet (older than `sales`, or interrupted).
    pub async fn pending_sales(&self) -> Result<Vec<Pending>> {
        Ok(sqlx::query_as(
            "SELECT house, kind, fetched_at, path FROM raw_snapshots
             WHERE sales_done = 0 AND path IS NOT NULL ORDER BY fetched_at",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Sales of an already indexed snapshot (see `pending_sales`).
    pub async fn add_sales(&self, p: &Pending, sales: &[ItemSales]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        add(&mut tx, p.house, p.fetched_at, sales).await?;
        sqlx::query(
            "UPDATE raw_snapshots SET sales_done = 1 WHERE house = ? AND kind = ? AND fetched_at = ?",
        )
        .bind(p.house)
        .bind(&p.kind)
        .bind(p.fetched_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The sales of `house` from the day `since` (YYYY-MM-DD; default: the last 7 days), oldest first.
    pub async fn sales_since(&self, house: u32, since: Option<&str>) -> Result<Vec<SalesRow>> {
        Ok(sqlx::query_as(
            "SELECT day, item_id, sold_proven, sold_estimated, sold_value FROM sales
             WHERE house = ? AND day >= COALESCE(?, date('now', '-7 days')) ORDER BY day, item_id",
        )
        .bind(house)
        .bind(since)
        .fetch_all(&self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sales(item_id: u32, proven: u64) -> ItemSales {
        ItemSales {
            item_id,
            sold_proven: proven,
            sold_estimated: 1,
            sold_value: 10 * proven,
        }
    }

    #[tokio::test]
    async fn sales_add_up_within_a_day() {
        let db = Db::memory().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        // 2026-11-25 10:00 and 15:00 UTC, then the next day.
        add(&mut tx, 114, 1_795_600_000, &[sales(5, 2)])
            .await
            .unwrap();
        add(&mut tx, 114, 1_795_618_000, &[sales(5, 3), sales(6, 1)])
            .await
            .unwrap();
        add(&mut tx, 114, 1_795_700_000, &[sales(5, 1)])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let rows = db.sales_since(114, Some("2026-11-01")).await.unwrap();
        let first = SalesRow {
            day: "2026-11-25".into(),
            item_id: 5,
            sold_proven: 5,
            sold_estimated: 2,
            sold_value: 50,
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], first);
        assert_eq!(rows[2].day, "2026-11-26");
        let later = db.sales_since(114, Some("2026-11-26")).await.unwrap();
        assert_eq!(later.len(), 1);
        assert!(
            db.sales_since(113, Some("2026-11-01"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn old_databases_get_the_marker_column() {
        let db = Db::memory().await.unwrap();
        // A snapshot indexed before `sales`: the column is added and the snapshot is pending.
        sqlx::raw_sql("DROP TABLE raw_snapshots; DROP TABLE sales;
            CREATE TABLE raw_snapshots (house INTEGER NOT NULL, kind TEXT NOT NULL,
                fetched_at INTEGER NOT NULL, last_seen INTEGER NOT NULL, last_modified TEXT,
                sha256 BLOB NOT NULL, path TEXT, raw_bytes INTEGER NOT NULL, bytes INTEGER NOT NULL,
                auctions INTEGER NOT NULL, aggregated INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (house, kind, fetched_at));
            INSERT INTO raw_snapshots VALUES (114, 'auctions', 100, 100, NULL, x'01', 'a', 1, 1, 1, 1);")
            .execute(&db.pool)
            .await
            .unwrap();
        super::super::migrate(&db.pool).await.unwrap();
        let pending = db.pending_sales().await.unwrap();
        assert_eq!(pending.len(), 1);
        db.add_sales(&pending[0], &[sales(5, 1)]).await.unwrap();
        assert!(db.pending_sales().await.unwrap().is_empty());
        assert_eq!(
            db.sales_since(114, Some("1970-01-01")).await.unwrap().len(),
            1
        );
    }
}
