use axum::body::{Body, to_bytes};
use axum::http::Request as HttpRequest;
use tower::ServiceExt;

use super::*;
use crate::aggregate::ItemPrices;
use crate::db::NewSnapshot;

const KEY: &str = "0123456789abcdef0123";

async fn api(ip_burst: u32, key_burst: u32) -> Router {
    let db = Db::memory().await.unwrap();
    db.upsert_house(114, "Hôtel des ventes de la Horde", 50)
        .await
        .unwrap();
    let row = |item_id, min| ItemPrices {
        item_id,
        min_unit: min,
        p25_unit: min,
        median_unit: min,
        quantity: 4,
        auctions: 2,
        buyout_less: u32::from(min.is_none()),
    };
    for (ts, rows) in [
        (100, vec![row(768, Some(147)), row(7, None)]),
        (200, vec![row(768, Some(150))]),
    ] {
        let snapshot = NewSnapshot {
            house: 114,
            kind: "auctions",
            fetched_at: ts,
            last_modified: None,
            sha256: &[ts as u8],
            path: None,
            raw_bytes: 0,
            bytes: 0,
            auctions: 2,
        };
        db.insert_snapshot(&snapshot, &rows).await.unwrap();
    }
    router(Arc::new(Api {
        db,
        storage: Arc::new(Storage::new(&std::env::temp_dir(), 3)),
        keys: Keys::parse(&format!("test={KEY}")),
        by_key: Limiter::new(key_burst, 60),
        by_ip: Limiter::new(ip_burst, 60),
    }))
}

async fn get(app: &Router, uri: &str, key: Option<&str>) -> (StatusCode, Response) {
    let mut request = HttpRequest::get(uri).header("x-forwarded-for", "203.0.113.7");
    if let Some(key) = key {
        request = request.header("x-scout-key", key);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    (response.status(), response)
}

async fn text(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn health_and_privacy_need_no_key() {
    let app = api(5, 5).await;
    assert_eq!(get(&app, "/health", None).await.0, StatusCode::OK);
    let (status, response) = get(&app, "/privacy", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(text(response).await.contains("Blizzard Entertainment"));
}

#[tokio::test]
async fn v1_wants_a_key_and_slows_down_guessers() {
    let app = api(2, 5).await;
    assert_eq!(
        get(&app, "/v1/houses", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, "/v1/houses", Some("0123456789abcdef0124"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, "/v1/houses", None).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // A valid key is not slowed down by the guesses from the same address.
    let (status, response) = get(&app, "/v1/houses", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    let houses: Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(houses[0]["id"], 114);
    assert_eq!(houses[0]["last_update"], 200);
}

#[tokio::test]
async fn keys_have_their_own_rate() {
    let app = api(5, 1).await;
    assert_eq!(get(&app, "/v1/houses", Some(KEY)).await.0, StatusCode::OK);
    assert_eq!(
        get(&app, "/v1/houses", Some(KEY)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn prices_come_as_csv_pages() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/houses/114/prices?since=100", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()["x-scout-until"], "200");
    assert_eq!(response.headers()["x-scout-more"], "0");
    assert_eq!(
        text(response).await,
        "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n200,768,150,150,150,4,2,0\n"
    );
    let (_, response) = get(&app, "/v1/houses/114/prices", Some(KEY)).await;
    let csv = text(response).await;
    assert!(csv.contains("\n100,7,,,,4,2,1\n"), "{csv}");
}

#[tokio::test]
async fn answers_are_gzipped_on_request() {
    let app = api(5, 5).await;
    let request = HttpRequest::get("/privacy")
        .header("accept-encoding", "gzip")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.headers()["content-encoding"], "gzip");
}
