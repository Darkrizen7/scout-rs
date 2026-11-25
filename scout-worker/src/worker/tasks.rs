//! Long tasks, each on its own thread: Scout import, game data download.

use std::sync::Arc;

use anyhow::{Result, bail};
use scout_core::{GAME_FORMAT, Row};
use scout_db::Db;
use scout_gamedata::Manifest;
use scout_sources as sources;
use scout_sources::{price_server, wow};

use super::refresh;
use crate::config::{Config, PriceServer};
use crate::message::Event;

pub(super) async fn import(db: &Db, config: &Config, send: &dyn Fn(Event)) -> Result<()> {
    let saved_variables = wow::saved_variables(config.wow_dir()?)?;
    send(Event::Status(format!(
        "Export de {}…",
        saved_variables.display()
    )));
    let script = wow::export_script(config.wow_dir()?)?;
    let rows = sources::run_scout_export(&script, &saved_variables)?;
    db.set_names(&sources::run_scout_names(&script, &saved_variables)?)
        .await?;
    let vendor = sources::run_vendor_export(&saved_variables)?;
    db.save_vendor_prices(&vendor).await?;
    send(Event::Vendor(Arc::new(db.vendor_data().await?)));
    match sources::run_scout_loot(&script, &saved_variables) {
        Ok(loot) => {
            db.save_observed_loot(&loot).await?;
        }
        Err(e) => send(Event::Error(format!(
            "butin enregistré par Scout (addon à jour ?) : {e:#}"
        ))),
    }
    store(db, config, rows, send).await
}

/// The newest game data package of our price server (which downloads wago.tools and Wowhead once
/// for everyone), stored when it is newer than ours.
pub(super) fn update_game_data(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    server: &PriceServer,
    send: &dyn Fn(Event),
) -> Result<()> {
    send(Event::Status(
        "Recherche des dernières données du jeu…".into(),
    ));
    let latest = latest_game_data(server);
    send(Event::LatestGameData(
        latest
            .as_ref()
            .map(Clone::clone)
            .map_err(|e| format!("{e:#}")),
    ));
    let Some(manifest) = latest? else {
        bail!("le serveur de prix n'a pas encore les données du jeu de cette version de l'outil");
    };
    let local = runtime.block_on(db.game_version())?;
    if !manifest.newer_than(local.as_ref().map(|(b, f, u)| (b.as_str(), *f, *u))) {
        send(Event::Status(format!(
            "Données du jeu déjà à jour (Forever {}).",
            manifest.build
        )));
        return Ok(());
    }
    send(Event::Status(format!(
        "Téléchargement des données du jeu (Forever {}, {} Ko)…",
        manifest.build,
        manifest.bytes / 1000
    )));
    let game =
        price_server::gamedata_package(&server.url, &manifest)?.into_game_data(GAME_FORMAT)?;
    runtime.block_on(db.save_game(&game))?;
    send(Event::Status(format!(
        "Données du jeu : Forever {}, {} recettes, {} objets.",
        game.build,
        game.recipes.len(),
        game.items.len()
    )));
    send(Event::GameData(Arc::new(game)));
    Ok(())
}

/// What the price server offers for this tool's `GAME_FORMAT` (None: nothing built yet).
pub(super) fn latest_game_data(server: &PriceServer) -> Result<Option<Manifest>> {
    price_server::gamedata_latest(&server.url, GAME_FORMAT)
}

/// Scout does not say which auction house it scanned: the one chosen in the settings
/// (none yet: given to the first one chosen).
async fn store(db: &Db, config: &Config, mut rows: Vec<Row>, send: &dyn Fn(Event)) -> Result<()> {
    let house = config.price_server.auction_house();
    for row in &mut rows {
        row.region.clone_from(&house.region);
        row.house = i64::from(house.id);
    }
    let added = db.insert(&rows).await?;
    refresh(db, &house, send).await?;
    send(Event::Status(format!(
        "{} relevés lus, {added} nouveaux enregistrés",
        rows.len()
    )));
    Ok(())
}
