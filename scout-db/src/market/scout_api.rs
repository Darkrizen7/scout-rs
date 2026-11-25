//! Prices from our price server (source `scout-api`), per auction house: the newest stored price of
//! each item, and how far each item's history was downloaded (a cache, refreshed once an hour old).

use std::collections::HashMap;

use anyhow::Result;
use scout_core::scout_api::{AuctionHouse, LastPrice};

use crate::Db;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS scout_api_history (
    item_id    INTEGER NOT NULL,
    region     TEXT    NOT NULL,
    house      INTEGER NOT NULL,
    until      INTEGER NOT NULL,  -- newest ts received; the next download asks for what follows
    checked_at INTEGER NOT NULL,  -- when it was last asked (Unix seconds)
    PRIMARY KEY (item_id, region, house)
) WITHOUT ROWID;
";

/// How far an item's history was downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct HistoryCache {
    pub until: i64,
    pub checked_at: i64,
}

impl Db {
    /// Newest stored Scout API price of each item at `house`.
    pub async fn last_api_prices(&self, house: &AuctionHouse) -> Result<HashMap<i64, LastPrice>> {
        let rows: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT s.item_id, s.ts, s.price FROM snapshots s
             JOIN (SELECT item_id, MAX(ts) AS ts FROM snapshots
                   WHERE source = 'scout-api' AND region = ?1 AND house = ?2
                   GROUP BY item_id) m ON m.item_id = s.item_id AND m.ts = s.ts
             WHERE s.source = 'scout-api' AND s.region = ?1 AND s.house = ?2",
        )
        .bind(&house.region)
        .bind(house.id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(item_id, ts, price)| (item_id, LastPrice { ts, price }))
            .collect())
    }

    pub async fn api_history(
        &self,
        item_id: i64,
        house: &AuctionHouse,
    ) -> Result<Option<HistoryCache>> {
        Ok(sqlx::query_as(
            "SELECT until, checked_at FROM scout_api_history
             WHERE item_id = ? AND region = ? AND house = ?",
        )
        .bind(item_id)
        .bind(&house.region)
        .bind(house.id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn set_api_history(
        &self,
        item_id: i64,
        house: &AuctionHouse,
        cache: HistoryCache,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO scout_api_history (item_id, region, house, until, checked_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(&house.region)
        .bind(house.id)
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

    fn house(region: &str, id: u32) -> AuctionHouse {
        AuctionHouse {
            region: region.into(),
            id,
        }
    }

    #[tokio::test]
    async fn prices_and_caches_stay_with_their_house() {
        let dir = std::env::temp_dir().join(format!("scout-rs-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).await.unwrap();
        let row = |source: &str, at: &AuctionHouse, ts, price| Row {
            source: source.into(),
            region: at.region.clone(),
            house: i64::from(at.id),
            item_id: 765,
            name: None,
            timestamp: ts,
            price,
            quantity: Some(1),
            realm: None,
            faction: None,
        };
        let (eu, us, none) = (house("eu", 114), house("us", 114), AuctionHouse::default());
        db.insert(&[
            row("scout-api", &eu, 100, Some(42)),
            row("scout-api", &eu, 200, None),
            row("scout-api", &us, 300, Some(9)),
            row("scout", &none, 150, Some(40)),
        ])
        .await
        .unwrap();
        let last = db.last_api_prices(&eu).await.unwrap();
        assert_eq!(
            last[&765],
            LastPrice {
                ts: 200,
                price: None
            }
        );
        assert_eq!(db.last_api_prices(&us).await.unwrap()[&765].ts, 300);

        // The Scout scan made before any house was chosen goes to the first one chosen.
        assert_eq!(db.history(765, &eu).await.unwrap().len(), 2);
        assert_eq!(db.claim_unassigned(&eu).await.unwrap(), 1);
        assert_eq!(db.claim_unassigned(&us).await.unwrap(), 0);
        assert_eq!(db.history(765, &eu).await.unwrap().len(), 3);
        assert_eq!(db.history(765, &us).await.unwrap().len(), 1);

        assert_eq!(db.api_history(765, &eu).await.unwrap(), None);
        let cache = HistoryCache {
            until: 200,
            checked_at: 1_795_596_469,
        };
        db.set_api_history(765, &eu, cache).await.unwrap();
        assert_eq!(db.api_history(765, &eu).await.unwrap(), Some(cache));
        assert_eq!(db.api_history(765, &us).await.unwrap(), None);
        // Windows cannot delete a file that is still open.
        db.pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
