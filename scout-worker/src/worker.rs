//! The worker loop: the only place that touches disk, database, network and processes.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use anyhow::Result;
use scout_core::analysis;
use scout_core::gamedata::GameData;
use scout_core::{GAME_FORMAT, Row};
use scout_db::Db;
use scout_sources as sources;
use scout_sources::wow;

use crate::config::{self, Config};
use crate::message::{Command, Event};

pub(crate) fn run(commands: Receiver<Command>, send: impl Fn(Event)) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return send(Event::Error(format!("runtime tokio : {e}"))),
    };
    // The old folders are moved, never recreated empty: on failure, stop rather than start a new database.
    if let Err(e) = config::migrate_old_folders() {
        return send(Event::Error(format!("migration de scout-prix : {e:#}")));
    }
    let mut config = Config::load();
    send(Event::Config(config.clone()));

    let db = match runtime.block_on(open_db()) {
        Ok(db) => db,
        Err(e) => return send(Event::Error(format!("base de données : {e:#}"))),
    };
    match runtime.block_on(db.load_game()) {
        Ok(game) if !game.is_empty() => send(Event::GameData(Arc::new(game))),
        Ok(_) => {}
        Err(e) => send(Event::Error(format!("données du jeu : {e:#}"))),
    }
    match runtime.block_on(db.vendor_data()) {
        Ok(prices) => send(Event::Vendor(Arc::new(prices))),
        Err(e) => send(Event::Error(format!("prix marchands : {e:#}"))),
    }
    if let Err(e) = runtime.block_on(refresh(&db, &send)) {
        send(Event::Error(format!("{e:#}")));
    }

    // Is the stored game data the newest Forever build? (network; the UI is already usable)
    send(Event::LatestBuild(
        sources::latest_build().map_err(|e| format!("{e:#}")),
    ));

    for command in commands {
        send(Event::Busy(true));
        let result = match command {
            Command::Import => runtime.block_on(import(&db, &config, &send)),
            Command::ImportLegacy => runtime.block_on(import_legacy(&db, &config, &send)),
            Command::SetFavorite(id, favorite) => runtime.block_on(async {
                db.set_favorite(id, favorite).await?;
                send(Event::Favorites(db.favorites().await?));
                Ok(())
            }),
            Command::UpdateGameData => update_game_data(&runtime, &db, &send),
            Command::Analyze => runtime.block_on(db.observations()).map(|observations| {
                send(Event::Analysis(Box::new(analysis::analyze(&observations))));
            }),
            Command::LoadHistory(id) => runtime
                .block_on(db.history(id))
                .map(|h| send(Event::History(id, h))),
            Command::LoadDrops(id) => runtime
                .block_on(db.item_drops(id))
                .map(|d| send(Event::Drops(id, Box::new(d)))),
            Command::LoadZones => runtime.block_on(db.zones()).map(|z| send(Event::Zones(z))),
            Command::LoadZone(zone) => runtime
                .block_on(db.zone_loot(&zone))
                .map(|z| send(Event::ZoneLoot(Box::new(z)))),
            Command::SetWowDir(dir) => {
                config.wow_dir = Some(dir);
                send(Event::Config(config.clone()));
                config.save()
            }
            Command::SetTheme(theme) => {
                config.theme = theme;
                config.save()
            }
        };
        if let Err(e) = result {
            send(Event::Error(format!("{e:#}")));
        }
        send(Event::Busy(false));
    }
}

async fn open_db() -> Result<Db> {
    Db::open(&config::database_path()?).await
}

async fn refresh(db: &Db, send: &impl Fn(Event)) -> Result<()> {
    send(Event::Items(db.items().await?));
    send(Event::Favorites(db.favorites().await?));
    Ok(())
}

async fn import(db: &Db, config: &Config, send: &impl Fn(Event)) -> Result<()> {
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
async fn import_drops(db: &Db, config: &Config, send: &impl Fn(Event)) -> Result<()> {
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

/// Newest Forever build → 9 client tables → recipes, stored with the build they come from.
fn update_game_data(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    send: &impl Fn(Event),
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
    let tables = sources::assemble(&tables)?;
    let now = chrono::Utc::now().timestamp();
    let game = GameData::new(build.clone(), now, tables);
    runtime.block_on(db.save_game(&game))?;
    send(Event::Status(format!(
        "Données du jeu : Forever {build}, {} recettes, {} objets.",
        game.recipes.len(),
        game.items.len()
    )));
    send(Event::GameData(Arc::new(game)));
    Ok(())
}

async fn import_legacy(db: &Db, config: &Config, send: &impl Fn(Event)) -> Result<()> {
    let saved_variables = wow::saved_variables(config.wow_dir()?)?;
    let dir = saved_variables.parent().unwrap_or(&saved_variables);
    send(Event::Status(format!(
        "Lecture d'Auctionator et Forever Guide dans {}…",
        dir.display()
    )));
    let rows = sources::run_legacy_export(dir)?;
    store(db, &rows, send).await
}

async fn store(db: &Db, rows: &[Row], send: &impl Fn(Event)) -> Result<()> {
    let added = db.insert(rows).await?;
    refresh(db, send).await?;
    send(Event::Status(format!(
        "{} relevés lus, {added} nouveaux enregistrés",
        rows.len()
    )));
    Ok(())
}
