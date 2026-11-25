//! Databases made before the auction houses were told apart (2026-11-25): `snapshots` gets its
//! `region` and `house` columns (part of the key, so the table is copied), and the item history
//! cache of the price server is keyed by house too. Auctionator and Forever Guide readings, no
//! longer imported, are removed.

use anyhow::Result;
use sqlx::SqlitePool;

/// Every row is kept, with no house (`''` / 0) until one is chosen (`Db::claim_unassigned`).
pub(crate) async fn add_houses(pool: &SqlitePool) -> Result<()> {
    let has_house: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('snapshots') WHERE name = 'house'",
    )
    .fetch_one(pool)
    .await?;
    if has_house > 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::raw_sql(
        "CREATE TABLE snapshots_houses (
             item_id  INTEGER NOT NULL REFERENCES items(item_id),
             source   TEXT    NOT NULL,
             region   TEXT    NOT NULL DEFAULT '',
             house    INTEGER NOT NULL DEFAULT 0,
             ts       INTEGER NOT NULL,
             price    INTEGER,
             quantity INTEGER,
             PRIMARY KEY (item_id, region, house, source, ts)
         ) WITHOUT ROWID;
         INSERT INTO snapshots_houses (item_id, source, ts, price, quantity)
             SELECT item_id, source, ts, price, quantity FROM snapshots;
         DROP TABLE snapshots;
         ALTER TABLE snapshots_houses RENAME TO snapshots;
         DROP TABLE IF EXISTS scout_api_history;",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Deletes the prices of the Auctionator and Forever Guide imports (once: remembered in `meta`).
pub(crate) async fn drop_addon_prices(pool: &SqlitePool) -> Result<()> {
    const KEY: &str = "addon_prices_removed";
    let done: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    if done.is_some() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM snapshots WHERE source IN ('auctionator', 'foreverguide')")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES (?, '1')")
        .bind(KEY)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use scout_core::scout_api::AuctionHouse;

    use crate::Db;

    #[tokio::test]
    async fn an_old_database_keeps_scout_snapshots_and_drops_the_addons_ones() {
        let dir = std::env::temp_dir().join(format!("scout-rs-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        let _ = std::fs::remove_file(&path);
        {
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true);
            let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
            sqlx::raw_sql(
                "CREATE TABLE items (item_id INTEGER PRIMARY KEY, name TEXT);
                 CREATE TABLE snapshots (
                     item_id INTEGER NOT NULL REFERENCES items(item_id), source TEXT NOT NULL,
                     ts INTEGER NOT NULL, price INTEGER, quantity INTEGER,
                     PRIMARY KEY (item_id, source, ts)) WITHOUT ROWID;
                 INSERT INTO items VALUES (2589, 'Étoffe de lin');
                 INSERT INTO snapshots VALUES (2589, 'scout', 100, 27, 40), (2589, 'scout-api', 90, 30, NULL), (2589, 'auctionator', 80, 31, NULL), (2589, 'foreverguide', 70, 32, NULL);",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }
        let db = Db::open(&path).await.unwrap();
        let none = AuctionHouse::default();
        assert_eq!(
            db.history(2589, &none).await.unwrap().len(),
            2,
            "scout and scout-api only"
        );
        let eu = AuctionHouse {
            region: "eu".into(),
            id: 114,
        };
        assert_eq!(db.claim_unassigned(&eu).await.unwrap(), 2);
        assert_eq!(db.items(&eu).await.unwrap()[0].price, Some(27));
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
