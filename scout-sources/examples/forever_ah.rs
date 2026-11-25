//! Throwaway probe of the Battle.net WoW: Forever auction API.
//!
//! `BLIZZARD_CLIENT_ID=… BLIZZARD_CLIENT_SECRET=… cargo run -p scout-sources --example forever_ah -- <out_dir>`
//! (optional `BLIZZARD_REGION`, default `eu`). Writes the raw JSON of every response and, per house,
//! a CSV `item_id,auctions,quantity,min_unit,median_unit` (copper) into `out_dir`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The busiest house is about 9 MB.
const MAX_BYTES: u64 = 128 * 1024 * 1024;
/// The ruleset must exist but does not filter the houses (2 = Normal, in every region).
const RULESET: u32 = 2;

fn main() -> Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("usage: forever_ah <out_dir>")?,
    );
    std::fs::create_dir_all(&out)?;
    let id = std::env::var("BLIZZARD_CLIENT_ID").context("BLIZZARD_CLIENT_ID manquant")?;
    let secret =
        std::env::var("BLIZZARD_CLIENT_SECRET").context("BLIZZARD_CLIENT_SECRET manquant")?;
    let region = std::env::var("BLIZZARD_REGION").unwrap_or_else(|_| "eu".into());

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent("scout-rs")
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(120)))
        .build()
        .into();
    let token = token(&agent, &id, &secret)?;
    println!("jeton OK");

    let api = Api {
        agent,
        token,
        region,
    };
    let list = api.get(
        &format!("/data/wow/ruleset/{RULESET}/auctions/"),
        &out.join("houses.json"),
    )?;
    let mut houses = house_ids(&list);
    if houses.is_empty() {
        println!("liste des maisons non comprise (voir houses.json), essai de 110 à 125");
        houses = (110..=125).map(|id| (id, String::new())).collect();
    }

    for (house, name) in houses {
        for (kind, suffix) in [("auctions", ""), ("commodities", "/commodities")] {
            let path = format!("/data/wow/ruleset/{RULESET}/auctions/{house}{suffix}");
            let start = Instant::now();
            let json = match api.get(&path, &out.join(format!("{house}_{kind}.json"))) {
                Ok(json) => json,
                Err(e) => {
                    println!("{house} {kind} : {e:#}");
                    continue;
                }
            };
            let stats = summarize(&json);
            let total: u64 = stats.values().map(|s| s.auctions).sum();
            println!(
                "{house} {name:<26} {kind:<11} {total:>6} enchères, {:>5} objets, {:.1} s",
                stats.len(),
                start.elapsed().as_secs_f32()
            );
            write_csv(&out.join(format!("{house}_{kind}.csv")), &stats)?;
        }
    }
    Ok(())
}

struct Api {
    agent: ureq::Agent,
    token: String,
    region: String,
}

impl Api {
    /// GETs `path`, saves the raw body to `save` and parses it.
    fn get(&self, path: &str, save: &Path) -> Result<Value> {
        let url = format!("https://{}.api.blizzard.com{path}", self.region);
        let mut response = self
            .agent
            .get(&url)
            .query("namespace", format!("dynamic-forever-{}", self.region))
            .query("locale", "fr_FR")
            .header("Authorization", format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("GET {path}"))?;
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BYTES)
            .read_to_string()?;
        std::fs::write(save, &body)?;
        serde_json::from_str(&body).context("JSON illisible")
    }
}

fn token(agent: &ureq::Agent, id: &str, secret: &str) -> Result<String> {
    let mut response = agent
        .post("https://oauth.battle.net/token")
        .header(
            "Authorization",
            format!("Basic {}", base64(&format!("{id}:{secret}"))),
        )
        .send_form([("grant_type", "client_credentials")])
        .context("jeton OAuth refusé (identifiants ?)")?;
    let json: Value = serde_json::from_reader(response.body_mut().as_reader())?;
    match json["access_token"].as_str() {
        Some(token) => Ok(token.to_owned()),
        None => bail!("pas de access_token dans {json}"),
    }
}

/// `(id, name)` of every house in the list, whatever its exact shape: any object with a numeric `id`.
fn house_ids(list: &Value) -> Vec<(u64, String)> {
    let mut found = Vec::new();
    collect_ids(list, &mut found);
    found.sort();
    found.dedup();
    found
}

fn collect_ids(value: &Value, found: &mut Vec<(u64, String)>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map.get("id").and_then(Value::as_u64) {
                let name = map.get("name").and_then(Value::as_str).unwrap_or_default();
                found.push((id, name.to_owned()));
            }
            map.values().for_each(|v| collect_ids(v, found));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_ids(v, found)),
        _ => {}
    }
}

#[derive(Default)]
struct Stats {
    auctions: u64,
    quantity: u64,
    /// Unit buyouts in copper, one per auction (bid-only auctions are skipped).
    units: Vec<u64>,
}

fn summarize(json: &Value) -> BTreeMap<u64, Stats> {
    let mut stats: BTreeMap<u64, Stats> = BTreeMap::new();
    for auction in json["auctions"].as_array().into_iter().flatten() {
        let Some(item) = auction["item"]["id"].as_u64() else {
            continue;
        };
        let quantity = auction["quantity"].as_u64().unwrap_or(1).max(1);
        let entry = stats.entry(item).or_default();
        entry.auctions += 1;
        entry.quantity += quantity;
        let unit = auction["unit_price"]
            .as_u64()
            .or_else(|| auction["buyout"].as_u64().map(|b| b / quantity));
        entry.units.extend(unit);
    }
    stats
}

fn write_csv(path: &Path, stats: &BTreeMap<u64, Stats>) -> Result<()> {
    let mut csv = String::from("item_id,auctions,quantity,min_unit,median_unit\n");
    for (item, s) in stats {
        let mut units = s.units.clone();
        units.sort_unstable();
        let min = units.first().map(u64::to_string).unwrap_or_default();
        let median = units
            .get(units.len() / 2)
            .map(u64::to_string)
            .unwrap_or_default();
        csv += &format!("{item},{},{},{min},{median}\n", s.auctions, s.quantity);
    }
    Ok(std::fs::write(path, csv)?)
}

fn base64(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
