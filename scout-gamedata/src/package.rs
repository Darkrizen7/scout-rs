//! The game data package: everything a download gives, as gzipped JSON, built by `scout-server`
//! once per Forever build and served to every app. Its version is the moment it was built
//! (`updated`), for one `GAME_FORMAT`: an app only takes a package of its own format.

use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use scout_core::gamedata::{GameData, Tables};
use serde::{Deserialize, Serialize};

/// A package is a few megabytes once unpacked; anything far larger is not one.
const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

/// What `/v1/gamedata/latest` says about the newest package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// `GAME_FORMAT` of the server that built it.
    pub format: i64,
    /// Forever client build, e.g. `1.60.1.71037`.
    pub build: String,
    /// When it was built (Unix seconds): the package's version.
    pub updated: i64,
    /// False when Wowhead refused some items: the server builds it again later.
    pub complete: bool,
    /// Size of the gzipped package.
    pub bytes: u64,
}

impl Manifest {
    /// Whether an app holding `local` (build, format, updated) should download this package.
    pub fn newer_than(&self, local: Option<(&str, i64, i64)>) -> bool {
        match local {
            None => true,
            Some((build, format, updated)) => {
                build != self.build || format != self.format || updated < self.updated
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Package {
    pub format: i64,
    pub build: String,
    pub updated: i64,
    pub complete: bool,
    pub tables: Tables,
}

impl Package {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::best());
        serde_json::to_writer(&mut gz, self)?;
        gz.flush()?;
        Ok(gz.finish()?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let reader = GzDecoder::new(bytes).take(MAX_JSON_BYTES);
        serde_json::from_reader(reader).context("paquet des données du jeu illisible")
    }

    /// The game data the app keeps, refusing a package of another format.
    pub fn into_game_data(self, format: i64) -> Result<GameData> {
        if self.format != format {
            bail!(
                "données du jeu au format {} (l'outil attend le {format}) : mets à jour l'outil",
                self.format
            );
        }
        let mut game = GameData::new(self.build, self.updated, self.tables);
        game.complete = self.complete;
        Ok(game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scout_core::GAME_FORMAT;
    use scout_core::gamedata::GameItem;

    #[test]
    fn round_trip() {
        let mut tables = Tables::default();
        tables.items.push(GameItem {
            id: 2589,
            name: "Étoffe de lin".into(),
            quality: 1,
            item_level: 5,
            required_level: 0,
            class_id: Some(7),
            subclass_id: Some(5),
            buy_price: 0,
            vendor_stack: 1,
            sell_price: 13,
        });
        tables.unnamed_categories.insert(5, (7, 5));
        let package = Package {
            format: GAME_FORMAT,
            build: "1.60.1.71037".into(),
            updated: 1_700_000_000,
            complete: false,
            tables,
        };
        let game = Package::decode(&package.encode().unwrap())
            .unwrap()
            .into_game_data(GAME_FORMAT)
            .unwrap();
        assert_eq!(game.items[&2589].name, "Étoffe de lin");
        assert_eq!((game.updated, game.complete), (1_700_000_000, false));
    }

    #[test]
    fn refuses_another_format() {
        let package = Package {
            format: GAME_FORMAT + 1,
            build: String::new(),
            updated: 0,
            complete: true,
            tables: Tables::default(),
        };
        assert!(package.into_game_data(GAME_FORMAT).is_err());
    }

    #[test]
    fn newer_package() {
        let manifest = Manifest {
            format: 6,
            build: "1.2".into(),
            updated: 100,
            complete: true,
            bytes: 0,
        };
        assert!(manifest.newer_than(None));
        assert!(!manifest.newer_than(Some(("1.2", 6, 100))));
        assert!(!manifest.newer_than(Some(("1.2", 6, 200))));
        assert!(manifest.newer_than(Some(("1.2", 6, 50))));
        assert!(manifest.newer_than(Some(("1.1", 6, 200))));
        assert!(manifest.newer_than(Some(("1.2", 5, 200))));
    }
}
