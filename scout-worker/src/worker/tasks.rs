//! Long tasks, each on its own thread: Scout import, game data download.

use std::sync::Arc;

use anyhow::{Result, bail};
use scout_core::scout_api::house_for_faction;
use scout_core::{GAME_FORMAT, Row};
use scout_db::Db;
use scout_gamedata::Manifest;
use scout_sources as sources;
use scout_sources::{price_server, wow};

use super::refresh;
use crate::config::{self, Config, PriceServer};
use crate::message::Event;
use crate::reference::{self, ServerSync};

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

/// The "update game data" button: the game data, then the reference loot, which never fails the first.
pub(super) fn update_all(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    server: &PriceServer,
    send: &dyn Fn(Event),
) -> Result<()> {
    let game = update_game_data(runtime, db, server, send);
    update_server_reference(runtime, db, server, false, send);
    game
}

/// Loot and merchants of the price server, when newer; tells the UI to reload what they feed. A failure is
/// only reported when `quiet` is false (at start, the tool just keeps its database).
pub(super) fn update_server_reference(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    server: &PriceServer,
    quiet: bool,
    send: &dyn Fn(Event),
) {
    let unpacked = config::database_path().map(|p| p.with_extension("server-reference.tmp"));
    let synced = unpacked
        .and_then(|path| runtime.block_on(reference::sync_server(db, &server.url, &path, send)));
    let message = match synced {
        Ok(ServerSync::Installed) => {
            if let Err(e) = runtime.block_on(reload(db, server, send)) {
                send(Event::Error(format!("butin de référence : {e:#}")));
            }
            "Butin et marchands de référence installés."
        }
        Ok(ServerSync::UpToDate) => "Butin et marchands de référence déjà à jour.",
        Ok(ServerSync::Unavailable) if quiet => return,
        Ok(ServerSync::Unavailable) => {
            "Le serveur n'a pas encore de butin de référence pour cette version."
        }
        Err(_) if quiet => return,
        Err(e) => {
            return send(Event::Status(format!(
                "Butin et marchands de référence non vérifiés : {e:#}"
            )));
        }
    };
    if !quiet || message.ends_with("installés.") {
        send(Event::Status(message.into()));
    }
}

/// Merchants and the items (which clears the zones, loot and sheets the UI holds) after new loot.
async fn reload(db: &Db, server: &PriceServer, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Vendor(Arc::new(db.vendor_data().await?)));
    refresh(db, &server.auction_house(), send).await
}

/// What the price server offers for this tool's `GAME_FORMAT` (None: nothing built yet).
pub(super) fn latest_game_data(server: &PriceServer) -> Result<Option<Manifest>> {
    price_server::gamedata_latest(&server.url, GAME_FORMAT)
}

/// The auction house of a Scout scan: the settings' game mode on the side of the scan's faction;
/// without faction (older exports) or house chosen, the settings' house itself (none yet: given to
/// the first one chosen). The realm is not stored: the mode comes from the settings.
async fn store(db: &Db, config: &Config, mut rows: Vec<Row>, send: &dyn Fn(Event)) -> Result<()> {
    let house = config.price_server.auction_house();
    for row in &mut rows {
        row.region.clone_from(&house.region);
        row.house = row
            .faction
            .as_deref()
            .and_then(|f| house_for_faction(house.id, f))
            .map_or(i64::from(house.id), i64::from);
    }
    let added = db.insert(&rows).await?;
    refresh(db, &house, send).await?;
    send(Event::Status(format!(
        "{} relevés lus, {added} nouveaux enregistrés",
        rows.len()
    )));
    Ok(())
}
