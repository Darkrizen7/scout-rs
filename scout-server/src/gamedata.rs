//! The game data every app needs (wago.tools client tables + Wowhead), downloaded here once per
//! Forever build and served as a versioned package (`scout_gamedata::Package`).
//!
//! Layout under `data/gamedata/`: `wowhead.json` (items Wowhead named, kept across builds: each is
//! asked for once), and per `GAME_FORMAT` a folder `<format>/` with the newest package
//! `<updated>.json.gz` and its `latest.json` (`Manifest`). Older packages are removed: they can
//! always be built again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use scout_core::GAME_FORMAT;
use scout_core::gamedata::GameItem;
use scout_gamedata::{Manifest, Package};

use crate::collector::unix_now;

pub struct GameStore {
    dir: PathBuf,
}

impl GameStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("gamedata");
        std::fs::create_dir_all(dir.join(GAME_FORMAT.to_string()))
            .with_context(|| format!("dossier {}", dir.display()))?;
        Ok(Self { dir })
    }

    fn format_dir(&self, format: i64) -> PathBuf {
        self.dir.join(format.to_string())
    }

    /// The newest package of `format`, if one was built.
    pub fn latest(&self, format: i64) -> Option<Manifest> {
        let text = std::fs::read(self.format_dir(format).join("latest.json")).ok()?;
        serde_json::from_slice(&text).ok()
    }

    /// Path of a package, when it is the newest of its format (the only one kept).
    pub fn package_path(&self, format: i64, updated: i64) -> Option<PathBuf> {
        let latest = self.latest(format)?;
        (latest.updated == updated).then(|| self.format_dir(format).join(file_name(updated)))
    }

    fn wowhead(&self) -> HashMap<i64, GameItem> {
        let path = self.dir.join("wowhead.json");
        let items: Vec<GameItem> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        items.into_iter().map(|i| (i.id, i)).collect()
    }

    fn save_wowhead(&self, items: &HashMap<i64, GameItem>) -> Result<()> {
        let mut list: Vec<&GameItem> = items.values().collect();
        list.sort_by_key(|i| i.id);
        write_atomic(&self.dir.join("wowhead.json"), &serde_json::to_vec(&list)?)
    }

    /// Builds a new package when Forever has a new build, when this format has none yet, or when
    /// the last one is incomplete (Wowhead refused). Blocking: network, a pause per Wowhead item.
    pub fn refresh(&self) -> Result<Option<Manifest>> {
        let build = scout_gamedata::latest_build()?;
        let latest = self.latest(GAME_FORMAT);
        if latest
            .as_ref()
            .is_some_and(|m| m.build == build && m.complete)
        {
            return Ok(None);
        }
        eprintln!("données du jeu : téléchargement de Forever {build}");
        let mut known = self.wowhead();
        let download = scout_gamedata::download(&build, &mut known, &|_| {});
        // Keep what Wowhead answered even when the download failed further on.
        self.save_wowhead(&known)?;
        let download = download?;
        let package = Package {
            format: GAME_FORMAT,
            build,
            updated: unix_now(),
            complete: download.blocked.is_none(),
            tables: download.tables,
        };
        let bytes = package.encode()?;
        let dir = self.format_dir(GAME_FORMAT);
        write_atomic(&dir.join(file_name(package.updated)), &bytes)?;
        let manifest = Manifest {
            format: package.format,
            build: package.build,
            updated: package.updated,
            complete: package.complete,
            bytes: bytes.len() as u64,
        };
        write_atomic(&dir.join("latest.json"), &serde_json::to_vec(&manifest)?)?;
        if let Some(old) = latest {
            let _ = std::fs::remove_file(dir.join(file_name(old.updated)));
        }
        if let Some(reason) = download.blocked {
            eprintln!("données du jeu incomplètes ({reason}) : nouvel essai au prochain tour");
        }
        Ok(Some(manifest))
    }

    /// Checks for a new build at start, then every `interval`.
    pub async fn run(self: std::sync::Arc<Self>, interval: Duration) {
        loop {
            let store = self.clone();
            match tokio::task::spawn_blocking(move || store.refresh()).await {
                Ok(Ok(Some(m))) => eprintln!(
                    "données du jeu : Forever {} prêtes ({} Ko)",
                    m.build,
                    m.bytes / 1000
                ),
                Ok(Ok(None)) => {}
                Ok(Err(e)) => eprintln!("données du jeu : {e:#}"),
                Err(e) => eprintln!("données du jeu : {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    }
}

fn file_name(updated: i64) -> String {
    format!("{updated}.json.gz")
}

/// Written beside, then renamed: a reader never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("écriture de {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("écriture de {}", path.display()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use scout_core::gamedata::Tables;

    /// A store holding one package of the current format, built at `updated`.
    pub fn store_with_package(dir: &Path, updated: i64) -> GameStore {
        let store = GameStore::new(dir).unwrap();
        let package = Package {
            format: GAME_FORMAT,
            build: "1.60.1.71037".into(),
            updated,
            complete: true,
            tables: Tables::default(),
        };
        let bytes = package.encode().unwrap();
        let folder = store.format_dir(GAME_FORMAT);
        write_atomic(&folder.join(file_name(updated)), &bytes).unwrap();
        let manifest = Manifest {
            format: GAME_FORMAT,
            build: package.build,
            updated,
            complete: true,
            bytes: bytes.len() as u64,
        };
        write_atomic(
            &folder.join("latest.json"),
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        store
    }

    #[test]
    fn only_the_newest_package_is_served() {
        let dir = std::env::temp_dir().join(format!("scout-gamedata-{}", std::process::id()));
        let store = store_with_package(&dir, 1000);
        assert_eq!(store.latest(GAME_FORMAT).unwrap().updated, 1000);
        assert!(store.package_path(GAME_FORMAT, 1000).unwrap().exists());
        assert!(store.package_path(GAME_FORMAT, 999).is_none());
        assert!(store.latest(GAME_FORMAT + 1).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
