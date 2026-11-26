//! The worker: the only place that touches disk, database, network and processes.
//!
//! Quick commands (loading a sheet, a zone, a favorite…) are answered by the worker loop; long tasks
//! (imports, game data download) each run on their own thread, so they never hold up the others.
//!
//! All threads share one multi-threaded tokio runtime and call `block_on` on it. Its own thread keeps
//! the background tasks moving: sqlx gives the pool's single connection back in such a task, so with one
//! runtime per thread (only driven inside `block_on`) a thread could wait for a connection that the
//! idle thread holding it never returned.

mod prices;
mod tasks;
mod update;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use anyhow::{Context, Result};
use scout_core::analysis;
use scout_core::sales::Sales;
use scout_core::scout_api::AuctionHouse;
use scout_core::update::AddonInstall;
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
    runtime: Arc<tokio::runtime::Runtime>,
    db: Db,
    config: Arc<Mutex<Config>>,
    /// Long tasks running: a second click on the same one is ignored.
    running: Arc<Mutex<HashSet<Task>>>,
    send: Sink,
    /// This executable, found before an update replaces it (Linux then names it "… (deleted)").
    exe: Option<PathBuf>,
}

impl Shared {
    fn config(&self) -> std::sync::MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `job` on its own thread, unless that task is already running.
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
                let result = job(&shared.runtime, &shared);
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

/// One worker thread is enough: the database has a single connection and the network calls block.
fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("tokio")
        .enable_all()
        .build()?)
}

pub(crate) fn run(commands: Receiver<Command>, send: Sink) {
    let runtime = match runtime() {
        Ok(runtime) => Arc::new(runtime),
        Err(e) => return send(Event::Error(format!("runtime tokio : {e}"))),
    };
    // The old folders are moved, never recreated empty: on failure, stop rather than start a new database.
    if let Err(e) = config::migrate_old_folders() {
        return send(Event::Error(format!("migration de scout-prix : {e:#}")));
    }
    let config = Config::load();
    send(Event::Config(config.clone()));
    send(Event::ScoutAddon(scout_addon(&config)));

    let db = match runtime.block_on(open_db()) {
        Ok(db) => db,
        Err(e) => return send(Event::Error(format!("base de données : {e:#}"))),
    };
    // A new database has no game data: downloaded below, once the worker is running.
    let first_start = !matches!(runtime.block_on(db.game_version()), Ok(Some(_)));
    match runtime.block_on(db.load_game()) {
        Ok(game) if !game.is_empty() => send(Event::GameData(Arc::new(game))),
        Ok(_) => {}
        Err(e) => send(Event::Error(format!("données du jeu : {e:#}"))),
    }
    match runtime.block_on(db.vendor_data()) {
        Ok(prices) => send(Event::Vendor(Arc::new(prices))),
        Err(e) => send(Event::Error(format!("prix marchands : {e:#}"))),
    }
    let shared_runtime = runtime.clone();
    let house = config.price_server.auction_house();
    if let Err(e) = runtime.block_on(async {
        if house.id != 0 {
            db.claim_unassigned(&house).await?;
        }
        refresh(&db, &house, &*send).await
    }) {
        send(Event::Error(format!("{e:#}")));
    }

    // Is the stored game data the newest package of the price server? (network: on its own, the
    // UI is already usable)
    let latest = send.clone();
    let server = config.price_server.clone();
    if let Err(e) = thread::Builder::new()
        .name("latest-game-data".into())
        .spawn(move || {
            latest(Event::LatestGameData(
                tasks::latest_game_data(&server).map_err(|e| format!("{e:#}")),
            ))
        })
    {
        send(Event::LatestGameData(Err(e.to_string())));
    }
    // Newer loot and merchants on the server? (network: on its own; silent when it fails)
    let (reference_db, reference_send) = (db.clone(), send.clone());
    let reference_server = config.price_server.clone();
    let reference_runtime = shared_runtime.clone();
    // (Not on the first start: the download of everything covers it.)
    if !first_start {
        let _ = thread::Builder::new()
            .name("server-reference".into())
            .spawn(move || {
                tasks::update_server_reference(
                    &reference_runtime,
                    &reference_db,
                    &reference_server,
                    true,
                    &*reference_send,
                )
            });
    }
    // A newer version of the tool? (network too; silent when offline)
    let updates = send.clone();
    let _ = thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            if let Some(release) = update::check_tool() {
                updates(Event::UpdateAvailable(release));
            }
            if let Some(release) = update::check_addon() {
                updates(Event::AddonRelease(release));
            }
        });

    let shared = Shared {
        runtime: shared_runtime,
        db,
        config: Arc::new(Mutex::new(config)),
        running: Arc::default(),
        send,
        exe: std::env::current_exe().ok(),
    };
    prices::start(&shared);
    if first_start {
        shared.spawn(Task::GameData, |runtime, shared| {
            tasks::update_all(
                runtime,
                &shared.db,
                &shared.config().price_server,
                &*shared.send,
            )
        });
    }
    for command in commands {
        if let Err(e) = handle(command, &shared) {
            (shared.send)(Event::Error(format!("{e:#}")));
        }
    }
}

