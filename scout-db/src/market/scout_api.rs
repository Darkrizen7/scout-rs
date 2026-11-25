//! Prices from our price server (source `scout-api`): the newest stored price of each item, and how
//! far each item's history was downloaded (a cache, refreshed once it is an hour old).

use std::collections::HashMap;

use anyhow::Result;
use scout_core::scout_api::LastPrice;

use crate::Db;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS scout_api_history (
    item_id    INTEGER PRIMARY KEY,
    house      INTEGER NOT NULL,  -- the server's auction house it was downloaded for
    until      INTEGER NOT NULL,  -- newest ts received; the next download asks for what follows
    checked_at INTEGER NOT NULL   -- when it was last asked (Unix seconds)
);
";

/// How far an item's history was downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct HistoryCache {
    pub house: i64,
    pub until: i64,
    pub checked_at: i64,
}

impl Db {
    /// Newest stored Scout API price of each item.
    pub async fn last_api_prices(&self) -> Result<HashMap<i64, LastPrice>> {
        let rows: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT s.item_id, s.ts, s.price FROM snapshots s
             JOIN (SELECT item_id, MAX(ts) AS ts FROM snapshots WHERE source = 'scout-api'
                   GROUP BY item_id) m ON m.item_id = s.item_id AND m.ts = s.ts
             WHERE s.source = 'scout-api'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(item_id, ts, price)| (item_id, LastPrice { ts, price }))
            .collect())
    }

    pub async fn api_history(&self, item_id: i64) -> Result<Option<HistoryCache>> {
        Ok(sqlx::query_as(
            "SELECT house, until, checked_at FROM scout_api_history WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn set_api_history(&self, item_id: i64, cache: HistoryCache) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO scout_api_history (item_id, house, until, checked_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(cache.house)
        .bind(cache.until)
        .bind(cache.checked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use scout_core::Row;

    use super::*;

    #[tokio::test]
    async fn remembers_the_newest_price_and_the_history_cache() {
        let dir = std::env::temp_dir().join(format!("scout-rs-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).await.unwrap();
        let row = |ts, price| Row {
            source: "scout-api".into(),
            item_id: 765,
            name: None,
            timestamp: ts,
            price,
            quantity: Some(1),
        };
        let scout = Row {
            source: "scout".into(),
            ..row(300, Some(1))
        };
        db.insert(&[row(100, Some(42)), row(200, None), scout])
            .await
            .unwrap();
        let last = db.last_api_prices().await.unwrap();
        assert_eq!(
            last[&765],
            LastPrice {
                ts: 200,
                price: None
            }
        );

        assert_eq!(db.api_history(765).await.unwrap(), None);
        let cache = HistoryCache {
            house: 114,
            until: 200,
            checked_at: 1_795_596_469,
        };
        db.set_api_history(765, cache).await.unwrap();
        assert_eq!(db.api_history(765).await.unwrap(), Some(cache));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
