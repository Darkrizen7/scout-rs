//! The game data every app needs (wago.tools client tables + Wowhead), downloaded here once per
//! Forever build and served as a versioned package (`scout_gamedata::Package`).
//!
//! Besides the recipe items, it names the items seen in the auction snapshots that neither the
//! client tables nor the built-in reference package name (`extra`).
//!
//! Layout under `data/gamedata/`: `wowhead.json` (items Wowhead named, kept across builds: each is
//! asked for once), `wowhead_unknown.json` (ids it does not know: not asked again), and per
//! `GAME_FORMAT` a folder `<format>/` with the newest package `<updated>.json.gz` and its
//! `latest.json` (`Manifest`). Older packages are removed: they can always be built again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use scout_core::GAME_FORMAT;
use scout_core::gamedata::GameItem;
use scout_gamedata::{Manifest, Naming, Package};

use crate::api::reference::named_items;
use crate::collector::unix_now;
use crate::db::Db;

/// Wowhead requests per round: the CDN blocked the address after ~270 in a row.
const ROUND_LIMIT: usize = 150;
/// Next round after the limit was reached: the rest is only a matter of spacing requests out.
const AFTER_LIMIT: Duration = Duration::from_secs(15 * 60);
/// Next round after a refusal (403…): let the block lift before asking again.
const AFTER_BLOCK: Duration = Duration::from_secs(60 * 60);

/// What is left to do once a round ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    Nothing,
    /// The round limit was reached, items remain.
    Limited,
    /// Wowhead refused the requests.
    Blocked,
}

impl Left {
    fn of(naming: &Naming) -> Self {
        if naming.blocked.is_some() {
            Self::Blocked
        } else if naming.limited {
            Self::Limited
        } else {
            Self::Nothing
        }
    }

