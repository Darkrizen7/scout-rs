//! `/v1/{region}/houses/{id}/sales`: what sold per day and item (`sales.rs`, `db/sales.rs`).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Api, Region, internal, unknown_region};

#[derive(Deserialize)]
pub(super) struct From {
    /// `YYYY-MM-DD` (UTC); default: the last 7 days.
    since: Option<String>,
}

/// `YYYY-MM-DD` with digits only: the database compares days as text.
fn is_day(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// JSON `{"since", "days": [{"day", "items": [{"item_id", "sold_proven", "sold_estimated",
/// "sold_value"}]}]}`, oldest day first; `sold_value` is in copper (units x unit price).
pub(super) async fn sales(
    State(api): State<Arc<Api>>,
    Path((name, house)): Path<(String, u32)>,
    Query(From { since }): Query<From>,
) -> Response {
    let Some(Region { db, .. }) = api.regions.get(&name) else {
        return unknown_region();
    };
    if since.as_deref().is_some_and(|s| !is_day(s)) {
        return (StatusCode::BAD_REQUEST, "since attendu : AAAA-MM-JJ\n").into_response();
    }
    let rows = match db.sales_since(house, since.as_deref()).await {
        Ok(rows) => rows,
        Err(e) => return internal(&e),
    };
    let mut days: Vec<Value> = Vec::new();
    for row in rows {
        let item = json!({
            "item_id": row.item_id,
            "sold_proven": row.sold_proven,
            "sold_estimated": row.sold_estimated,
            "sold_value": row.sold_value,
        });
        match days.last_mut() {
            Some(day) if day["day"] == row.day => {
                day["items"].as_array_mut().expect("items").push(item);
            }
            _ => days.push(json!({"day": row.day, "items": [item]})),
        }
    }
    Json(json!({"since": since, "days": days})).into_response()
}

#[cfg(test)]
mod tests {
    use super::is_day;

    #[test]
    fn days_are_strict() {
        assert!(is_day("2026-11-25"));
        assert!(!is_day("2026-1-25"));
        assert!(!is_day("2026/11/25"));
        assert!(!is_day("2026-11-2x"));
    }
}
