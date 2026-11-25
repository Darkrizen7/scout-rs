//! The server's SQLite database: the index of the raw snapshots and the fetch log
//! (the aggregates served to the app come in step 2, docs/serveur.md).

use std::path::Path;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS houses (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    first_seen INTEGER NOT NULL,  -- Unix seconds
    last_seen  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS raw_snapshots (
    house         INTEGER NOT NULL,
    kind          TEXT    NOT NULL,  -- auctions, commodities
    fetched_at    INTEGER NOT NULL,  -- first fetch with this content, Unix seconds
    last_seen     INTEGER NOT NULL,  -- last fetch that still returned it
    last_modified TEXT,              -- Last-Modified header, as sent by Battle.net
    sha256        BLOB    NOT NULL,  -- of the uncompressed body
    path          TEXT,              -- relative to the data folder; NULL when not written (disk full)
    raw_bytes     INTEGER NOT NULL,
    bytes         INTEGER NOT NULL,  -- compressed size, 0 when not written
    auctions      INTEGER NOT NULL,
    PRIMARY KEY (house, kind, fetched_at)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS fetch_log (
    ts     INTEGER NOT NULL,
    house  INTEGER NOT NULL,
    kind   TEXT    NOT NULL,
    status TEXT    NOT NULL,  -- new, same, not_modified, skipped, error
    ms     INTEGER NOT NULL,
    error  TEXT
);
CREATE INDEX IF NOT EXISTS fetch_log_ts ON fetch_log (ts);
";

/// Cheap to clone (a connection pool).
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

/// The newest snapshot of one listing.
#[derive(Debug, sqlx::FromRow)]
pub struct Last {
    pub fetched_at: i64,
    pub last_modified: Option<String>,
    pub sha256: Vec<u8>,
}

pub struct NewSnapshot<'a> {
    pub house: u32,
    pub kind: &'a str,
    pub fetched_at: i64,
    pub last_modified: Option<&'a str>,
    pub sha256: &'a [u8],
    pub path: Option<&'a str>,
    pub raw_bytes: u64,
    pub bytes: u64,
    pub auctions: u64,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        Self::with(options).await
    }

    #[cfg(test)]
    pub async fn memory() -> Result<Self> {
        Self::with(SqliteConnectOptions::new().in_memory(true)).await
    }

    async fn with(options: SqliteConnectOptions) -> Result<Self> {
        // One writer (the collector) and a few reads: one connection is plenty.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn upsert_house(&self, id: u32, name: &str, now: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO houses (id, name, first_seen, last_seen) VALUES (?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, last_seen = excluded.last_seen",
        )
        .bind(id)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn last_snapshot(&self, house: u32, kind: &str) -> Result<Option<Last>> {
        Ok(sqlx::query_as(
            "SELECT fetched_at, last_modified, sha256 FROM raw_snapshots
             WHERE house = ? AND kind = ? ORDER BY fetched_at DESC LIMIT 1",
        )
        .bind(house)
        .bind(kind)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// The content of `fetched_at` was returned again at `now`.
    pub async fn touch_snapshot(
        &self,
        house: u32,
        kind: &str,
        fetched_at: i64,
        now: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE raw_snapshots SET last_seen = ? WHERE house = ? AND kind = ? AND fetched_at = ?",
        )
        .bind(now)
        .bind(house)
        .bind(kind)
        .bind(fetched_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_snapshot(&self, s: &NewSnapshot<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO raw_snapshots (house, kind, fetched_at, last_seen, last_modified, sha256,
                                        path, raw_bytes, bytes, auctions)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.house)
        .bind(s.kind)
        .bind(s.fetched_at)
        .bind(s.fetched_at)
        .bind(s.last_modified)
        .bind(s.sha256)
        .bind(s.path)
        .bind(s.raw_bytes as i64)
        .bind(s.bytes as i64)
        .bind(s.auctions as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn log_fetch(
        &self,
        (ts, house, kind): (i64, u32, &str),
        status: &str,
        ms: u64,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO fetch_log (ts, house, kind, status, ms, error) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(ts)
        .bind(house)
        .bind(kind)
        .bind(status)
        .bind(ms as i64)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn purge_log(&self, before: i64) -> Result<()> {
        sqlx::query("DELETE FROM fetch_log WHERE ts < ?")
            .bind(before)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// When a listing last answered (new or unchanged content), for `/health`.
    pub async fn last_success(&self) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT MAX(last_seen) FROM raw_snapshots")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    #[cfg(test)]
    pub async fn count(&self, table_query: &'static str) -> i64 {
        sqlx::query_scalar(table_query)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}