    /// Wait before the next round: sooner while the package is incomplete.
    fn next_delay(self, interval: Duration) -> Duration {
        match self {
            Self::Nothing => interval,
            Self::Limited => AFTER_LIMIT.min(interval),
            Self::Blocked => AFTER_BLOCK.min(interval),
        }
    }
}

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

    fn unknown(&self) -> HashSet<i64> {
        let ids: Vec<i64> = std::fs::read(self.dir.join("wowhead_unknown.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        ids.into_iter().collect()
    }

    fn save_unknown(&self, ids: &HashSet<i64>) -> Result<()> {
        let mut list: Vec<i64> = ids.iter().copied().collect();
        list.sort_unstable();
        write_atomic(
            &self.dir.join("wowhead_unknown.json"),
            &serde_json::to_vec(&list)?,
        )
    }

    /// The newest package, decoded.
    fn read_package(&self, manifest: &Manifest) -> Result<Package> {
        let path = self
            .format_dir(manifest.format)
            .join(file_name(manifest.updated));
        Package::decode(&std::fs::read(&path).with_context(|| format!("{}", path.display()))?)
    }

    /// Writes `package` and its manifest, then removes the package it replaces.
    fn publish(&self, package: &Package, old: Option<&Manifest>) -> Result<Manifest> {
        let bytes = package.encode()?;
        let dir = self.format_dir(GAME_FORMAT);
        write_atomic(&dir.join(file_name(package.updated)), &bytes)?;
        let manifest = Manifest {
            format: package.format,
            build: package.build.clone(),
            updated: package.updated,
            complete: package.complete,
            bytes: bytes.len() as u64,
        };
        write_atomic(&dir.join("latest.json"), &serde_json::to_vec(&manifest)?)?;
        if let Some(old) = old.filter(|o| o.updated != package.updated) {
            let _ = std::fs::remove_file(dir.join(file_name(old.updated)));
        }
        Ok(manifest)
    }

    /// Builds a new package when Forever has a new build or this format has none yet (full
    /// download). Otherwise, when the last package is incomplete (Wowhead refused) or lacks items
    /// (recipe items, `extra`) that Wowhead can still name, names them in that package: no new
    /// wago.tools download. `extra`: items seen in the auction snapshots that nothing else names.
    /// At most `ROUND_LIMIT` Wowhead requests per call; the returned `Left` says whether another
    /// round is needed. Blocking: network, a pause per Wowhead item.
    pub fn refresh(&self, extra: &[i64]) -> Result<(Option<Manifest>, Left)> {
        let build = scout_gamedata::latest_build()?;
        let latest = self.latest(GAME_FORMAT);
        let mut known = self.wowhead();
        let mut unknown = self.unknown();
        // The package of the same build, when it can be completed in place.
        let current = latest
            .as_ref()
            .filter(|m| m.build == build)
            .and_then(|m| self.read_package(m).ok());
        let (package, left) = match current {
            Some(mut package) => {
                let wanted = scout_gamedata::wanted_items(&package.tables, extra, &unknown);
                if package.complete && wanted.is_empty() {
                    return Ok((None, Left::Nothing));
                }
                eprintln!(
                    "données du jeu : {} objets à nommer par Wowhead",
                    wanted.len()
                );
                let naming = scout_gamedata::name_items(
                    &mut package.tables,
                    extra,
                    &mut known,
                    &mut unknown,
                    Some(ROUND_LIMIT),
                    &|_| {},
                );
                self.save_caches(&known, &unknown)?;
                package.updated = unix_now();
                package.complete = naming.complete();
                report(&naming);
                (package, Left::of(&naming))
            }
            None => {
                eprintln!("données du jeu : téléchargement de Forever {build}");
                let download = scout_gamedata::download(
                    &build,
                    extra,
                    &mut known,
                    &mut unknown,
                    Some(ROUND_LIMIT),
                    &|_| {},
                );
                // Keep what Wowhead answered even when the download failed further on.
                self.save_caches(&known, &unknown)?;
                let download = download?;
                let naming = Naming {
                    blocked: download.blocked,
                    limited: download.limited,
                };
                report(&naming);
                let package = Package {
                    format: GAME_FORMAT,
                    build,
                    updated: unix_now(),
                    complete: naming.complete(),
                    tables: download.tables,
                };
                (package, Left::of(&naming))
            }
        };
        let manifest = self.publish(&package, latest.as_ref())?;
        Ok((Some(manifest), left))
    }

    fn save_caches(&self, known: &HashMap<i64, GameItem>, unknown: &HashSet<i64>) -> Result<()> {
        self.save_wowhead(known)?;
        self.save_unknown(unknown)
    }

    /// Checks for a new build at start, then every `interval`. `dbs`: the price databases, whose
    /// items get named too.
    pub async fn run(self: std::sync::Arc<Self>, dbs: Vec<Db>, interval: Duration) {
        let named = named_items().await.unwrap_or_else(|e| {
            eprintln!("données du jeu : paquet de référence : {e:#}");
            HashSet::new()
        });
        loop {
            let extra = match unnamed_priced(&dbs, &named).await {
                Ok(extra) => extra,
                Err(e) => {
                    eprintln!("données du jeu : objets de l'HdV : {e:#}");
                    Vec::new()
                }
            };
            let store = self.clone();
            let mut left = Left::Nothing;
            match tokio::task::spawn_blocking(move || store.refresh(&extra)).await {
                Ok(Ok((manifest, rest))) => {
                    left = rest;
                    if let Some(m) = manifest {
                        eprintln!(
                            "données du jeu : Forever {} prêtes ({} Ko)",
                            m.build,
                            m.bytes / 1000
                        );
                    }
                }
                Ok(Err(e)) => eprintln!("données du jeu : {e:#}"),
                Err(e) => eprintln!("données du jeu : {e}"),
            }
            tokio::time::sleep(left.next_delay(interval)).await;
        }
    }
}

fn report(naming: &Naming) {
    if let Some(reason) = &naming.blocked {
        eprintln!("données du jeu incomplètes ({reason}) : nouvel essai dans une heure");
    } else if naming.limited {
        eprintln!(
            "données du jeu incomplètes (limite de {ROUND_LIMIT} objets par tour) : suite dans 15 min"
        );
    }
}

/// Items seen at the auction house of any region that the reference package does not name.
async fn unnamed_priced(dbs: &[Db], named: &HashSet<i64>) -> Result<Vec<i64>> {
    let mut ids = HashSet::new();
    for db in dbs {
        ids.extend(db.priced_items().await?);
    }
    let mut ids: Vec<i64> = ids.difference(named).copied().collect();
    ids.sort_unstable();
    Ok(ids)
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
    fn incomplete_packages_come_back_sooner() {
        let normal = Duration::from_secs(6 * 3600);
        assert_eq!(Left::Nothing.next_delay(normal), normal);
        assert_eq!(Left::Limited.next_delay(normal), AFTER_LIMIT);
        assert_eq!(Left::Blocked.next_delay(normal), AFTER_BLOCK);
        // Never later than the normal interval.
        let short = Duration::from_secs(60);
        assert_eq!(Left::Blocked.next_delay(short), short);
    }

    #[tokio::test]
    async fn priced_items_already_named_are_left_out() {
        let db = Db::memory().await.unwrap();
        let named = HashSet::from([1]);
        assert!(unnamed_priced(&[db], &named).await.unwrap().is_empty());
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
