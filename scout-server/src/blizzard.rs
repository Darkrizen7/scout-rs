//! Battle.net API client for WoW: Forever's auction houses (blocking, run in `spawn_blocking`).
//!
//! Forever has no realms: the houses hang off a ruleset, which must exist but does not filter them
//! (every ruleset lists the same 15 houses), so ruleset 2 (Normal, in every region) is always used.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const RULESET: u32 = 2;
/// The busiest house is about 10 MB; ureq's default limit is 10 MB.
const MAX_BYTES: u64 = 128 * 1024 * 1024;
/// A token is renewed this long before it expires (tokens last 24 h).
const TOKEN_MARGIN: Duration = Duration::from_secs(600);

/// The two listings of a house.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Non-stackable items, one buyout per auction.
    Auctions,
    /// Stackable items, one unit price per auction.
    Commodities,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Auctions, Kind::Commodities];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Auctions => "auctions",
            Kind::Commodities => "commodities",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Kind::Auctions => "",
            Kind::Commodities => "/commodities",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct House {
    pub id: u32,
    pub name: String,
}

/// The answer to a conditional GET.
#[derive(Debug)]
pub enum Fetched {
    /// 304: unchanged since the `Last-Modified` we sent.
    NotModified,
    Body {
        bytes: Vec<u8>,
        last_modified: Option<String>,
    },
}

pub struct Credentials {
    pub region: String,
    pub client_id: String,
    pub client_secret: String,
}

struct Token {
    value: String,
    renew_at: Instant,
}

pub struct Client {
    agent: ureq::Agent,
    credentials: Credentials,
    token: Mutex<Option<Token>>,
}

impl Client {
    pub fn new(credentials: Credentials) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent("scout-server")
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(180)))
            .build()
            .into();
        Self {
            agent,
            credentials,
            token: Mutex::new(None),
        }
    }

    /// The houses of the region, empty ones included.
    pub fn houses(&self) -> Result<Vec<House>> {
        match self.get(&format!("/data/wow/ruleset/{RULESET}/auctions/"), None)? {
            Fetched::Body { bytes, .. } => parse_houses(&bytes),
            Fetched::NotModified => bail!("304 inattendu sur la liste des maisons"),
        }
    }

    /// One listing of a house; `since` is the `Last-Modified` of the copy we already have.
    pub fn listing(&self, house: u32, kind: Kind, since: Option<&str>) -> Result<Fetched> {
        let path = format!(
            "/data/wow/ruleset/{RULESET}/auctions/{house}{}",
            kind.suffix()
        );
        self.get(&path, since)
    }

    /// GET with the current token, renewed once if Battle.net refuses it.
    fn get(&self, path: &str, since: Option<&str>) -> Result<Fetched> {
        for attempt in 0..2 {
            let token = self.token()?;
            let region = &self.credentials.region;
            let mut request = self
                .agent
                .get(format!("https://{region}.api.blizzard.com{path}"))
                .query("namespace", format!("dynamic-forever-{region}"))
                .query("locale", locale(region))
                .header("Authorization", format!("Bearer {token}"));
            if let Some(since) = since {
                request = request.header("If-Modified-Since", since);
            }
            let mut response = request.call().with_context(|| format!("GET {path}"))?;
            match response.status().as_u16() {
                200 => {
                    let last_modified = response
                        .headers()
                        .get("last-modified")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    let bytes = response
                        .body_mut()
                        .with_config()
                        .limit(MAX_BYTES)
                        .read_to_vec()
                        .with_context(|| format!("corps de {path}"))?;
                    return Ok(Fetched::Body {
                        bytes,
                        last_modified,
                    });
                }
                304 => return Ok(Fetched::NotModified),
                401 if attempt == 0 => *self.token.lock().expect("token lock") = None,
                status => bail!("GET {path} : HTTP {status}"),
            }
        }
        bail!("GET {path} : jeton refusé deux fois")
    }

    /// The cached token, or a new one (client credentials flow).
    fn token(&self) -> Result<String> {
        let mut cached = self.token.lock().expect("token lock");
        if let Some(token) = cached.as_ref().filter(|t| Instant::now() < t.renew_at) {
            return Ok(token.value.clone());
        }
        let Credentials {
            client_id,
            client_secret,
            ..
        } = &self.credentials;
        let mut response = self
            .agent
            .post("https://oauth.battle.net/token")
            .header(
                "Authorization",
                format!("Basic {}", base64(&format!("{client_id}:{client_secret}"))),
            )
            .send_form([("grant_type", "client_credentials")])
            .context("demande de jeton Battle.net")?;
        let status = response.status().as_u16();
        if status != 200 {
            bail!("jeton Battle.net refusé : HTTP {status} (identifiants ?)");
        }
        let token: TokenResponse = serde_json::from_reader(response.body_mut().as_reader())
            .context("réponse de jeton illisible")?;
        let lifetime = Duration::from_secs(token.expires_in).saturating_sub(TOKEN_MARGIN);
        *cached = Some(Token {
            value: token.access_token.clone(),
            renew_at: Instant::now() + lifetime,
        });
        Ok(token.access_token)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

pub fn parse_houses(bytes: &[u8]) -> Result<Vec<House>> {
    #[derive(Deserialize)]
    struct List {
        auctions: Vec<House>,
    }
    let list: List = serde_json::from_slice(bytes).context("liste des maisons illisible")?;
    Ok(list.auctions)
}

/// Names in the region's own language (a locale the region does not serve gives no names).
fn locale(region: &str) -> &'static str {
    match region {
        "us" => "en_US",
        "kr" => "ko_KR",
        "tw" => "zh_TW",
        _ => "fr_FR",
    }
}

fn base64(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        assert_eq!(base64("a"), "YQ==");
        assert_eq!(base64("ab"), "YWI=");
        assert_eq!(base64("abc"), "YWJj");
        assert_eq!(base64("id:secret"), "aWQ6c2VjcmV0");
    }

    #[test]
    fn reads_the_house_list() {
        let json = r#"{"_links":{"self":{"href":"x"}},"auctions":[
            {"key":{"href":"x/110"},"name":"Hôtel des ventes de l’Alliance","id":110},
            {"key":{"href":"x/114"},"name":"Hôtel des ventes de la Horde","id":114}]}"#;
        let houses = parse_houses(json.as_bytes()).unwrap();
        assert_eq!(houses.len(), 2);
        assert_eq!(houses[1].id, 114);
        assert_eq!(houses[1].name, "Hôtel des ventes de la Horde");
    }
}
