//! Where things are inside the WoW Forever folder (`_forever_`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;

/// `tools/export.lua` of the installed Scout addon.
pub fn export_script(wow_dir: &Path) -> Result<PathBuf> {
    let path = wow_dir.join("Interface/AddOns/Scout/tools/export.lua");
    if !path.is_file() {
        bail!("script d'export introuvable : {}", path.display());
    }
    Ok(path)
}

/// Forever Guide's list of merchant prices for profession reagents, when that addon is installed.
pub fn guide_vendor_prices(wow_dir: &Path) -> Option<PathBuf> {
    let path = wow_dir.join("Interface/AddOns/ForeverGuide/VendorPrices.lua");
    path.is_file().then_some(path)
}

/// The most recently written `Scout.lua` among the accounts' SavedVariables.
pub fn saved_variables(wow_dir: &Path) -> Result<PathBuf> {
    let accounts = wow_dir.join("WTF/Account");
    let entries =
        fs::read_dir(&accounts).with_context(|| format!("lecture de {}", accounts.display()))?;
    entries
        .flatten()
        .map(|e| e.path().join("SavedVariables/Scout.lua"))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
        .with_context(|| format!("aucun SavedVariables/Scout.lua sous {}", accounts.display()))
}

/// Looks for `<prefix>/drive_c/Program Files (x86)/World of Warcraft/_forever_` in usual Wine prefixes.
pub fn detect_wow_dir() -> Option<PathBuf> {
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
