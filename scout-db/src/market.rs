//! Auction house prices: items, favorites and every scan ever imported.

use anyhow::Result;
use scout_core::analysis::Observation;
use scout_core::{ItemName, ItemSummary, Row, Snapshot};

use super::Db;

impl Db {
    pub async fn favorites(&self) -> Result<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT item_id FROM favorites")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn set_favorite(&self, item_id: i64, favorite: bool) -> Result<()> {
        let sql = if favorite {
            "INSERT OR IGNORE INTO favorites (item_id) VALUES (?)"
        } else {
            "DELETE FROM favorites WHERE item_id = ?"
        };
        sqlx::query(sql).bind(item_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Inserts the rows not already stored; returns how many scans were new.
    pub async fn insert(&self, rows: &[Row]) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        let mut added = 0;
        for row in rows {
            let name = row.name.as_deref().filter(|n| !n.is_empty());
            sqlx::query(
                "INSERT INTO items (item_id, name) VALUES (?, ?)
                 ON CONFLICT (item_id) DO UPDATE SET name = COALESCE(excluded.name, items.name)",
            )
            .bind(row.item_id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
            added += sqlx::query(
                "INSERT OR IGNORE INTO snapshots (item_id, source, ts, price, quantity) VALUES (?, ?, ?, ?, ?)",
            )
                .bind(row.item_id)
                .bind(&row.source)
                .bind(row.timestamp)
                .bind(row.price)
                .bind(row.quantity)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Names Scout saw; they also name the items imported from other addons (same item ids).
    pub async fn set_names(&self, names: &[ItemName]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for n in names.iter().filter(|n| !n.name.is_empty()) {
            sqlx::query(
                "INSERT INTO items (item_id, name, class_id, subclass_id, class_name, subclass_name)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT (item_id) DO UPDATE SET
                     name = excluded.name,
                     class_id = COALESCE(excluded.class_id, items.class_id),
                     subclass_id = COALESCE(excluded.subclass_id, items.subclass_id),
                     class_name = COALESCE(excluded.class_name, items.class_name),
                     subclass_name = COALESCE(excluded.subclass_name, items.subclass_name)",
            )
            .bind(n.item_id)
            .bind(&n.name)
            .bind(n.class_id)
            .bind(n.subclass_id)
            .bind(n.class.as_deref().filter(|c| !c.is_empty()))
            .bind(n.subclass.as_deref().filter(|c| !c.is_empty()))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn items(&self) -> Result<Vec<ItemSummary>> {
        let items = sqlx::query_as(
            // Latest snapshot of each item, any source (Scout wins a tie).
            "SELECT i.item_id, i.name, i.class_id, i.subclass_id, i.class_name, i.subclass_name,
                    s.price, s.quantity, s.ts, s.scans
             FROM items i
             JOIN (SELECT item_id, price, quantity, ts,
                          COUNT(*) OVER w AS scans,
                          ROW_NUMBER() OVER (w ORDER BY ts DESC, source = 'scout' DESC) AS rank
                   FROM snapshots
                   WINDOW w AS (PARTITION BY item_id)) s
               ON s.item_id = i.item_id AND s.rank = 1",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(items)
    }

    /// Every priced snapshot, for the analysis.
    pub async fn observations(&self) -> Result<Vec<Observation>> {
        let rows: Vec<(i64, i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT item_id, ts, price, quantity FROM snapshots WHERE price IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(item_id, ts, price, quantity)| Observation {
                item_id,
                ts,
                price,
                quantity,
            })
            .collect())
    }

    pub async fn history(&self, item_id: i64) -> Result<Vec<Snapshot>> {
        let history = sqlx::query_as(
            "SELECT source, ts, price, quantity FROM snapshots WHERE item_id = ? ORDER BY ts",
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(history)
    }
}

#[cfg(test)]
mod tests {
    use scout_core::SCOUT;

    use super::*;

    fn row(
        item_id: i64,
        name: Option<&str>,
        timestamp: i64,
        price: Option<i64>,
        quantity: i64,
    ) -> Row {
        Row {
            source: SCOUT.to_owned(),
            item_id,
            name: name.map(str::to_owned),
            timestamp,
            price,
            quantity: Some(quantity),
        }
    }

    #[tokio::test]
    async fn keeps_names_and_ignores_duplicate_scans() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("scout-rs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).await?;

        assert_eq!(
            db.insert(&[row(1, Some("Lin"), 100, Some(50), 3)]).await?,
            1
        );
        // Same scan again (no name this time) plus a newer one where the item is missing.
        assert_eq!(
            db.insert(&[row(1, None, 100, Some(50), 3), row(1, None, 200, None, 0)])
                .await?,
            1
        );

        let items = db.items().await?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name.as_deref(), Some("Lin"));
        assert_eq!(
            (items[0].ts, items[0].price, items[0].scans),
            (200, None, 2)
        );
        assert_eq!(db.history(1).await?.len(), 2);

        // Same time from another addon: a separate snapshot, the newest one wins in the list.
        let mut legacy = row(1, None, 300, Some(40), 0);
        legacy.source = "auctionator".into();
        legacy.quantity = None;
        assert_eq!(db.insert(&[legacy]).await?, 1);
        let items = db.items().await?;
        assert_eq!(
            (
                items[0].ts,
                items[0].price,
                items[0].quantity,
                items[0].scans
            ),
            (300, Some(40), None, 3)
        );

        // A name learnt later (item first imported from another addon) is applied.
        db.set_names(&[ItemName {
            item_id: 1,
            name: "Tissu de lin".into(),
            class_id: Some(7),
            subclass_id: Some(5),
            class: Some("Artisanat".into()),
            subclass: Some("Tissu".into()),
        }])
        .await?;
        let item = &db.items().await?[0];
        assert_eq!(item.name.as_deref(), Some("Tissu de lin"));
        assert_eq!(
            (item.class_id, item.subclass_name.as_deref()),
            (Some(7), Some("Tissu"))
        );

        db.set_favorite(1, true).await?;
        db.set_favorite(1, true).await?;
        assert_eq!(db.favorites().await?, vec![1]);
        db.set_favorite(1, false).await?;
        assert!(db.favorites().await?.is_empty());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
