use axum::body::{Body, to_bytes};
use axum::http::Request as HttpRequest;
use tower::ServiceExt;

use super::*;
use crate::aggregate::ItemPrices;
use crate::db::NewSnapshot;

async fn api(minute_burst: u32, hour_burst: u32) -> Router {
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
        let sold = [crate::sales::ItemSales {
            item_id: 768,
            sold_proven: 2,
            sold_estimated: 1,
            sold_value: 450,
        }];
        let sales = if ts == 200 { &sold[..] } else { &[] };
        db.insert_snapshot(&snapshot, &rows, sales).await.unwrap();
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
        by_minute: Limiter::new(minute_burst, 60),
        by_hour: Limiter::new(hour_burst, 60),
        trusted: vec!["192.0.2.9".to_owned()],
    }))
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Response) {
    let request = HttpRequest::get(uri).header("x-forwarded-for", "203.0.113.7");
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
async fn health_and_privacy_are_open() {
    let app = api(5, 5).await;
    assert_eq!(get(&app, "/health").await.0, StatusCode::OK);
    let (status, response) = get(&app, "/privacy").await;
    assert_eq!(status, StatusCode::OK);
    let page = text(response).await;
    assert!(page.contains("Blizzard Entertainment"));
    assert!(
        !page.contains("clé"),
        "the privacy page still mentions keys"
    );
    assert!(page.contains("scout-addon/issues"));
}

#[tokio::test]
async fn thanks_needs_no_key() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/thanks").await;
    assert_eq!(status, StatusCode::OK);
    assert!(text(response).await.contains("wago.tools"));
}

#[tokio::test]
async fn v1_is_open_and_ignores_the_old_key() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/eu/houses").await;
    assert_eq!(status, StatusCode::OK);
    let houses: Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(houses[0]["id"], 114);
    assert_eq!(houses[0]["last_update"], 200);
    // An app still sending its key gets the same answer.
    let request = HttpRequest::get("/v1/regions")
        .header("x-scout-key", "0123456789abcdef0123")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn only_regions_with_houses_are_listed() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/regions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text(response).await, r#"["eu"]"#);
    assert_eq!(get(&app, "/v1/xx/houses").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn each_address_has_a_minute_and_an_hour_rate() {
    let app = api(2, 3).await;
    let get_from = |ip: &'static str| {
        let app = app.clone();
        async move {
            let request = HttpRequest::get("/v1/eu/houses")
                .header("x-forwarded-for", ip)
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await.unwrap().status()
        }
    };
    assert_eq!(get_from("203.0.113.7").await, StatusCode::OK);
    assert_eq!(get_from("203.0.113.7").await, StatusCode::OK);
    // The minute window is empty.
    assert_eq!(get_from("203.0.113.7").await, StatusCode::TOO_MANY_REQUESTS);
    // Another address has its own buckets.
    assert_eq!(get_from("198.51.100.1").await, StatusCode::OK);
    // A trusted address is never limited.
    for _ in 0..5 {
        assert_eq!(get_from("192.0.2.9").await, StatusCode::OK);
    }
    // /health is not limited.
    assert_eq!(get(&app, "/health").await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_hour_window_limits_too() {
    let app = api(10, 2).await;
    assert_eq!(get(&app, "/v1/eu/houses").await.0, StatusCode::OK);
    assert_eq!(get(&app, "/v1/eu/houses").await.0, StatusCode::OK);
    assert_eq!(
        get(&app, "/v1/eu/houses").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn prices_come_as_csv_pages() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/eu/houses/114/prices?since=100").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()["x-scout-until"], "200");
    assert_eq!(response.headers()["x-scout-more"], "0");
    assert_eq!(
        text(response).await,
        "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n200,768,150,150,150,4,2,0\n"
    );
    let (_, response) = get(&app, "/v1/eu/houses/114/prices").await;
    let csv = text(response).await;
    assert!(csv.contains("\n100,7,,,,4,2,1\n"), "{csv}");
}

#[tokio::test]
async fn latest_prices_and_one_item_history() {
    let app = api(5, 5).await;
    let (status, response) = get(&app, "/v1/eu/houses/114/latest").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        text(response).await,
        "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n200,768,150,150,150,4,2,0\n"
    );
    let (_, response) = get(&app, "/v1/eu/houses/114/items/768/history").await;
    let csv = text(response).await;
    assert_eq!(
        csv.lines().skip(1).collect::<Vec<_>>(),
        ["100,768,147,147,147,4,2,0", "200,768,150,150,150,4,2,0"]
    );
    let (_, response) = get(&app, "/v1/eu/houses/114/items/768/history?since=100").await;
    assert_eq!(text(response).await.lines().count(), 2);
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
    let (status, response) = get(&app, &format!("/v1/gamedata/latest?format={format}")).await;
    assert_eq!(status, StatusCode::OK);
    let manifest: scout_gamedata::Manifest = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(manifest.updated, 1000);
    let (status, _) = get(&app, &format!("/v1/gamedata/latest?format={}", format + 1)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, response) = get(&app, &format!("/v1/gamedata/{format}/1000")).await;
    assert_eq!(status, StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        scout_gamedata::Package::decode(&bytes).unwrap().build,
        "1.60.1.71037"
    );
    let (status, _) = get(&app, &format!("/v1/gamedata/{format}/999")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reference_package() {
    let app = api(10, 10).await;
    let format = scout_core::reference::REFERENCE_FORMAT;
    let (status, response) = get(&app, &format!("/v1/reference/latest?format={format}")).await;
    assert_eq!(status, StatusCode::OK);
    let manifest: scout_core::reference::ReferenceManifest =
        serde_json::from_str(&text(response).await).unwrap();
    let (status, _) = get(&app, &format!("/v1/reference/latest?format={}", format + 1)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let uri = format!("/v1/reference/{format}/{}", manifest.updated);
    let (status, response) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/gzip");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(bytes.len() as u64, manifest.bytes);
    let (status, _) = get(
        &app,
        &format!("/v1/reference/{format}/{}", manifest.updated - 1),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sales_come_by_day_and_item() {
    let app = api(50, 50).await;
    let (status, response) = get(&app, "/v1/eu/houses/114/sales?since=1970-01-01").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(body["days"][0]["day"], "1970-01-01");
    assert_eq!(body["days"][0]["items"][0]["item_id"], 768);
    assert_eq!(body["days"][0]["items"][0]["sold_value"], 450);
    // The default window is the last 7 days: the 1970 sales are out of it.
    let (_, response) = get(&app, "/v1/eu/houses/114/sales").await;
    let body: Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(body["days"], json!([]));
    assert_eq!(
        get(&app, "/v1/eu/houses/114/sales?since=hier").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(&app, "/v1/xx/houses/114/sales").await.0,
        StatusCode::NOT_FOUND
    );
}
