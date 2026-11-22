//! Settings (WoW folder, theme) and the paths derived from it.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// The `_forever_` folder of the WoW install.
    pub wow_dir: Option<PathBuf>,
    #[serde(default)]
    pub theme: Theme,
}

/// GNOME (libadwaita) colors, like the Files app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    #[serde(alias = "parchment")]
    Light,
    #[serde(alias = "slate")]
    Dark,
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("", "", "scout-prix").context("dossier personnel introuvable")
}

fn config_path() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("config.toml"))
}

pub fn database_path() -> Result<PathBuf> {
    let dir = project_dirs()?.data_dir().to_path_buf();
    fs::create_dir_all(&dir).with_context(|| format!("création de {}", dir.display()))?;
    Ok(dir.join("prix.db"))
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
            config.wow_dir = detect_wow_dir();
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

    fn wow_dir(&self) -> Result<&Path> {
        self.wow_dir
            .as_deref()
            .context("dossier WoW non configuré (Réglages)")
    }

    /// `tools/export.lua` of the installed Scout addon.
    pub fn export_script(&self) -> Result<PathBuf> {
        let path = self
            .wow_dir()?
            .join("Interface/AddOns/Scout/tools/export.lua");
        if !path.is_file() {
            bail!("script d'export introuvable : {}", path.display());
        }
        Ok(path)
    }

    /// Forever Guide's list of merchant prices for profession reagents, when that addon is installed.
    pub fn guide_vendor_prices(&self) -> Option<PathBuf> {
        let path = self
            .wow_dir
            .as_ref()?
            .join("Interface/AddOns/ForeverGuide/VendorPrices.lua");
        path.is_file().then_some(path)
    }

    /// The most recently written `Scout.lua` among the accounts' SavedVariables.
    pub fn saved_variables(&self) -> Result<PathBuf> {
        let accounts = self.wow_dir()?.join("WTF/Account");
        let entries = fs::read_dir(&accounts)
            .with_context(|| format!("lecture de {}", accounts.display()))?;
        entries
            .flatten()
            .map(|e| e.path().join("SavedVariables/Scout.lua"))
            .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, p)| p)
            .with_context(|| format!("aucun SavedVariables/Scout.lua sous {}", accounts.display()))
    }
}

/// Looks for `<prefix>/drive_c/Program Files (x86)/World of Warcraft/_forever_` in usual Wine prefixes.
fn detect_wow_dir() -> Option<PathBuf> {
    let home = BaseDirs::new()?.home_dir().to_path_buf();
    let mut prefixes = vec![home.join(".wine")];
    for parent in ["Games", ".local/share/lutris/prefixes"] {
        if let Ok(entries) = fs::read_dir(home.join(parent)) {
            prefixes.extend(entries.flatten().map(|e| e.path()));
        }
    }
    prefixes
        .into_iter()
        .map(|p| p.join("drive_c/Program Files (x86)/World of Warcraft/_forever_"))
        .find(|p| p.join("WTF/Account").is_dir())
}
