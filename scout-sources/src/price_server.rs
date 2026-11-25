//! Our price server (`scout-server`, on the homelab): auction prices of WoW: Forever from the
//! Battle.net API, read with a personal access key.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use scout_core::scout_api::{AuctionHouse, PriceLine, ServerHouse};
use serde::Deserialize;

/// The latest prices of a busy house are ~300 KB of CSV (gzipped on the way).
const MAX_BYTES: u64 = 64 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("scout-rs")
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into()
}

fn get(url: &str, key: &str) -> Result<ureq::http::Response<ureq::Body>> {
    match agent().get(url).header("X-Scout-Key", key).call() {
        Err(ureq::Error::StatusCode(401)) => {
            bail!("le serveur de prix refuse la clé d'accès (voir Réglages)")
        }
        Err(ureq::Error::StatusCode(429)) => {
            bail!("le serveur de prix limite les requêtes : réessaie dans une heure")
        }
        result => Ok(result.context("serveur de prix injoignable")?),
    }
}

/// The regions where the server found Forever auction houses (eu, us, kr, tw).
pub fn regions(server: &str, key: &str) -> Result<Vec<String>> {
    read_json(
        &format!("{}/v1/regions", base(server)),
        key,
        "liste des régions illisible",
    )
}

/// The houses the server records in `region`.
pub fn houses(server: &str, key: &str, region: &str) -> Result<Vec<ServerHouse>> {
    let url = format!("{}/v1/{region}/houses", base(server));
    read_json(&url, key, "liste des hôtels des ventes illisible")
}

/// The newest prices of every item of `house` (`quantity = 0`: just left the auction house).
pub fn latest(server: &str, key: &str, house: &AuctionHouse) -> Result<Vec<PriceLine>> {
    let url = format!(
        "{}/v1/{}/houses/{}/latest",
        base(server),
        house.region,
        house.id
    );
    read_csv(&url, key)
}

/// Every price of one item at `house` after `since` (Unix seconds), oldest first.
pub fn history(
    server: &str,
    key: &str,
    house: &AuctionHouse,
    item_id: i64,
    since: i64,
) -> Result<Vec<PriceLine>> {
    let url = format!(
        "{}/v1/{}/houses/{}/items/{item_id}/history?since={since}",
        base(server),
        house.region,
        house.id
    );
    read_csv(&url, key)
}

fn base(server: &str) -> &str {
    server.trim_end_matches('/')
}

fn read_json<T: serde::de::DeserializeOwned>(url: &str, key: &str, what: &str) -> Result<T> {
    let mut response = get(url, key)?;
    let body = response.body_mut().with_config().limit(MAX_BYTES).reader();
    serde_json::from_reader(body).context(what.to_owned())
}

fn read_csv(url: &str, key: &str) -> Result<Vec<PriceLine>> {
    let body = get(url, key)?
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_string()
        .context("prix du serveur")?;
    parse(&body)
}

/// `ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less`; only what the market keeps.
fn parse(csv: &str) -> Result<Vec<PriceLine>> {
    #[derive(Deserialize)]
    struct Line {
        ts: i64,
        item_id: i64,
        min_unit: Option<i64>,
        quantity: i64,
    }
    csv::Reader::from_reader(csv.as_bytes())
        .deserialize()
        .map(|line| {
            let line: Line = line.context("ligne de prix illisible")?;
            Ok(PriceLine {
                ts: line.ts,
                item_id: line.item_id,
                min_unit: line.min_unit,
                quantity: line.quantity,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_csv() {
        let csv = "ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n\
                   100,768,147,147,150,4,2,0\n100,7,,,,0,0,0\n";
        assert_eq!(
            parse(csv).unwrap(),
            [
                PriceLine {
                    ts: 100,
                    item_id: 768,
                    min_unit: Some(147),
                    quantity: 4
                },
                PriceLine {
                    ts: 100,
                    item_id: 7,
                    min_unit: None,
                    quantity: 0
                },
            ]
        );
        assert!(
            parse("ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less\n")
                .unwrap()
                .is_empty()
        );
    }
}
