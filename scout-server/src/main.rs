//! scout-server: polls WoW: Forever's auction houses on Battle.net and keeps every snapshot
//! (docs/serveur.md): the collector, the prices of every new snapshot, and the API that serves them.
//!
//! Settings (environment): `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET` (required),
//! `SCOUT_KEYS` (access keys, `name=key,…`; none = `/v1` refuses everyone), `SCOUT_REGIONS` (eu,us,kr,tw:
//! each region has its own database and raw folder, `data/` for eu, `data/<region>/` for the others),
//! `SCOUT_DATA_DIR` (data), `SCOUT_LISTEN` (0.0.0.0:3000), `SCOUT_INTERVAL_SECS` (600),
//! `SCOUT_ZSTD_LEVEL` (15), `SCOUT_GAMEDATA_SECS` (21600: how often to look for a new Forever build),
//! `SCOUT_STAGGER_SECS` (5: delay between the first fetches of two regions).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::Semaphore;
mod aggregate;
mod api;
mod blizzard;
mod collector;
mod db;
mod gamedata;
#[cfg(test)]
mod peak;
mod storage;

use api::{Api, Keys, Limiter, Region};
use blizzard::{Client, Credentials};
use collector::Collector;
use db::Db;
use gamedata::GameStore;
use storage::Storage;

/// Requests per hour for one key (and at once): a first sync pages through the whole history.
const KEY_RATE: u32 = 600;
/// Requests per hour from one address without a valid key.
const GUESS_RATE: u32 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = PathBuf::from(env_or("SCOUT_DATA_DIR", "data"));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("dossier {}", data_dir.display()))?;
    let level: i32 = env_or("SCOUT_ZSTD_LEVEL", "15").parse()?;
    let client_id = std::env::var("BLIZZARD_CLIENT_ID").context("BLIZZARD_CLIENT_ID manquant")?;
    let client_secret =
        std::env::var("BLIZZARD_CLIENT_SECRET").context("BLIZZARD_CLIENT_SECRET manquant")?;
    let interval = Duration::from_secs(env_or("SCOUT_INTERVAL_SECS", "600").parse()?);
    let stagger = Duration::from_secs(env_or("SCOUT_STAGGER_SECS", "5").parse()?);
    // One heavy job (download, aggregation, compression) at a time over all regions: four at once
    // went over the memory limit (OOM).
    let heavy = Arc::new(Semaphore::new(1));
    let mut regions = BTreeMap::new();
    let mut index = 0u32;
    for region in env_or("SCOUT_REGIONS", "eu,us,kr,tw")
        .split(',')
        .map(str::trim)
    {
        if region.is_empty() {
            continue;
        }
        // eu was the only region at first: its files stay at the top of the data folder.
        let dir = if region == "eu" {
            data_dir.clone()
        } else {
            data_dir.join(region)
        };
        std::fs::create_dir_all(&dir).with_context(|| format!("dossier {}", dir.display()))?;
        let db = Db::open(&dir.join("scout-server.db")).await?;
        let storage = Arc::new(Storage::new(&dir, level));
        let client = Arc::new(Client::new(Credentials {
            region: region.to_owned(),
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
        }));
        tokio::spawn(
            Collector {
                region: region.to_owned(),
                client,
                db: db.clone(),
                storage: storage.clone(),
                interval,
                heavy: heavy.clone(),
                delay: stagger * index,
            }
            .run(),
        );
        index += 1;
        regions.insert(region.to_owned(), Region { db, storage });
    }

    let gamedata = Arc::new(GameStore::new(&data_dir)?);
    let gamedata_every = Duration::from_secs(env_or("SCOUT_GAMEDATA_SECS", "21600").parse()?);
    tokio::spawn(gamedata.clone().run(gamedata_every));

    let keys = Keys::parse(&env_or("SCOUT_KEYS", ""));
    if keys.len() == 0 {
        eprintln!("aucune clé d'accès (SCOUT_KEYS) : /v1 refuse toutes les requêtes");
    }
    let app = api::router(Arc::new(Api {
        regions,
        gamedata,
        keys,
        by_key: Limiter::new(KEY_RATE, KEY_RATE),
        by_ip: Limiter::new(GUESS_RATE, GUESS_RATE),
    }));
    let listen = env_or("SCOUT_LISTEN", "0.0.0.0:3000");
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .with_context(|| format!("écoute sur {listen}"))?;
    eprintln!(
        "scout-server {} sur {listen}, relevé toutes les {} s",
        env!("CARGO_PKG_VERSION"),
        interval.as_secs()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// Ctrl-C or SIGTERM (`docker stop`). A raw file is renamed into place only once complete,
/// so stopping mid-fetch loses nothing but that fetch.
async fn shutdown() {
    let mut term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}
