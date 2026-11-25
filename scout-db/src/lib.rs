//! SQLite storage. Every scan ever imported is kept, even after Scout forgets it in game.

use std::path::Path;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

mod drops;
mod game;
mod market;
mod reference;
mod vendor;

pub use market::scout_api::HistoryCache;
pub use reference::ReferenceUpdate;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY,
    name    TEXT
);
CREATE TABLE IF NOT EXISTS favorites (
    item_id INTEGER PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,  -- reference: id of the reference database last applied
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS zone_favorites (
    zone TEXT PRIMARY KEY  -- zone name (Loot module)
);
CREATE TABLE IF NOT EXISTS snapshots (
    item_id  INTEGER NOT NULL REFERENCES items(item_id),
    source   TEXT    NOT NULL,  -- scout, scout-api, auctionator, foreverguide
    region   TEXT    NOT NULL DEFAULT '',  -- auction house: Battle.net region (eu, us, kr, tw)…
    house    INTEGER NOT NULL DEFAULT 0,   -- …and house id (114…); '' / 0 = not known yet
    ts       INTEGER NOT NULL,  -- scan time, Unix seconds
    price    INTEGER,           -- unit buyout in copper, NULL when not listed
    quantity INTEGER,           -- units listed, NULL when the source did not record it
    PRIMARY KEY (item_id, region, house, source, ts)
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

/// Cheap to clone (a connection pool): shared by the worker's threads.
#[derive(Clone)]
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
            // One connection shared by every thread: a long import may hold it for a while.
            .acquire_timeout(std::time::Duration::from_secs(600))
            .connect_with(options)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        sqlx::raw_sql(game::SCHEMA).execute(&pool).await?;
        sqlx::raw_sql(drops::SCHEMA).execute(&pool).await?;
        sqlx::raw_sql(game::maps::SCHEMA).execute(&pool).await?;
        sqlx::raw_sql(game::wowhead::SCHEMA).execute(&pool).await?;
        market::migrate::add_houses(&pool).await?;
        sqlx::raw_sql(market::scout_api::SCHEMA)
            .execute(&pool)
            .await?;
        Self::migrate_game(&pool).await?;
        Self::migrate_vendor(&pool).await?;
        Self::migrate_drops(&pool).await?;
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
}
