//! The tool's own releases on GitHub (public API, no account needed).

use std::time::Duration;

use anyhow::{Context, Result};
use scout_core::update::{BINARY, Release};
use serde::Deserialize;

const LATEST: &str = "https://api.github.com/repos/Darkrizen7/scout-rs/releases/latest";
/// The binary is about 30 MB.
const MAX_BYTES: u64 = 256 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("scout-rs")
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The newest published release, if it has a binary for this platform.
pub fn latest_release() -> Result<Option<Release>> {
    let mut response = agent()
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("dernière version sur GitHub")?;
    let reader = response.body_mut().with_config().limit(MAX_BYTES).reader();
    let release: GithubRelease =
        serde_json::from_reader(reader).context("réponse de GitHub illisible")?;
    let Some(binary) = BINARY else {
        return Ok(None);
    };
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Ok(url(binary)
        .zip(url(&format!("{binary}.sha256")))
        .map(|(binary_url, checksum_url)| Release {
            version: release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name)
                .to_owned(),
            binary_url,
            checksum_url,
        }))
}

/// One file of a release.
pub fn download(url: &str) -> Result<Vec<u8>> {
    let mut response = agent()
        .get(url)
        .call()
        .with_context(|| format!("téléchargement de {url}"))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .with_context(|| format!("lecture de {url}"))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    /// The published release has this platform's binary, and its checksum matches.
    #[test]
    #[ignore = "network: GitHub"]
    fn latest_release_downloads() {
        let release = latest_release()
            .unwrap()
            .expect("no binary for this platform");
        let checksum = String::from_utf8(download(&release.checksum_url).unwrap()).unwrap();
        let expected = scout_core::update::parse_checksum(&checksum).unwrap();
        let binary = download(&release.binary_url).unwrap();
        let actual: String = Sha256::digest(&binary)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(actual, expected);
        println!("{} : {} octets", release.version, binary.len());
    }
}
