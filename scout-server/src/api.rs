//! The HTTP API: `/health`, `/privacy` and `/thanks` for everyone, `/v1/…` open to all, within a rate per address.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

use crate::collector::{self, STOP_RAW_BYTES};
use crate::db::{Db, PriceRow};
use crate::gamedata::GameStore;
use crate::storage::Storage;

mod access;
mod reference;

pub use access::Limiter;

/// `/health` warns below this much free space.
const WARN_FREE_BYTES: u64 = 10_000_000_000;
/// Rows per `/prices` page: about a day of a busy house, ~8 MB of CSV before gzip.
const PAGE_ROWS: i64 = 200_000;

/// One Battle.net region: its own database and raw folder.
pub struct Region {
    pub db: Db,
    pub storage: Arc<Storage>,
}

pub struct Api {
    /// eu, us, kr, tw (`SCOUT_REGIONS`).
    pub regions: BTreeMap<String, Region>,
    /// Game data package, the same for every region.
    pub gamedata: Arc<GameStore>,
    /// Per address: short bursts (the minute window).
    pub by_minute: Limiter,
    /// Per address: the sustained rate (the hour window).
    pub by_hour: Limiter,
    /// Addresses never limited (`SCOUT_TRUSTED_IPS`: the administrator's own, for tests).
    pub trusted: Vec<String>,
}

pub fn router(api: Arc<Api>) -> Router {
    let v1 = Router::new()
        .route("/regions", get(regions))
        .route("/gamedata/latest", get(gamedata_latest))
        .route("/gamedata/{format}/{updated}", get(gamedata_package))
        .route("/reference/latest", get(reference::latest))
        .route("/reference/{format}/{updated}", get(reference::package))
        .route("/{region}/houses", get(houses))
        .route("/{region}/houses/{id}/prices", get(prices))
        .route("/{region}/houses/{id}/latest", get(latest))
        .route(
            "/{region}/houses/{id}/items/{item}/history",
            get(item_history),
        )
        .layer(middleware::from_fn_with_state(api.clone(), guard));
    Router::new()
        .route("/health", get(health))
        .route("/privacy", get(privacy))
        .route("/thanks", get(thanks))
        .nest("/v1", v1)
        // The game data package is gzipped already.
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("application/gzip")),
        ))
        .with_state(api)
}

/// Lets a request through within the rate of its address (both windows), or answers 429.
/// The old `X-Scout-Key` header, still sent by older apps, is ignored.
async fn guard(State(api): State<Arc<Api>>, request: Request, next: Next) -> Response {
    let now = Instant::now();
    let ip = access::client_ip(request.headers());
    let trusted = api.trusted.contains(&ip);
    if !trusted && !(api.by_minute.allow(&ip, now) && api.by_hour.allow(&ip, now)) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "trop de requêtes, réessaie plus tard\n",
        )
            .into_response();
    }
    next.run(request).await
}

/// Answers as long as the process runs (the homelab checks 2xx); the body says how the collector is doing.
async fn health(State(api): State<Arc<Api>>) -> Json<Value> {
    let mut regions = serde_json::Map::new();
    for (name, region) in &api.regions {
        let last = region.db.last_success().await.ok().flatten();
        regions.insert(
            name.clone(),
            json!({
                "last_success": last,
                "seconds_since": last.map(|t| collector::unix_now() - t),
            }),
        );
    }
    let free = api
        .regions
        .values()
        .next()
        .and_then(|r| r.storage.free_bytes().ok());
    Json(json!({
        "regions": regions,
        "free_gb": free.map(|b| b / 1_000_000_000),
        "low_disk": free.is_some_and(|b| b < WARN_FREE_BYTES),
        "raw_stopped": free.is_some_and(|b| b < STOP_RAW_BYTES),
    }))
}

/// The regions where Forever has auction houses (those that answered at least once).
async fn regions(State(api): State<Arc<Api>>) -> Response {
    let mut active = Vec::new();
    for (name, region) in &api.regions {
        match region.db.houses().await {
            Ok(houses) if !houses.is_empty() => active.push(name.clone()),
            Ok(_) => {}
            Err(e) => return internal(&e),
        }
    }
    Json(active).into_response()
}

fn unknown_region() -> Response {
    (StatusCode::NOT_FOUND, "région inconnue\n").into_response()
}

