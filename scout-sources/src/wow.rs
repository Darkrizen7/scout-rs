//! Where things are inside the WoW Forever folder (`_forever_`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

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

/// OpenMap's zone shapes (`MapTerritory` masks), if the addon is installed.
pub fn open_map_masks(wow_dir: &Path) -> Option<PathBuf> {
    let dir = wow_dir.join("Interface/AddOns/OpenMap/Media/Textures/Forever/MapTerritory");
    dir.is_dir().then_some(dir)
}

/// Forever Guide's folder, when that addon is installed (its item sources are the reference for drops).
pub fn forever_guide(wow_dir: &Path) -> Option<PathBuf> {
    let path = wow_dir.join("Interface/AddOns/ForeverGuide");
    path.join("ItemSearchData.lua").is_file().then_some(path)
}

/// Files the drops export reads from Forever Guide.
const GUIDE_DROP_FILES: [&str; 4] = [
    "ItemSearchData.lua",
    "DungeonLoot.lua",
    "DataFrFR.lua",
    "Locale.lua",
];

/// Size and modification time of the files the drops export reads, plus `format` (bumped when the export
/// changes): the drops are imported again only when this changes.
pub fn guide_fingerprint(guide: &Path, format: u32) -> String {
    let mut text = format!("v{format}");
    for name in GUIDE_DROP_FILES {
        let meta = fs::metadata(guide.join(name)).ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        text += &format!(";{name}:{}:{modified}", meta.map_or(0, |m| m.len()));
    }
    text
}

/// The most recently written `Scout.lua` among the accounts' SavedVariables.
pub fn saved_variables(wow_dir: &Path) -> Result<PathBuf> {
    let accounts = wow_dir.join("WTF/Account");
    let entries = fs::read_dir(&accounts).with_context(|| {
        format!(
            "aucun compte WoW dans {} : est-ce bien le dossier _forever_ ? (sinon lance le jeu une fois)",
            wow_dir.display()
        )
    })?;
    entries
        .flatten()
        .map(|e| e.path().join("SavedVariables/Scout.lua"))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
        .with_context(|| {
            format!(
                "Scout n'a encore rien enregistré ({} absent) : en jeu, addon Scout activé, fais un /reload",
                accounts.join("*/SavedVariables/Scout.lua").display()
            )
        })
}

/// Where WoW is usually installed, relative to a drive (Windows) or a Wine prefix's `drive_c`.
const INSTALL_DIRS: [&str; 4] = [
    "Program Files (x86)/World of Warcraft/_forever_",
    "Program Files/World of Warcraft/_forever_",
    "World of Warcraft/_forever_",
    "Games/World of Warcraft/_forever_",
];

/// The first usual install folder that holds accounts: on Windows, on every drive; elsewhere, in
/// usual Wine prefixes (`~/.wine`, `~/Games/*`, Lutris).
pub fn detect_wow_dir() -> Option<PathBuf> {
    roots()
        .into_iter()
        .flat_map(|root| INSTALL_DIRS.map(|dir| root.join(dir)))
        .find(|p| p.join("WTF/Account").is_dir())
}

#[cfg(windows)]
fn roots() -> Vec<PathBuf> {
    (b'C'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|drive| drive.is_dir())
        .collect()
}

#[cfg(not(windows))]
fn roots() -> Vec<PathBuf> {
    let Some(dirs) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let home = dirs.home_dir();
    let mut prefixes = vec![home.join(".wine")];
    for parent in ["Games", ".local/share/lutris/prefixes"] {
        if let Ok(entries) = fs::read_dir(home.join(parent)) {
            prefixes.extend(entries.flatten().map(|e| e.path()));
        }
    }
    prefixes.into_iter().map(|p| p.join("drive_c")).collect()
}
