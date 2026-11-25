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
    let storage = Arc::new(Storage::new(&std::env::temp_dir(), 3));
    let empty = Region {
        db: Db::memory().await.unwrap(),
        storage: storage.clone(),
    };
    let regions = BTreeMap::from([
        ("eu".to_owned(), Region { db, storage }),
        ("us".to_owned(), empty),
    ]);
    // One folder per router: tests run side by side.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("scout-api-{}-{n}", std::process::id()));
    let gamedata = Arc::new(crate::gamedata::tests::store_with_package(&dir, 1000));
    router(Arc::new(Api {
        regions,
        gamedata,
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
        get(&app, "/v1/eu/houses", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, "/v1/eu/houses", Some("0123456789abcdef0124"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, "/v1/eu/houses", None).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // A valid key is not slowed down by the guesses from the same address.
    let (status, response) = get(&app, "/v1/eu/houses", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    let houses: Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(houses[0]["id"], 114);
    assert_eq!(houses[0]["last_update"], 200);
}

#[tokio::test]
async fn only_regions_with_houses_are_listed() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/regions", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text(response).await, r#"["eu"]"#);
    assert_eq!(
        get(&app, "/v1/xx/houses", Some(KEY)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn keys_have_their_own_rate() {
    let app = api(5, 1).await;
    assert_eq!(
        get(&app, "/v1/eu/houses", Some(KEY)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/v1/eu/houses", Some(KEY)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn prices_come_as_csv_pages() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/eu/houses/114/prices?since=100", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()["x-scout-until"], "200");
    assert_eq!(response.headers()["x-scout-more"], "0");
    assert_eq!(
        text(response).await,
        "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n200,768,150,150,150,4,2,0\n"
    );
    let (_, response) = get(&app, "/v1/eu/houses/114/prices", Some(KEY)).await;
    let csv = text(response).await;
    assert!(csv.contains("\n100,7,,,,4,2,1\n"), "{csv}");
}

#[tokio::test]
async fn latest_prices_and_one_item_history() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/eu/houses/114/latest", Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        text(response).await,
        "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n200,768,150,150,150,4,2,0\n"
    );
    let (_, response) = get(&app, "/v1/eu/houses/114/items/768/history", Some(KEY)).await;
    let csv = text(response).await;
    assert_eq!(
        csv.lines().skip(1).collect::<Vec<_>>(),
        ["100,768,147,147,147,4,2,0", "200,768,150,150,150,4,2,0"]
    );
    let (_, response) = get(
        &app,
        "/v1/eu/houses/114/items/768/history?since=100",
        Some(KEY),
    )
    .await;
    assert_eq!(text(response).await.lines().count(), 2);
    assert_eq!(
        get(&app, "/v1/eu/houses/114/latest", None).await.0,
        StatusCode::UNAUTHORIZED
    );
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

#[tokio::test]
async fn game_data_package() {
    let app = api(10, 10).await;
    let format = scout_core::GAME_FORMAT;
    let (status, response) = get(
        &app,
        &format!("/v1/gamedata/latest?format={format}"),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let manifest: scout_gamedata::Manifest = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(manifest.updated, 1000);
    let (status, _) = get(
        &app,
        &format!("/v1/gamedata/latest?format={}", format + 1),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, response) = get(&app, &format!("/v1/gamedata/{format}/1000"), Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        scout_gamedata::Package::decode(&bytes).unwrap().build,
        "1.60.1.71037"
    );
    let (status, _) = get(&app, &format!("/v1/gamedata/{format}/999"), Some(KEY)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&app, &format!("/v1/gamedata/{format}/1000"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
