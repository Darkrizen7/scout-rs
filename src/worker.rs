//! Background thread: export, database and settings. The UI only sends commands and reads events.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use anyhow::Result;
use eframe::egui;

use crate::config::{self, Config, Theme};
use crate::db::{Db, ItemSummary, Snapshot};
use crate::import;

pub enum Command {
    Import,
    /// One-off import of the prices Auctionator and Forever Guide kept.
    ImportLegacy,
    LoadHistory(i64),
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
    if let Err(e) = runtime.block_on(refresh(&db, &send)) {
        send(Event::Error(format!("{e:#}")));
    }

    for command in commands {
        send(Event::Busy(true));
        let result = match command {
            Command::Import => runtime.block_on(import(&db, &config, &send)),
            Command::ImportLegacy => runtime.block_on(import_legacy(&db, &config, &send)),
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
    store(db, &rows, send).await
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
