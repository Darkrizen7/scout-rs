//! SQLite storage. Every scan ever imported is kept, even after Scout forgets it in game.

use std::path::Path;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

mod game;
mod vendor;

pub use game::GAME_FORMAT;
pub use vendor::{VendorData, VendorOffer};

use crate::analysis::Observation;
use crate::import::{ItemName, Row};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY,
    name    TEXT
);
CREATE TABLE IF NOT EXISTS favorites (
    item_id INTEGER PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS snapshots (
    item_id  INTEGER NOT NULL REFERENCES items(item_id),
    source   TEXT    NOT NULL,  -- scout, auctionator, foreverguide
    ts       INTEGER NOT NULL,  -- scan time, Unix seconds
    price    INTEGER,           -- unit buyout in copper, NULL when not listed
    quantity INTEGER,           -- units listed, NULL when the source did not record it
    PRIMARY KEY (item_id, source, ts)
) WITHOUT ROWID;
";

/// Columns added after the first release: (name, statement adding it when missing).
const ITEM_COLUMNS: [(&str, &str); 4] = [
    ("class_id", "ALTER TABLE items ADD COLUMN class_id INTEGER"),
    (
        "subclass_id",
        "ALTER TABLE items ADD COLUMN subclass_id INTEGER",
    ),
    ("class_name", "ALTER TABLE items ADD COLUMN class_name TEXT"),
    (
        "subclass_name",
        "ALTER TABLE items ADD COLUMN subclass_name TEXT",
    ),
];

/// An item with its latest scan, for the list view.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ItemSummary {
    pub item_id: i64,
    pub name: Option<String>,
    pub class_id: Option<i64>,
    pub subclass_id: Option<i64>,
    pub class_name: Option<String>,
    pub subclass_name: Option<String>,
    pub price: Option<i64>,
    pub quantity: Option<i64>,
    pub ts: i64,
    pub scans: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Snapshot {
    pub source: String,
    pub ts: i64,
    pub price: Option<i64>,
    pub quantity: Option<i64>,
}

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        sqlx::raw_sql(game::SCHEMA).execute(&pool).await?;
        Self::migrate_game(&pool).await?;
        Self::migrate_vendor(&pool).await?;
        let existing: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('items')")
                .fetch_all(&pool)
                .await?;
        for (column, add) in ITEM_COLUMNS {
            if !existing.iter().any(|(name,)| name == column) {
                sqlx::raw_sql(add).execute(&pool).await?;
            }
        }
        Ok(Self { pool })
    }

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
    use super::*;

    fn row(
        item_id: i64,
        name: Option<&str>,
        timestamp: i64,
        price: Option<i64>,
        quantity: i64,
    ) -> Row {
        Row {
            source: crate::import::SCOUT.to_owned(),
            item_id,
            name: name.map(str::to_owned),
            timestamp,
            price,
            quantity: Some(quantity),
        }
    }

    #[tokio::test]
    async fn keeps_names_and_ignores_duplicate_scans() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("scout-prix-test-{}", std::process::id()));
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

/// Imports the local WoW install into a throwaway database:
/// `cargo test -- --ignored --nocapture`.
#[cfg(test)]
mod local_install {
    use super::*;
    use crate::{config::Config, import};

    #[tokio::test]
    #[ignore = "reads the local WoW install"]
    async fn imports_local_saved_variables() -> Result<()> {
        let config = Config::load();
        let saved_variables = config.saved_variables()?;
        let scout = import::run_scout_export(&config.export_script()?, &saved_variables)?;
        let legacy = import::run_legacy_export(saved_variables.parent().unwrap())?;

        let path = std::env::temp_dir().join(format!("scout-prix-local-{}.db", std::process::id()));
        let db = Db::open(&path).await?;
        let added = db.insert(&scout).await? + db.insert(&legacy).await?;
        assert_eq!(db.insert(&legacy).await?, 0, "a second import adds nothing");
        let items = db.items().await?;
        println!(
            "{} scout + {} legacy rows, {added} stored, {} items",
            scout.len(),
            legacy.len(),
            items.len()
        );
        for id in [2589, 8191] {
            for s in db.history(id).await? {
                println!("{id} {} {} {:?} {:?}", s.source, s.ts, s.price, s.quantity);
            }
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }
}
