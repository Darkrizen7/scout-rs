//! The worker: the only place that touches disk, database, network and processes.
//!
//! Quick commands (loading a sheet, a zone, a favorite…) are answered by the worker loop; long tasks
//! (imports, game data download) each run on their own thread, so they never hold up the others.

mod tasks;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use anyhow::Result;
use scout_core::analysis;
use scout_db::Db;
use scout_sources as sources;
use scout_sources::wow;

use crate::config::{self, Config};
use crate::message::{Command, Event, Task};

/// Sends an event to the UI (and wakes it up); shared by the loop and the task threads.
pub(crate) type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// What every thread shares.
#[derive(Clone)]
struct Shared {
    db: Db,
    config: Arc<Mutex<Config>>,
    /// Long tasks running: a second click on the same one is ignored.
    running: Arc<Mutex<HashSet<Task>>>,
    send: Sink,
}

impl Shared {
    fn config(&self) -> std::sync::MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `job` on its own thread (with its own runtime), unless that task is already running.
    fn spawn(
        &self,
        task: Task,
        job: impl FnOnce(&tokio::runtime::Runtime, &Shared) -> Result<()> + Send + 'static,
    ) {
        if !self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(task)
        {
            return;
        }
        let shared = self.clone();
        let spawned = thread::Builder::new()
            .name(format!("{task:?}"))
            .spawn(move || {
                let send = &shared.send;
                send(Event::Busy(task, true));
                let result = runtime().and_then(|runtime| job(&runtime, &shared));
                if let Err(e) = result {
                    send(Event::Error(format!("{e:#}")));
                }
                shared
                    .running
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&task);
                send(Event::Busy(task, false));
            });
        if let Err(e) = spawned {
            self.running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&task);
            (self.send)(Event::Error(format!("impossible de lancer la tâche : {e}")));
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

pub(crate) fn run(commands: Receiver<Command>, send: Sink) {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(e) => return send(Event::Error(format!("runtime tokio : {e}"))),
    };
    // The old folders are moved, never recreated empty: on failure, stop rather than start a new database.
    if let Err(e) = config::migrate_old_folders() {
        return send(Event::Error(format!("migration de scout-prix : {e:#}")));
    }
    let config = Config::load();
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
    if let Err(e) = runtime.block_on(refresh(&db, &*send)) {
        send(Event::Error(format!("{e:#}")));
    }

    // Is the stored game data the newest Forever build? (network: on its own, the UI is already usable)
    let latest = send.clone();
    if let Err(e) = thread::Builder::new()
        .name("latest-build".into())
        .spawn(move || {
            latest(Event::LatestBuild(
                sources::latest_build().map_err(|e| format!("{e:#}")),
            ))
        })
    {
        send(Event::LatestBuild(Err(e.to_string())));
    }

    let shared = Shared {
        db,
        config: Arc::new(Mutex::new(config)),
        running: Arc::default(),
        send,
    };
    for command in commands {
        if let Err(e) = handle(command, &runtime, &shared) {
            (shared.send)(Event::Error(format!("{e:#}")));
        }
    }
}

fn handle(command: Command, runtime: &tokio::runtime::Runtime, shared: &Shared) -> Result<()> {
    let (db, send) = (&shared.db, &*shared.send);
    match command {
        Command::Import => shared.spawn(Task::Import, |runtime, shared| {
            let config = shared.config().clone();
            runtime.block_on(tasks::import(&shared.db, &config, &*shared.send))?;
            let mut config = shared.config();
            config.last_import = Some(chrono::Utc::now().timestamp());
            (shared.send)(Event::Config(config.clone()));
            config.save()
        }),
        Command::ImportLegacy => shared.spawn(Task::ImportLegacy, |runtime, shared| {
            let config = shared.config().clone();
            runtime.block_on(tasks::import_legacy(&shared.db, &config, &*shared.send))
        }),
        Command::UpdateGameData => shared.spawn(Task::GameData, |runtime, shared| {
            tasks::update_game_data(runtime, &shared.db, &*shared.send)
        }),
        Command::SetFavorite(id, favorite) => runtime.block_on(async {
            db.set_favorite(id, favorite).await?;
            send(Event::Favorites(db.favorites().await?));
            anyhow::Ok(())
        })?,
        Command::Analyze => {
            let observations = runtime.block_on(db.observations())?;
            send(Event::Analysis(Box::new(analysis::analyze(&observations))));
        }
        Command::LoadHistory(id) => send(Event::History(id, runtime.block_on(db.history(id))?)),
        Command::LoadDrops(id) => {
            let drops = runtime.block_on(db.item_drops(id))?;
            send(Event::Drops(id, Box::new(drops)));
        }
        Command::LoadZones => send(Event::Zones(runtime.block_on(db.zones())?)),
        Command::LoadZone(zone) => {
            let loot = runtime.block_on(db.zone_loot(&zone))?;
            send(Event::ZoneLoot(Box::new(loot)));
        }
        Command::LoadMapMasks => {
            let dir = shared.config().wow_dir().ok().and_then(wow::open_map_masks);
            // Without OpenMap, the zones are drawn as rectangles: an empty answer is enough.
            let (masks, result) =
                match dir.map_or_else(|| Ok(HashMap::new()), |d| sources::zone_masks(&d)) {
                    Ok(masks) => (masks, Ok(())),
                    Err(e) => (HashMap::new(), Err(e)),
                };
            send(Event::MapMasks(Arc::new(masks)));
            result?;
        }
        Command::SetWowDir(dir) => {
            let mut config = shared.config();
            config.wow_dir = Some(dir);
            send(Event::Config(config.clone()));
            config.save()?;
        }
        Command::SetTheme(theme) => {
            let mut config = shared.config();
            config.theme = theme;
            config.save()?;
        }
        Command::SetHideObsolete(hide) => {
            let mut config = shared.config();
            config.hide_obsolete = hide;
            config.save()?;
        }
    }
    Ok(())
}

async fn open_db() -> Result<Db> {
    Db::open(&config::database_path()?).await
}

async fn refresh(db: &Db, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Items(db.items().await?));
    send(Event::Favorites(db.favorites().await?));
    Ok(())
}
