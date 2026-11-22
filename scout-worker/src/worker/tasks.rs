//! Long tasks, each on its own thread: Scout import, older prices, game data download.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use scout_core::gamedata::{GameData, Tables};
use scout_core::{GAME_FORMAT, Row};
use scout_db::Db;
use scout_sources as sources;
use scout_sources::wow;

use super::refresh;
use crate::config::Config;
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
    let vendor = sources::run_vendor_export(
        &saved_variables,
        wow::guide_vendor_prices(config.wow_dir()?).as_deref(),
    )?;
    db.save_vendor_prices(&vendor).await?;
    send(Event::Vendor(Arc::new(db.vendor_data().await?)));
    // Drops are a bonus: a failure there must not stop the price import.
    if let Err(e) = import_drops(db, config, send).await {
        send(Event::Error(format!("butin de Forever Guide : {e:#}")));
    }
    match sources::run_scout_loot(&script, &saved_variables) {
        Ok(loot) => {
            db.save_observed_loot(&loot).await?;
        }
        Err(e) => send(Event::Error(format!(
            "butin enregistré par Scout (addon à jour ?) : {e:#}"
        ))),
    }
    store(db, &rows, send).await
}

/// Forever Guide's item sources, imported again only when its files (or our export) changed.
async fn import_drops(db: &Db, config: &Config, send: &dyn Fn(Event)) -> Result<()> {
    let Some(guide) = wow::forever_guide(config.wow_dir()?) else {
        return Ok(());
    };
    let fingerprint = wow::guide_fingerprint(&guide, sources::DROPS_FORMAT);
    if db.drops_fingerprint().await?.as_deref() == Some(fingerprint.as_str()) {
        return Ok(());
    }
    send(Event::Status("Lecture du butin de Forever Guide…".into()));
    let export = sources::run_drops_export(&guide)?;
    let count = db.save_drops(&export, &fingerprint).await?;
    send(Event::Status(format!(
        "Butin de Forever Guide : {count} sources"
    )));
    Ok(())
}

/// Newest Forever build → client tables (+ Wowhead for the items they lack) → recipes, stored
/// with the build they come from.
pub(super) fn update_game_data(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    send: &dyn Fn(Event),
) -> Result<()> {
    send(Event::Status(
        "Recherche de la dernière version de WoW Forever…".into(),
    ));
    let build = sources::latest_build()?;
    send(Event::LatestBuild(Ok(build.clone())));
    if runtime.block_on(db.game_version())? == Some((build.clone(), GAME_FORMAT)) {
        send(Event::Status(format!(
            "Données du jeu déjà à jour (Forever {build})."
        )));
        return Ok(());
    }
    let mut tables = HashMap::new();
    for (i, table) in sources::TABLES.iter().enumerate() {
        send(Event::Status(format!(
            "Forever {build} : téléchargement {}/{} ({table})…",
            i + 1,
            sources::TABLES.len()
        )));
        tables.insert(*table, sources::table_csv(table, &build)?);
    }
    send(Event::Status("Assemblage des recettes…".into()));
    let mut tables = sources::assemble(&tables)?;
    let blocked = add_wowhead_items(runtime, db, &mut tables, send)?;
    let now = chrono::Utc::now().timestamp();
    let mut game = GameData::new(build.clone(), now, tables);
    if blocked.is_some() {
        // Stored as incomplete: the next update runs again instead of finding the data up to date.
        game.format = GAME_FORMAT - 1;
    }
    runtime.block_on(db.save_game(&game))?;
    send(Event::Status(format!(
        "Données du jeu : Forever {build}, {} recettes, {} objets.",
        game.recipes.len(),
        game.items.len()
    )));
    send(Event::GameData(Arc::new(game)));
    if let Some(reason) = blocked {
        send(Event::Error(format!(
            "{reason}. Mise à jour incomplète (le reste est enregistré) : réessaie plus tard."
        )));
    }
    Ok(())
}

/// Recipe items missing from the client tables (sent by Forever's server as hotfixes), named by
/// Wowhead. Each item is asked for once: the answers are kept across updates. Stops at the first refusal
/// (403…) and returns why, so the update can say to try again later.
fn add_wowhead_items(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    tables: &mut Tables,
    send: &dyn Fn(Event),
) -> Result<Option<String>> {
    let mut blocked = None;
    let missing = tables.missing_recipe_items();
    let mut known = runtime.block_on(db.wowhead_items())?;
    let unknown: Vec<i64> = missing
        .iter()
        .copied()
        .filter(|id| !known.contains_key(id))
        .collect();
    for (i, &id) in unknown.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(sources::wowhead::PAUSE);
        }
        send(Event::Status(format!(
            "Objets absents des tables du jeu, depuis Wowhead : {}/{}…",
            i + 1,
            unknown.len()
        )));
        // Wowhead only names a few extra items: when it refuses (403…), stop asking and keep going
        // with the client tables rather than losing the whole update.
        match sources::wowhead::item(id) {
            Ok(Some(item)) => {
                runtime.block_on(db.save_wowhead_item(&item))?;
                known.insert(id, item);
            }
            Ok(None) => {}
            Err(error) => {
                blocked = Some(format!("{error:#}"));
                break;
            }
        }
    }
    tables.add_items(missing.iter().filter_map(|id| known.remove(id)));
    Ok(blocked)
}

pub(super) async fn import_legacy(db: &Db, config: &Config, send: &dyn Fn(Event)) -> Result<()> {
    let saved_variables = wow::saved_variables(config.wow_dir()?)?;
    let dir = saved_variables.parent().unwrap_or(&saved_variables);
    send(Event::Status(format!(
        "Lecture d'Auctionator et Forever Guide dans {}…",
        dir.display()
    )));
    let rows = sources::run_legacy_export(dir)?;
    store(db, &rows, send).await
}

async fn store(db: &Db, rows: &[Row], send: &dyn Fn(Event)) -> Result<()> {
    let added = db.insert(rows).await?;
    refresh(db, send).await?;
    send(Event::Status(format!(
        "{} relevés lus, {added} nouveaux enregistrés",
        rows.len()
    )));
    Ok(())
}
