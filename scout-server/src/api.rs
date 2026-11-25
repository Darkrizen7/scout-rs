//! The HTTP API: `/health` and `/privacy` for everyone, `/v1/…` with an access key.

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

use crate::collector::{self, STOP_RAW_BYTES};
use crate::db::{Db, PriceRow};
use crate::storage::Storage;

mod access;

pub use access::{Keys, Limiter};

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
    pub keys: Keys,
    /// Per key: a first sync pages through the whole history.
    pub by_key: Limiter,
    /// Per address, for requests without a valid key.
    pub by_ip: Limiter,
}

pub fn router(api: Arc<Api>) -> Router {
    let v1 = Router::new()
        .route("/regions", get(regions))
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
        .nest("/v1", v1)
        .layer(CompressionLayer::new())
        .with_state(api)
}

/// A valid `X-Scout-Key` within its rate, or 401 / 429.
async fn guard(State(api): State<Arc<Api>>, request: Request, next: Next) -> Response {
    let now = Instant::now();
    let allowed = match api.keys.check(request.headers()) {
        Some(name) => api.by_key.allow(name, now),
        None => {
            let ip = access::client_ip(request.headers());
            if api.by_ip.allow(&ip, now) {
                return (
                    StatusCode::UNAUTHORIZED,
                    "clé d'accès absente ou inconnue (X-Scout-Key)\n",
                )
                    .into_response();
            }
            false
        }
    };
    if !allowed {
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

fn internal(e: &anyhow::Error) -> Response {
    eprintln!("API : {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne\n").into_response()
}

#[cfg(test)]
mod tests;
