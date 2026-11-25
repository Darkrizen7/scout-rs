//! scout-server: polls WoW: Forever's auction houses on Battle.net and keeps every snapshot
//! (docs/serveur.md). Step 1: the collector and `/health`; the aggregates and their API come next.
//!
//! Settings (environment): `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET` (required),
//! `BLIZZARD_REGION` (eu), `SCOUT_DATA_DIR` (data), `SCOUT_LISTEN` (0.0.0.0:3000),
//! `SCOUT_INTERVAL_SECS` (600), `SCOUT_ZSTD_LEVEL` (15).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

mod blizzard;
mod collector;
mod db;
mod storage;

use blizzard::{Client, Credentials};
use collector::Collector;
use db::Db;
use storage::Storage;

/// `/health` warns below this much free space.
const WARN_FREE_BYTES: u64 = 10_000_000_000;

#[derive(Clone)]
struct AppState {
    db: Db,
    storage: Arc<Storage>,
}

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

    let app = Router::new()
        .route("/health", get(health))
        .with_state(AppState { db, storage });
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

/// Answers as long as the process runs (the homelab checks 2xx); the body says how the collector is doing.
async fn health(State(state): State<AppState>) -> Json<Value> {
    let last = state.db.last_success().await.ok().flatten();
    let free = state.storage.free_bytes().ok();
    Json(json!({
        "last_success": last,
        "seconds_since": last.map(|t| collector::unix_now() - t),
        "free_gb": free.map(|b| b / 1_000_000_000),
        "low_disk": free.is_some_and(|b| b < WARN_FREE_BYTES),
        "raw_stopped": free.is_some_and(|b| b < collector::STOP_RAW_BYTES),
    }))
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