async fn houses(State(api): State<Arc<Api>>, Path(name): Path<String>) -> Response {
    let Some(Region { db, .. }) = api.regions.get(&name) else {
        return unknown_region();
    };
    match db.houses().await {
        Ok(houses) => Json(houses).into_response(),
        Err(e) => internal(&e),
    }
}

#[derive(Deserialize)]
struct Since {
    #[serde(default)]
    since: i64,
}

/// CSV of the prices of a house after `since` (Unix seconds), oldest first, whole snapshots only.
/// `X-Scout-Until` is the `since` of the next page, `X-Scout-More: 1` says there is one.
async fn prices(
    State(api): State<Arc<Api>>,
    Path((name, house)): Path<(String, u32)>,
    Query(Since { since }): Query<Since>,
) -> Response {
    let Some(Region { db, .. }) = api.regions.get(&name) else {
        return unknown_region();
    };
    let page = match db.prices_after(house, since, PAGE_ROWS).await {
        Ok(page) => page,
        Err(e) => return internal(&e),
    };
    let csv = to_csv(&page.rows);
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::HeaderName::from_static("x-scout-until"),
                page.until.to_string(),
            ),
            (
                header::HeaderName::from_static("x-scout-more"),
                u8::from(page.more).to_string(),
            ),
        ],
        csv,
    )
        .into_response()
}

/// The newest version of the house's listings, one line per item (`quantity = 0`: just left).
async fn latest(State(api): State<Arc<Api>>, Path((name, house)): Path<(String, u32)>) -> Response {
    let Some(Region { db, .. }) = api.regions.get(&name) else {
        return unknown_region();
    };
    match db.latest(house).await {
        Ok(rows) => (
            [(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            to_csv(&rows),
        )
            .into_response(),
        Err(e) => internal(&e),
    }
}

/// Every line of one item after `since`, oldest first.
async fn item_history(
    State(api): State<Arc<Api>>,
    Path((name, house, item_id)): Path<(String, u32, u32)>,
    Query(Since { since }): Query<Since>,
) -> Response {
    let Some(Region { db, .. }) = api.regions.get(&name) else {
        return unknown_region();
    };
    match db.item_history(house, item_id, since).await {
        Ok(rows) => (
            [(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            to_csv(&rows),
        )
            .into_response(),
        Err(e) => internal(&e),
    }
}

#[derive(Deserialize)]
struct Format {
    format: i64,
}

/// The newest game data package of the app's `GAME_FORMAT` (`Manifest`), 404 when none is built.
async fn gamedata_latest(
    State(api): State<Arc<Api>>,
    Query(Format { format }): Query<Format>,
) -> Response {
    match api.gamedata.latest(format) {
        Some(manifest) => Json(manifest).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "pas de données du jeu pour ce format\n",
        )
            .into_response(),
    }
}

/// The package itself (gzipped JSON), only while it is the newest of its format.
async fn gamedata_package(
    State(api): State<Arc<Api>>,
    Path((format, updated)): Path<(i64, i64)>,
) -> Response {
    let Some(path) = api.gamedata.package_path(format, updated) else {
        return (StatusCode::NOT_FOUND, "paquet inconnu ou remplacé\n").into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/gzip")], bytes).into_response(),
        Err(e) => internal(&anyhow::Error::from(e).context(path.display().to_string())),
    }
}

/// `ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less`, empty for `NULL`.
fn to_csv(rows: &[PriceRow]) -> String {
    let mut csv = String::with_capacity(40 * rows.len() + 100);
    csv.push_str("ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n");
    let opt = |v: Option<i64>| v.map(|v| v.to_string()).unwrap_or_default();
    for r in rows {
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{},{}",
            r.ts,
            r.item_id,
            opt(r.min_unit),
            opt(r.p25_unit),
            opt(r.median_unit),
            r.quantity,
            r.auctions,
            r.buyout_less
        );
    }
    csv
}

async fn privacy() -> Html<&'static str> {
    Html(include_str!("api/privacy.html"))
}

async fn thanks() -> Html<&'static str> {
    Html(thanks::page())
}

fn internal(e: &anyhow::Error) -> Response {
    eprintln!("API : {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne\n").into_response()
}

mod thanks;

#[cfg(test)]
mod tests;
