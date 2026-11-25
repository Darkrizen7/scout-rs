//! Our price server (`scout-server`, on the homelab): auction prices of WoW: Forever from the
//! Battle.net API.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use scout_core::reference::ReferenceManifest;
use scout_core::sales::{SalesAnswer, SalesDay};
use scout_core::scout_api::{AuctionHouse, PriceLine, ServerHouse};
use scout_gamedata::{Manifest, Package};
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

fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    checked(agent().get(url).call())
}

fn checked(
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<ureq::http::Response<ureq::Body>> {
    match result {
        Err(ureq::Error::StatusCode(429)) => {
            bail!("le serveur de prix limite les requêtes : réessaie dans une heure")
        }
        result => Ok(result.context("serveur de prix injoignable")?),
    }
}

/// The newest game data package the server built for this tool's `format`, None while it has none.
pub fn gamedata_latest(server: &str, format: i64) -> Result<Option<Manifest>> {
    let url = format!("{}/v1/gamedata/latest?format={format}", base(server));
    let mut response = match agent().get(&url).call() {
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        result => checked(result)?,
    };
    let body = response.body_mut().with_config().limit(MAX_BYTES).reader();
    serde_json::from_reader(body).context("version des données du jeu illisible")
}

/// The game data package `manifest` describes (a few megabytes, gzipped).
pub fn gamedata_package(server: &str, manifest: &Manifest) -> Result<Package> {
    let url = format!(
        "{}/v1/gamedata/{}/{}",
        base(server),
        manifest.format,
        manifest.updated
    );
    let bytes = get(&url)?
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .context("téléchargement des données du jeu")?;
    Package::decode(&bytes)
}

/// The newest loot and merchants package of the server for this tool's `format`, None while it has none.
pub fn reference_latest(server: &str, format: i64) -> Result<Option<ReferenceManifest>> {
    let url = format!("{}/v1/reference/latest?format={format}", base(server));
    let mut response = match agent().get(&url).call() {
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        result => checked(result)?,
    };
    let body = response.body_mut().with_config().limit(MAX_BYTES).reader();
    serde_json::from_reader(body).context("version du butin de référence illisible")
}

/// The gzipped SQLite package `manifest` describes (loot and merchants only).
pub fn reference_package(server: &str, manifest: &ReferenceManifest) -> Result<Vec<u8>> {
    let url = format!(
        "{}/v1/reference/{}/{}",
        base(server),
        manifest.format,
        manifest.updated
    );
    get(&url)?
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .context("téléchargement du butin de référence")
}

/// The regions where the server found Forever auction houses (eu, us, kr, tw).
pub fn regions(server: &str) -> Result<Vec<String>> {
    read_json(
        &format!("{}/v1/regions", base(server)),
        "liste des régions illisible",
    )
}

/// The houses the server records in `region`.
pub fn houses(server: &str, region: &str) -> Result<Vec<ServerHouse>> {
    let url = format!("{}/v1/{region}/houses", base(server));
    read_json(&url, "liste des hôtels des ventes illisible")
}

/// The newest prices of every item of `house` (`quantity = 0`: just left the auction house).
pub fn latest(server: &str, house: &AuctionHouse) -> Result<Vec<PriceLine>> {
    let url = format!(
        "{}/v1/{}/houses/{}/latest",
        base(server),
        house.region,
        house.id
    );
    read_csv(&url)
}

/// The daily sales totals of `house` from the UTC day `since` (`YYYY-MM-DD`) on.
pub fn sales(server: &str, house: &AuctionHouse, since: &str) -> Result<Vec<SalesDay>> {
    let url = format!(
        "{}/v1/{}/houses/{}/sales?since={since}",
        base(server),
        house.region,
        house.id
    );
    let answer: SalesAnswer = read_json(&url, "ventes du serveur illisibles")?;
    Ok(answer.days)
}

/// Every price of one item at `house` after `since` (Unix seconds), oldest first.
pub fn history(
    server: &str,
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
    read_csv(&url)
}

fn base(server: &str) -> &str {
    server.trim_end_matches('/')
}

fn read_json<T: serde::de::DeserializeOwned>(url: &str, what: &str) -> Result<T> {
    let mut response = get(url)?;
    let body = response.body_mut().with_config().limit(MAX_BYTES).reader();
    serde_json::from_reader(body).context(what.to_owned())
}

fn read_csv(url: &str) -> Result<Vec<PriceLine>> {
    let body = get(url)?
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