/// The Scout addon in the configured WoW folder (missing without one).
fn scout_addon(config: &Config) -> AddonInstall {
    config
        .wow_dir()
        .map_or(AddonInstall::Missing, sources::addon_install)
}

fn handle(command: Command, shared: &Shared) -> Result<()> {
    let (runtime, db, send) = (&*shared.runtime, &shared.db, &*shared.send);
    match command {
        Command::Import => shared.spawn(Task::Import, |runtime, shared| {
            let config = shared.config().clone();
            runtime.block_on(tasks::import(&shared.db, &config, &*shared.send))?;
            let mut config = shared.config();
            config.last_import = Some(chrono::Utc::now().timestamp());
            (shared.send)(Event::Config(config.clone()));
            config.save()
        }),
        Command::UpdateGameData => shared.spawn(Task::GameData, |runtime, shared| {
            tasks::update_all(
                runtime,
                &shared.db,
                &shared.config().price_server,
                &*shared.send,
            )
        }),
        Command::SetFavorite(id, favorite) => runtime.block_on(async {
            db.set_favorite(id, favorite).await?;
            send(Event::Favorites(db.favorites().await?));
            anyhow::Ok(())
        })?,
        Command::SetZoneFavorite(zone, favorite) => runtime.block_on(async {
            db.set_zone_favorite(&zone, favorite).await?;
            send(Event::ZoneFavorites(db.zone_favorites().await?));
            anyhow::Ok(())
        })?,
        Command::Analyze => {
            let house = shared.config().price_server.auction_house();
            let observations = runtime.block_on(db.observations(&house))?;
            send(Event::Analysis(Box::new(analysis::analyze(&observations))));
        }
        Command::LoadHistory(id) => {
            let house = shared.config().price_server.auction_house();
            send(Event::History(
                id,
                runtime.block_on(db.history(id, &house))?,
            ));
            prices::history_in_background(shared, id);
        }
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
            send(Event::ScoutAddon(scout_addon(&config)));
            config.save()?;
        }
        Command::CheckScoutAddon => send(Event::ScoutAddon(scout_addon(&shared.config()))),
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
        Command::SetProfessionSkill(profession, skill) => {
            let mut config = shared.config();
            match skill {
                Some(skill) => config
                    .profession_skills
                    .insert(profession.to_string(), skill),
                None => config.profession_skills.remove(&profession.to_string()),
            };
            config.save()?;
        }
        Command::InstallUpdate(release) => shared.spawn(Task::Update, move |_, shared| {
            let exe = shared
                .exe
                .as_deref()
                .context("chemin de l'exécutable inconnu")?;
            update::install(&release, exe, &*shared.send)
        }),
        Command::InstallAddon(release) => shared.spawn(Task::Addon, move |_, shared| {
            let wow_dir = shared.config().wow_dir()?.to_path_buf();
            update::install_addon(&release, &wow_dir, &*shared.send)
        }),
        Command::SetPriceServer { url, region, house } => {
            prices::set_server(shared, url, (region, house))?
        }
        Command::LoadServerHouses => prices::load_houses(shared),
        Command::SyncPrices => shared.spawn(Task::Prices, |runtime, shared| {
            prices::sync_latest(runtime, shared, true)
        }),
        Command::Restart => {
            let exe = shared
                .exe
                .as_deref()
                .context("chemin de l'exécutable inconnu")?;
            update::restart(exe, send)?;
        }
    }
    Ok(())
}

async fn open_db() -> Result<Db> {
    Db::open(&config::database_path()?).await
}

/// The items of the house shown, and the favorites.
async fn refresh(db: &Db, house: &AuctionHouse, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Items(db.items(house).await?));
    send_sales(db, house, send).await?;
    send(Event::Favorites(db.favorites().await?));
    send(Event::ZoneFavorites(db.zone_favorites().await?));
    Ok(())
}

/// What the house sells per day, from the sales stored.
async fn send_sales(db: &Db, house: &AuctionHouse, send: &dyn Fn(Event)) -> Result<()> {
    let sales = Sales::from_rows(&db.sales_rows(house).await?);
    send(Event::Sales(Arc::new(sales)));
    Ok(())
}
