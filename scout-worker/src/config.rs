//! Settings (WoW folder, theme) and the paths of the tool's own files.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use scout_core::Theme;
use scout_core::scout_api::AuctionHouse;
use scout_sources::wow;
use serde::{Deserialize, Serialize};

const APP: &str = "scout-rs";
/// Name used before the tool was renamed; its folders are moved to the new name at start.
const OLD_APP: &str = "scout-prix";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The `_forever_` folder of the WoW install.
    pub wow_dir: Option<PathBuf>,
    #[serde(default)]
    pub theme: Theme,
    /// Hide items whose name marks them as obsolete ("Deprecated", "Unused", "périmé"…).
    #[serde(default = "yes")]
    pub hide_obsolete: bool,
    /// When the last import from Scout succeeded (Unix seconds).
    #[serde(default)]
    pub last_import: Option<i64>,
    /// Our price server (Battle.net auction API).
    #[serde(default)]
    pub price_server: PriceServer,
}

/// Where the Blizzard prices come from; nothing is synced without a house.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceServer {
    #[serde(default = "default_server")]
    pub url: String,
    /// Battle.net region of the house (eu, us, kr, tw).
    #[serde(default = "default_region")]
    pub region: String,
    /// Auction house whose prices are kept (`/v1/houses`).
    #[serde(default)]
    pub house: Option<u32>,
    /// When the last sync succeeded (Unix seconds).
    #[serde(default)]
    pub last_sync: Option<i64>,
}

impl Default for PriceServer {
    fn default() -> Self {
        Self {
            url: default_server(),
            region: default_region(),
            house: None,
            last_sync: None,
        }
    }
}

impl PriceServer {
    /// Url and house, once a house is chosen.
    pub fn ready(&self) -> Option<(&str, AuctionHouse)> {
        self.house?;
        Some((self.url.as_str(), self.auction_house()))
    }

    /// The house whose prices every page shows (`AuctionHouse::default()` until one is chosen).
    pub fn auction_house(&self) -> AuctionHouse {
        self.house
            .map_or_else(AuctionHouse::default, |id| AuctionHouse {
                region: self.region.clone(),
                id,
            })
    }
}

fn default_region() -> String {
    "eu".to_owned()
}

fn default_server() -> String {
    scout_core::scout_api::DEFAULT_SERVER.to_owned()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wow_dir: None,
            theme: Theme::default(),
            hide_obsolete: true,
            last_import: None,
            price_server: PriceServer::default(),
        }
    }
}

fn yes() -> bool {
    true
}

fn project_dirs(app: &str) -> Result<ProjectDirs> {
    ProjectDirs::from("", "", app).context("dossier personnel introuvable")
}

fn config_path() -> Result<PathBuf> {
    Ok(project_dirs(APP)?.config_dir().join("config.toml"))
}

pub fn database_path() -> Result<PathBuf> {
    let dir = project_dirs(APP)?.data_dir().to_path_buf();
    fs::create_dir_all(&dir).with_context(|| format!("création de {}", dir.display()))?;
    Ok(dir.join("prix.db"))
}

/// Moves the folders of the old name (`scout-prix`) to the new one, only when the new one does not exist yet
/// (or is an empty folder, e.g. created by an earlier start). Nothing is ever overwritten.
pub fn migrate_old_folders() -> Result<()> {
    let (old, new) = (project_dirs(OLD_APP)?, project_dirs(APP)?);
    for (from, to) in [
        (old.config_dir(), new.config_dir()),
        (old.data_dir(), new.data_dir()),
    ] {
        rename_if_new(from, to)?;
    }
    Ok(())
}

fn rename_if_new(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() && !has_content(to) {
        if to.is_dir() {
            fs::remove_dir(to)?;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from, to)
            .with_context(|| format!("déplacement de {} vers {}", from.display(), to.display()))?;
    }
    Ok(())
}

/// True when `path` exists and is not an empty folder.
fn has_content(path: &Path) -> bool {
    match fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => path.exists(),
    }
}

impl Config {
    /// Loads the saved settings; detects the WoW folder when none is saved.
    pub fn load() -> Self {
        let mut config: Self = config_path()
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        if config.wow_dir.is_none() {
            config.wow_dir = wow::detect_wow_dir();
        }
        config
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, toml::to_string(self)?)
            .with_context(|| format!("écriture de {}", path.display()))
    }

    pub fn wow_dir(&self) -> Result<&Path> {
        self.wow_dir
            .as_deref()
            .context("dossier WoW non configuré (Réglages)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_with_server_key_still_loads() {
        let text = "[price_server]\nurl = \"https://x.test\"\nkey = \"secret\"\nregion = \"us\"\nhouse = 7\n";
        let config: Config = toml::from_str(text).unwrap();
        let (url, house) = config.price_server.ready().unwrap();
        assert_eq!((url, house.id), ("https://x.test", 7));
    }

    #[test]
    fn moves_old_folder_once_and_never_overwrites() {
        let root = std::env::temp_dir().join(format!("scout-rs-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (old, new) = (root.join("scout-prix"), root.join("scout-rs"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("prix.db"), "data").unwrap();

        rename_if_new(&old, &new).unwrap();
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new.join("prix.db")).unwrap(), "data");

        // The new folder has content: a recreated old one is left alone.
        fs::create_dir_all(&old).unwrap();
        rename_if_new(&old, &new).unwrap();
        assert!(old.exists());

        // An empty new folder does not block the move.
        let (old2, new2) = (root.join("a-prix"), root.join("a-rs"));
        fs::create_dir_all(&old2).unwrap();
        fs::write(old2.join("config.toml"), "x").unwrap();
        fs::create_dir_all(&new2).unwrap();
        rename_if_new(&old2, &new2).unwrap();
        assert_eq!(fs::read_to_string(new2.join("config.toml")).unwrap(), "x");
        fs::remove_dir_all(&root).unwrap();
    }
}
