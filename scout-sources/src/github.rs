//! Releases of the tool and of the Scout addon on GitHub (public API, no account needed).

use std::time::Duration;

use anyhow::{Context, Result, bail};
use scout_core::update::{self, Release};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// This tool.
pub const TOOL_REPO: &str = "Darkrizen7/scout-rs";
/// The Scout addon.
pub const ADDON_REPO: &str = "Darkrizen7/scout-addon";
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

/// The newest published release of `repo`, if it has `file` (and its `.sha256`).
pub fn latest_release(repo: &str, file: &str) -> Result<Option<Release>> {
    let mut response = agent()
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("dernière version de {repo} sur GitHub"))?;
    let reader = response.body_mut().with_config().limit(MAX_BYTES).reader();
    let release: GithubRelease =
        serde_json::from_reader(reader).context("réponse de GitHub illisible")?;
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Ok(url(file)
        .zip(url(&format!("{file}.sha256")))
        .map(|(url, checksum_url)| Release {
            version: release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name)
                .to_owned(),
            url,
            checksum_url,
        }))
}

/// One file of a release, checked against the release's `.sha256`.
pub fn download_checked(release: &Release) -> Result<Vec<u8>> {
    let checksum = String::from_utf8(download(&release.checksum_url)?)
        .ok()
        .as_deref()
        .and_then(update::parse_checksum)
        .context("somme de contrôle illisible")?;
    let bytes = download(&release.url)?;
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != checksum {
        bail!(
            "fichier corrompu (somme de contrôle différente) : {}",
            release.url
        );
    }
    Ok(bytes)
}

fn download(url: &str) -> Result<Vec<u8>> {
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
    use scout_core::update::{ADDON_ARCHIVE, BINARY};

    use super::*;

    /// The published releases have their files, and the checksums match.
    #[test]
    #[ignore = "network: GitHub"]
    fn latest_releases_download() {
        let binary = BINARY.expect("no binary for this platform");
        for (repo, file) in [(TOOL_REPO, binary), (ADDON_REPO, ADDON_ARCHIVE)] {
            let release = latest_release(repo, file)
                .unwrap()
                .expect("no release file");
            let bytes = download_checked(&release).unwrap();
            println!("{repo} {} : {} octets", release.version, bytes.len());
        }
    }
}
