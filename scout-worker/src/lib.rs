//! Background threads: export, database, network and settings. The UI only sends commands and reads events.
//!
//! After each event the `notify` callback runs (e.g. `ctx.request_repaint()`), so the UI never polls.

mod config;
mod message;
mod reference;
mod worker;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub use config::{Config, PriceServer, database_path};
pub use message::{Command, Event, Task};
pub use reference::export as export_reference;

/// Callback invoked after each event is sent.
pub type Notify = Box<dyn Fn() + Send + Sync + 'static>;

/// UI-side handle on the worker thread.
pub struct WorkerHandle {
    commands: Sender<Command>,
    events: Receiver<Event>,
}

impl WorkerHandle {
    pub fn spawn(notify: Notify) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        // The UI is gone when sending fails: nothing left to do.
        let send: worker::Sink = Arc::new(move |event| {
            let _ = event_tx.send(event);
            notify();
        });
        thread::Builder::new()
            .name("worker".into())
            .spawn(move || worker::run(command_rx, send))
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
