//! Polls every listing of every house and keeps each new content once (raw file + index row).

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::blizzard::{Client, Fetched, Kind};
use crate::db::{Db, Last, NewSnapshot};
use crate::storage::{self, Storage};

/// Below this, raw snapshots are no longer written (their index row still is).
pub const STOP_RAW_BYTES: u64 = 3_000_000_000;
/// The fetch log is only for watching the collector: a week is enough.
const LOG_DAYS: i64 = 7;

pub struct Collector {
    pub client: Arc<Client>,
    pub db: Db,
    pub storage: Arc<Storage>,
    pub interval: Duration,
}

/// What one fetch of a listing gave.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// New content, written to disk.
    New,
    /// Same content as the newest snapshot.
    Same,
    /// 304 to our `If-Modified-Since`.
    NotModified,
    /// New content, not written: the disk is nearly full.
    Skipped,
}

impl Outcome {
    fn as_str(&self) -> &'static str {
        match self {
            Outcome::New => "new",
            Outcome::Same => "same",
            Outcome::NotModified => "not_modified",
            Outcome::Skipped => "skipped",
        }
    }
}

impl Collector {
    pub async fn run(self) {
        let mut ticks = tokio::time::interval(self.interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            if let Err(e) = self.cycle().await {
                eprintln!("relevé interrompu : {e:#}");
            }
        }
    }

    async fn cycle(&self) -> Result<()> {
        let start = Instant::now();
        let client = self.client.clone();
        let houses = tokio::task::spawn_blocking(move || client.houses()).await??;
        let now = unix_now();
        for house in &houses {
            self.db.upsert_house(house.id, &house.name, now).await?;
        }
        let mut new = 0;
        for house in &houses {
            for kind in Kind::ALL {
                new += u32::from(self.listing(house.id, kind).await == Some(Outcome::New));
            }
        }
        self.db.purge_log(now - LOG_DAYS * 86_400).await?;
        eprintln!(
            "relevé : {} maisons, {new} nouveaux instantanés, {:.1} s",
            houses.len(),
            start.elapsed().as_secs_f32()
        );
        Ok(())
    }

    /// Fetches and stores one listing, logging the result; `None` on error.
    async fn listing(&self, house: u32, kind: Kind) -> Option<Outcome> {
        let start = Instant::now();
        let now = unix_now();
        let result = self.fetch_and_store(house, kind, now).await;
        let ms = start.elapsed().as_millis() as u64;
        let (status, error) = match &result {
            Ok(outcome) => (outcome.as_str(), None),
            Err(e) => ("error", Some(format!("{e:#}"))),
        };
        if let Some(e) = &error {
            eprintln!("{house} {} : {e}", kind.as_str());
        }
        if let Err(e) = self
            .db
            .log_fetch((now, house, kind.as_str()), status, ms, error.as_deref())
            .await
        {
            eprintln!("journal des relevés : {e:#}");
        }
        result.ok()
    }

    async fn fetch_and_store(&self, house: u32, kind: Kind, now: i64) -> Result<Outcome> {
        let last = self.db.last_snapshot(house, kind.as_str()).await?;
        let since = last.as_ref().and_then(|l| l.last_modified.clone());
        let client = self.client.clone();
        let fetched =
            tokio::task::spawn_blocking(move || client.listing(house, kind, since.as_deref()))
                .await??;
        match fetched {
            Fetched::NotModified => {
                if let Some(last) = &last {
                    self.db
                        .touch_snapshot(house, kind.as_str(), last.fetched_at, now)
                        .await?;
                }
                Ok(Outcome::NotModified)
            }
            Fetched::Body {
                bytes,
                last_modified,
            } => {
                let at = (house, kind, now);
                store(
                    &self.db,
                    &self.storage,
                    at,
                    last.as_ref(),
                    bytes,
                    last_modified.as_deref(),
                )
                .await
            }
        }
    }
}

/// Keeps `bytes` unless it is the content of `last`, the listing's newest snapshot, again.
pub async fn store(
    db: &Db,
    storage: &Arc<Storage>,
    (house, kind, now): (u32, Kind, i64),
    last: Option<&Last>,
    bytes: Vec<u8>,
    last_modified: Option<&str>,
) -> Result<Outcome> {
    let sha256 = Sha256::digest(&bytes).to_vec();
    if let Some(last) = last.filter(|l| l.sha256 == sha256) {
        db.touch_snapshot(house, kind.as_str(), last.fetched_at, now)
            .await?;
        return Ok(Outcome::Same);
    }
    let relative = storage::relative_path(house, kind, now);
    let storage = storage.clone();
    let (raw_bytes, auctions, written) = tokio::task::spawn_blocking(move || -> Result<_> {
        let auctions = crate::blizzard::count_auctions(&bytes)?;
        let written = if storage.free_bytes()? < STOP_RAW_BYTES {
            None
        } else {
            Some((storage.write(&relative, &bytes)?, relative))
        };
        Ok((bytes.len() as u64, auctions, written))
    })
    .await
    .context("compression")??;
    db.insert_snapshot(&NewSnapshot {
        house,
        kind: kind.as_str(),
        fetched_at: now,
        last_modified,
        sha256: &sha256,
        path: written.as_ref().map(|(_, p)| p.as_str()),
        raw_bytes,
        bytes: written.as_ref().map_or(0, |(size, _)| *size),
        auctions,
    })
    .await?;
    Ok(if written.is_some() {
        Outcome::New
    } else {
        Outcome::Skipped
    })
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_each_content_once() {
        let dir =
            std::env::temp_dir().join(format!("scout-server-collector-{}", std::process::id()));
        let db = Db::memory().await.unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Arc::new(Storage::new(&dir, 3));
        let body = |n: u32| {
            format!(
                r#"{{"auctions":[{{"id":{n},"item":{{"id":768}},"buyout":147,"quantity":1}}]}}"#
            )
        };
        let t = 1_795_598_107;

        let put = async |dt, n, last_modified| {
            let last = db.last_snapshot(114, "auctions").await.unwrap();
            let at = (114, Kind::Auctions, t + dt);
            let bytes = body(n).into_bytes();
            store(&db, &storage, at, last.as_ref(), bytes, last_modified)
                .await
                .unwrap()
        };
        assert_eq!(
            put(0, 1, Some("Wed, 25 Nov 2026 09:00:00 GMT")).await,
            Outcome::New
        );
        assert_eq!(put(600, 1, None).await, Outcome::Same);
        assert_eq!(put(1200, 2, None).await, Outcome::New);

        assert_eq!(db.count("SELECT COUNT(*) FROM raw_snapshots").await, 2);
        let first = db
            .count("SELECT last_seen FROM raw_snapshots ORDER BY fetched_at LIMIT 1")
            .await;
        assert_eq!(first, t + 600);
        let last = db.last_snapshot(114, "auctions").await.unwrap().unwrap();
        assert_eq!(last.fetched_at, t + 1200);
        assert!(
            dir.join(storage::relative_path(114, Kind::Auctions, t + 1200))
                .exists()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
