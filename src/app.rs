//! Application state and top-level layout.

use eframe::egui;

use crate::config::{Config, Theme};
use crate::db::{ItemSummary, Snapshot};
use crate::ui::{calculator, chart, item_list, theme, toolbar};
use crate::worker::{Command, Event, WorkerHandle};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Chart,
    Calculator,
}

/// Everything the panels read; only changed by worker events and user input.
pub struct State {
    pub view: View,
    pub config: Option<Config>,
    pub busy: bool,
    pub status: String,
    pub error: Option<String>,
    pub items: Vec<ItemSummary>,
    /// Bumped whenever `items` changes, so panels can refresh their caches.
    pub revision: u64,
    pub selected: Option<i64>,
    pub history: Option<(i64, Vec<Snapshot>)>,
}

impl State {
    fn apply(&mut self, event: Event) {
        match event {
            Event::Config(config) => self.config = Some(config),
            Event::Busy(busy) => self.busy = busy,
            Event::Status(status) => {
                self.status = status;
                self.error = None;
            }
            Event::Error(error) => self.error = Some(error),
            Event::Items(items) => {
                self.items = items;
                self.revision += 1;
            }
            Event::History(id, history) => self.history = Some((id, history)),
        }
    }

    pub fn selected_item(&self) -> Option<&ItemSummary> {
        let id = self.selected?;
        self.items.iter().find(|i| i.item_id == id)
    }

    /// Selects an item and asks for its history if not already loaded.
    pub fn select(&mut self, item_id: i64, worker: &WorkerHandle) {
        self.selected = Some(item_id);
        if self.history.as_ref().is_none_or(|(id, _)| *id != item_id) {
            worker.send(Command::LoadHistory(item_id));
        }
    }
}

pub fn item_name(item: &ItemSummary) -> String {
    item.name
        .clone()
        .unwrap_or_else(|| format!("objet #{}", item.item_id))
}

pub struct App {
    worker: WorkerHandle,
    state: State,
    list: item_list::ItemList,
    calculator: calculator::Calculator,
    settings: toolbar::Settings,
    /// Theme currently applied to the egui context.
    theme: Option<Theme>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            worker: WorkerHandle::spawn(cc.egui_ctx.clone()),
            state: State {
                view: View::List,
                config: None,
                busy: false,
                status: String::new(),
                error: None,
                items: Vec::new(),
                revision: 0,
                selected: None,
                history: None,
            },
            list: item_list::ItemList::default(),
            calculator: calculator::Calculator::default(),
            settings: toolbar::Settings::default(),
            theme: None,
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let events: Vec<Event> = self.worker.poll().collect();
        for event in events {
            self.state.apply(event);
        }
        let wanted = self
            .state
            .config
            .as_ref()
            .map_or_else(Theme::default, |c| c.theme);
        if self.theme != Some(wanted) {
            theme::apply(ctx, wanted);
            self.theme = Some(wanted);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let Self {
            worker,
            state,
            list,
            calculator,
            settings,
            ..
        } = self;

        egui::Panel::top("toolbar").show(ui, |ui| toolbar::show(ui, state, worker, settings));
        egui::Panel::bottom("status").show(ui, |ui| toolbar::status(ui, state));
        egui::CentralPanel::default().show(ui, |ui| match state.view {
            View::List => list.show(ui, state, worker),
            View::Chart => chart::show(ui, state),
            View::Calculator => calculator.show(ui, state),
        });
        settings.window(&ui.ctx().clone(), state, worker);
    }
}
