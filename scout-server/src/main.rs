//! scout-server: polls WoW: Forever's auction houses on Battle.net and keeps every snapshot
//! (docs/serveur.md): the collector, the prices of every new snapshot, and the API that serves them.
//!
//! Settings (environment): `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET` (required),
//! `SCOUT_KEYS` (access keys, `name=key,…`; none = `/v1` refuses everyone), `BLIZZARD_REGION` (eu),
//! `SCOUT_DATA_DIR` (data), `SCOUT_LISTEN` (0.0.0.0:3000), `SCOUT_INTERVAL_SECS` (600),
//! `SCOUT_ZSTD_LEVEL` (15).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
mod aggregate;
mod api;
mod blizzard;
mod collector;
mod db;
mod storage;

use api::{Api, Keys, Limiter};
use blizzard::{Client, Credentials};
use collector::Collector;
use db::Db;
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
    let db = Db::open(&data_dir.join("scout-server.db")).await?;
    let storage = Arc::new(Storage::new(
        &data_dir,
        env_or("SCOUT_ZSTD_LEVEL", "15").parse()?,
    ));
    let client = Arc::new(Client::new(Credentials {
        region: env_or("BLIZZARD_REGION", "eu"),
        client_id: std::env::var("BLIZZARD_CLIENT_ID").context("BLIZZARD_CLIENT_ID manquant")?,
        client_secret: std::env::var("BLIZZARD_CLIENT_SECRET")
            .context("BLIZZARD_CLIENT_SECRET manquant")?,
    }));
    let interval = Duration::from_secs(env_or("SCOUT_INTERVAL_SECS", "600").parse()?);

    tokio::spawn(
        Collector {
            client,
            db: db.clone(),
            storage: storage.clone(),
            interval,
        }
        .run(),
    );

    let keys = Keys::parse(&env_or("SCOUT_KEYS", ""));
    if keys.len() == 0 {
        eprintln!("aucune clé d'accès (SCOUT_KEYS) : /v1 refuse toutes les requêtes");
    }
    let app = api::router(Arc::new(Api {
        db,
        storage,
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
