//! Daily sales totals of the price server, per auction house: aggregates of the server (not the user's
//! data), so the oldest are simply dropped.

use anyhow::Result;
use scout_core::sales::{SalesDay, SalesItem, SalesRow, WINDOW_DAYS};
use scout_core::scout_api::AuctionHouse;

use crate::Db;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sales (
    region         TEXT    NOT NULL,
    house          INTEGER NOT NULL,
    day            TEXT    NOT NULL,  -- UTC day, YYYY-MM-DD
    item_id        INTEGER NOT NULL,
    sold_proven    INTEGER NOT NULL,
    sold_estimated INTEGER NOT NULL,
    sold_value     INTEGER NOT NULL,  -- copper, sales with a price only
    PRIMARY KEY (region, house, day, item_id)
) WITHOUT ROWID;
";

impl Db {
    /// Stores the days received, each replacing the rows of that day (the server sends running totals),
    /// then forgets what is more than 30 days older than the newest day.
    pub async fn replace_sales(&self, house: &AuctionHouse, days: &[SalesDay]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for day in days {
            sqlx::query("DELETE FROM sales WHERE region = ? AND house = ? AND day = ?")
                .bind(&house.region)
                .bind(house.id)
                .bind(&day.day)
                .execute(&mut *tx)
                .await?;
            for item in &day.items {
                sqlx::query("INSERT INTO sales VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&house.region)
                    .bind(house.id)
                    .bind(&day.day)
                    .bind(item.item_id)
                    .bind(item.sold_proven)
                    .bind(item.sold_estimated)
                    .bind(item.sold_value)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("DELETE FROM sales WHERE day < date((SELECT MAX(day) FROM sales), '-30 days')")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The rows of the last `WINDOW_DAYS` days that `house` has.
    pub async fn sales_rows(&self, house: &AuctionHouse) -> Result<Vec<SalesRow>> {
        let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT day, item_id, sold_proven, sold_estimated, sold_value FROM sales
             WHERE region = ?1 AND house = ?2
               AND day IN (SELECT DISTINCT day FROM sales WHERE region = ?1 AND house = ?2
                           ORDER BY day DESC LIMIT ?3)",
        )
        .bind(&house.region)
        .bind(house.id)
        .bind(WINDOW_DAYS)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(day, item_id, sold_proven, sold_estimated, sold_value)| {
                (
                    day,
                    SalesItem {
                        item_id,
                        sold_proven,
                        sold_estimated,
                        sold_value,
                    },
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(day: &str, proven: i64) -> SalesDay {
        SalesDay {
            day: day.into(),
            items: vec![SalesItem {
                item_id: 7,
                sold_proven: proven,
                sold_estimated: 0,
                sold_value: 10,
            }],
        }
    }

    #[tokio::test]
    async fn days_are_replaced_and_old_ones_dropped() {
        let dir = std::env::temp_dir().join(format!("scout-rs-sales-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).await.unwrap();
        let eu = AuctionHouse {
            region: "eu".into(),
            id: 114,
        };
        let other = AuctionHouse {
            region: "eu".into(),
            id: 115,
        };
        db.replace_sales(&eu, &[day("2026-10-01", 1), day("2026-11-24", 2)])
            .await
            .unwrap();
        db.replace_sales(&eu, &[day("2026-11-24", 5), day("2026-11-25", 3)])
            .await
            .unwrap();
        db.replace_sales(&other, &[day("2026-11-25", 9)])
            .await
            .unwrap();
        let mut rows = db.sales_rows(&eu).await.unwrap();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        // 2026-10-01 is more than 30 days old; 2026-11-24 was replaced, not added to.
        let seen: Vec<_> = rows
            .iter()
            .map(|(d, i)| (d.as_str(), i.sold_proven))
            .collect();
        assert_eq!(seen, [("2026-11-24", 5), ("2026-11-25", 3)]);
        assert_eq!(db.sales_rows(&other).await.unwrap().len(), 1);
    }
}
