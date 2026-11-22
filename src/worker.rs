//! Background thread: export, database and settings. The UI only sends commands and reads events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use anyhow::Result;
use eframe::egui;

use crate::analysis::{self, Analysis};
use crate::config::{self, Config, Theme};
use crate::db::{Db, ItemSummary, Snapshot, VendorData};
use crate::gamedata::{self, GameData};
use crate::import;

pub enum Command {
    Import,
    /// One-off import of the prices Auctionator and Forever Guide kept.
    ImportLegacy,
    LoadHistory(i64),
    Analyze,
    SetFavorite(i64, bool),
    /// Downloads the newest Forever client tables (unless already stored).
    UpdateGameData,
    SetWowDir(PathBuf),
    SetTheme(Theme),
}

pub enum Event {
    Config(Config),
    Busy(bool),
    Status(String),
    Error(String),
    Items(Vec<ItemSummary>),
    History(i64, Vec<Snapshot>),
    Analysis(Box<Analysis>),
    Favorites(Vec<i64>),
    GameData(Arc<GameData>),
    /// Newest Forever build on wago.tools, or why it could not be checked.
    LatestBuild(Result<String, String>),
    /// Merchant prices and which NPCs sell what.
    Vendor(Arc<VendorData>),
}

pub struct WorkerHandle {
    commands: Sender<Command>,
    events: Receiver<Event>,
}

impl WorkerHandle {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let send = move |event| {
            // The UI is gone when this fails: nothing left to do.
            let _ = event_tx.send(event);
            ctx.request_repaint();
        };
        thread::Builder::new()
            .name("worker".into())
            .spawn(move || run(command_rx, send))
            .expect("impossible de lancer le thread de fond");
        Self { commands, events }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn poll(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.try_iter()
    }
}

fn run(commands: Receiver<Command>, send: impl Fn(Event)) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return send(Event::Error(format!("runtime tokio : {e}"))),
    };
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
        gamedata::latest_build().map_err(|e| format!("{e:#}")),
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
    let saved_variables = config.saved_variables()?;
    send(Event::Status(format!(
        "Export de {}…",
        saved_variables.display()
    )));
    let script = config.export_script()?;
    let rows = import::run_scout_export(&script, &saved_variables)?;
    db.set_names(&import::run_scout_names(&script, &saved_variables)?)
        .await?;
    let vendor =
        import::run_vendor_export(&saved_variables, config.guide_vendor_prices().as_deref())?;
    db.save_vendor_prices(&vendor).await?;
    send(Event::Vendor(Arc::new(db.vendor_data().await?)));
    store(db, &rows, send).await
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
    let build = gamedata::latest_build()?;
    send(Event::LatestBuild(Ok(build.clone())));
    if runtime.block_on(db.game_version())? == Some((build.clone(), crate::db::GAME_FORMAT)) {
        send(Event::Status(format!(
            "Données du jeu déjà à jour (Forever {build})."
        )));
        return Ok(());
    }
    let mut tables = HashMap::new();
    for (i, table) in gamedata::TABLES.iter().enumerate() {
        send(Event::Status(format!(
            "Forever {build} : téléchargement {}/{} ({table})…",
            i + 1,
            gamedata::TABLES.len()
        )));
        tables.insert(*table, gamedata::table_csv(table, &build)?);
    }
    send(Event::Status("Assemblage des recettes…".into()));
    let tables = gamedata::assemble(&tables)?;
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
    let saved_variables = config.saved_variables()?;
    let dir = saved_variables.parent().unwrap_or(&saved_variables);
    send(Event::Status(format!(
        "Lecture d'Auctionator et Forever Guide dans {}…",
        dir.display()
    )));
    let rows = import::run_legacy_export(dir)?;
    store(db, &rows, send).await
}

async fn store(db: &Db, rows: &[import::Row], send: &impl Fn(Event)) -> Result<()> {
    let added = db.insert(rows).await?;
    refresh(db, send).await?;
    send(Event::Status(format!(
        "{} relevés lus, {added} nouveaux enregistrés",
        rows.len()
    )));
    Ok(())
}
