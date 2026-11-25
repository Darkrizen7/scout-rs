//! Our price server (source `scout-api`): the latest prices of every item once they are an hour
//! old, and one item's whole history when its market sheet opens (cached, refreshed hourly too).

use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use scout_core::scout_api::{self, AuctionHouse, REFRESH_SECS, ServerChoice};
use scout_db::HistoryCache;
use scout_sources::price_server;
use tokio::runtime::Runtime;

use super::{Shared, refresh};
use crate::message::{Event, Task};

/// How often the latest prices' age is checked (they are downloaded once an hour old).
const CHECK_EVERY: Duration = Duration::from_secs(600);

/// Checks the latest prices now, then every `CHECK_EVERY`, for as long as the tool runs.
pub(super) fn start(shared: &Shared) {
    load_houses(shared);
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
    (region, house): (String, Option<u32>),
) -> Result<()> {
    let chosen = {
        let mut config = shared.config();
        let server = &mut config.price_server;
        let url = url.trim();
        server.url = if url.is_empty() {
            scout_api::DEFAULT_SERVER.to_owned()
        } else {
            url.trim_end_matches('/').to_owned()
        };
        // Another region has other houses: one must be chosen again.
        let house = if region == server.region { house } else { None };
        let changed = server.house != house || server.region != region;
        if changed {
            // Another house: its prices are downloaded at once.
            server.region = region;
            server.house = house;
            server.last_sync = None;
        }
        (shared.send)(Event::Config(config.clone()));
        config.save()?;
        changed.then(|| config.price_server.auction_house())
    };
    if let Some(house) = chosen {
        show_house(&shared.runtime, shared, &house)?;
    }
    load_houses(shared);
    shared.spawn(Task::Prices, |runtime, shared| {
        sync_latest(runtime, shared, false)
    });
    Ok(())
}

/// The pages show another house: the snapshots recorded before any house was chosen become its
/// own (the first house chosen is where they were scanned), then its items.
pub(super) fn show_house(runtime: &Runtime, shared: &Shared, house: &AuctionHouse) -> Result<()> {
    if house.id == 0 {
        return Ok(());
    }
    let claimed = runtime.block_on(shared.db.claim_unassigned(house))?;
    if claimed > 0 {
        (shared.send)(Event::Status(format!(
            "{claimed} relevés enregistrés avant le choix d'un hôtel des ventes rattachés à {} n° {}",
            house.region.to_uppercase(),
            house.id
        )));
    }
    runtime.block_on(refresh(&shared.db, house, &*shared.send))
}

/// The server's regions and the houses of the chosen one, on their own thread (network).
pub(super) fn load_houses(shared: &Shared) {
    let server = shared.config().price_server.clone();
    let send = shared.send.clone();
    let _ = thread::Builder::new()
        .name("server-houses".into())
        .spawn(move || {
            let choice = (|| {
                Ok::<_, anyhow::Error>(ServerChoice {
                    regions: price_server::regions(&server.url)?,
                    houses: price_server::houses(&server.url, &server.region)?,
                    region: server.region.clone(),
                })
            })();
            send(Event::ServerHouses(choice.map_err(|e| format!("{e:#}"))));
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
    let Some((url, house)) = server.ready() else {
        if force {
            bail!("serveur de prix : il manque l'hôtel des ventes (Réglages)");
        }
        return Ok(());
    };
    let started = now();
    if !force && server.last_sync.is_some_and(|t| started - t < REFRESH_SECS) {
        return Ok(());
    }
    let latest = price_server::latest(url, &house)?;
    let (db, send) = (&shared.db, &*shared.send);
    let added = runtime.block_on(async {
        let last = db.last_api_prices(&house).await?;
        db.insert(&scout_api::latest_rows(&house, &last, &latest))
            .await
    })?;
    {
        let mut config = shared.config();
        config.price_server.last_sync = Some(started);
        send(Event::Config(config.clone()));
        config.save()?;
    }
    if added > 0 {
        runtime.block_on(refresh(db, &house, send))?;
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
    let Some((url, house)) = server.ready() else {
        return Ok(());
    };
    let db = &shared.db;
    let started = now();
    let since = match runtime.block_on(db.api_history(item_id, &house))? {
        Some(cache) if started - cache.checked_at < REFRESH_SECS => return Ok(()),
        Some(cache) => cache.until,
        // Never downloaded at this house: all of it.
        None => 0,
    };
    let lines = price_server::history(url, &house, item_id, since)?;
    let until = lines.iter().map(|l| l.ts).max().unwrap_or(since);
    let history = runtime.block_on(async {
        db.insert(&scout_api::history_rows(&house, &lines)).await?;
        let cache = HistoryCache {
            until,
            checked_at: started,
        };
        db.set_api_history(item_id, &house, cache).await?;
        db.history(item_id, &house).await
    })?;
    // Still the house shown (it may have been changed meanwhile)?
    if !lines.is_empty() && shared.config().price_server.auction_house() == house {
        (shared.send)(Event::History(item_id, history));
    }
    Ok(())
}
