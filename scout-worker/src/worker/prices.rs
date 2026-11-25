//! Our price server (source `scout-api`): the latest prices of every item once they are an hour
//! old, and one item's whole history when its market sheet opens (cached, refreshed hourly too).

use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use scout_core::scout_api::{self, REFRESH_SECS};
use scout_db::HistoryCache;
use scout_sources::price_server;
use tokio::runtime::Runtime;

use super::{Shared, refresh};
use crate::message::{Event, Task};

/// How often the latest prices' age is checked (they are downloaded once an hour old).
const CHECK_EVERY: Duration = Duration::from_secs(600);

/// Checks the latest prices now, then every `CHECK_EVERY`, for as long as the tool runs.
pub(super) fn start(shared: &Shared) {
    let shared = shared.clone();
    let _ = thread::Builder::new()
        .name("prices-timer".into())
        .spawn(move || {
            loop {
                shared.spawn(Task::Prices, |runtime, shared| {
                    sync_latest(runtime, shared, false)
                });
                thread::sleep(CHECK_EVERY);
            }
        });
}

/// New server settings: saved, then its houses and (with a house) the prices follow.
pub(super) fn set_server(
    shared: &Shared,
    url: String,
    key: Option<String>,
    house: Option<u32>,
) -> Result<()> {
    {
        let mut config = shared.config();
        let server = &mut config.price_server;
        let url = url.trim();
        server.url = if url.is_empty() {
            scout_api::DEFAULT_SERVER.to_owned()
        } else {
            url.trim_end_matches('/').to_owned()
        };
        server.key = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
        if server.house != house {
            // Another house: its prices are downloaded at once.
            server.house = house;
            server.last_sync = None;
        }
        (shared.send)(Event::Config(config.clone()));
        config.save()?;
    }
    load_houses(shared);
    shared.spawn(Task::Prices, |runtime, shared| {
        sync_latest(runtime, shared, false)
    });
    Ok(())
}

/// The server's houses, on their own thread (network).
pub(super) fn load_houses(shared: &Shared) {
    let server = shared.config().price_server.clone();
    let Some(key) = server.key.clone() else {
        return;
    };
    let send = shared.send.clone();
    let _ = thread::Builder::new()
        .name("server-houses".into())
        .spawn(move || {
            let houses = price_server::houses(&server.url, &key).map_err(|e| format!("{e:#}"));
            send(Event::ServerHouses(houses));
        });
}

/// `item_history` on its own thread: the sheet already shows what the database has.
pub(super) fn history_in_background(shared: &Shared, item_id: i64) {
    let shared = shared.clone();
    let _ = thread::Builder::new()
        .name("item-history".into())
        .spawn(move || {
            if let Err(e) = item_history(&shared.runtime, &shared, item_id) {
                (shared.send)(Event::Error(format!("historique Scout API : {e:#}")));
            }
        });
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Downloads the latest prices when they are an hour old (or `force`), keeping what changed.
pub(super) fn sync_latest(runtime: &Runtime, shared: &Shared, force: bool) -> Result<()> {
    let server = shared.config().price_server.clone();
    let Some((url, key, house)) = server.ready() else {
        if force {
            bail!("serveur de prix : il manque la clé d'accès ou l'hôtel des ventes (Réglages)");
        }
        return Ok(());
    };
    let started = now();
    if !force && server.last_sync.is_some_and(|t| started - t < REFRESH_SECS) {
        return Ok(());
    }
    let latest = price_server::latest(url, key, house)?;
    let (db, send) = (&shared.db, &*shared.send);
    let added = runtime.block_on(async {
        let last = db.last_api_prices().await?;
        db.insert(&scout_api::latest_rows(&last, &latest)).await
    })?;
    {
        let mut config = shared.config();
        config.price_server.last_sync = Some(started);
        send(Event::Config(config.clone()));
        config.save()?;
    }
    if added > 0 {
        runtime.block_on(refresh(db, send))?;
    }
    send(Event::Status(format!(
        "Scout API : {} prix lus, {added} changements enregistrés",
        latest.len()
    )));
    Ok(())
}

/// Completes one item's history from the server, unless it was asked less than an hour ago,
/// then sends it again.
fn item_history(runtime: &Runtime, shared: &Shared, item_id: i64) -> Result<()> {
    let server = shared.config().price_server.clone();
    let Some((url, key, house)) = server.ready() else {
        return Ok(());
    };
    let db = &shared.db;
    let started = now();
    let since = match runtime.block_on(db.api_history(item_id))? {
        Some(cache) if cache.house == i64::from(house) => {
            if started - cache.checked_at < REFRESH_SECS {
                return Ok(());
            }
            cache.until
        }
        // Never downloaded, or for another house: all of it.
        _ => 0,
    };
    let lines = price_server::history(url, key, house, item_id, since)?;
    let until = lines.iter().map(|l| l.ts).max().unwrap_or(since);
    let history = runtime.block_on(async {
        db.insert(&scout_api::history_rows(&lines)).await?;
        let cache = HistoryCache {
            house: i64::from(house),
            until,
            checked_at: started,
        };
        db.set_api_history(item_id, cache).await?;
        db.history(item_id).await
    })?;
    if !lines.is_empty() {
        (shared.send)(Event::History(item_id, history));
    }
    Ok(())
}
