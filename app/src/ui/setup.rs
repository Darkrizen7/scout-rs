//! First-launch assistant: WoW folder, addon, auction house, game data, prices for the addon.
//! It replaces the main window until it is finished; everything goes through the worker.

use eframe::egui;
use scout_worker::{Command, WorkerHandle};

use crate::state::State;

mod steps;

/// Width of the column the assistant is drawn in.
const WIDTH: f32 = 480.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    WowDir,
    Addon,
    House,
    GameData,
    Prices,
    Done,
}

impl Step {
    fn next(self) -> Option<Self> {
        Some(match self {
            Self::WowDir => Self::Addon,
            Self::Addon => Self::House,
            Self::House => Self::GameData,
            Self::GameData => Self::Prices,
            Self::Prices => Self::Done,
            Self::Done => return None,
        })
    }

    fn previous(self) -> Option<Self> {
        Some(match self {
            Self::WowDir => return None,
            Self::Addon => Self::WowDir,
            Self::House => Self::Addon,
            Self::GameData => Self::House,
            Self::Prices => Self::GameData,
            Self::Done => Self::Prices,
        })
    }

    fn title(self) -> &'static str {
        match self {
            Self::WowDir => "Dossier de WoW",
            Self::Addon => "Addon Scout",
            Self::House => "Hôtel des ventes",
            Self::GameData => "Données du jeu",
            Self::Prices => "Prix pour l'addon",
            Self::Done => "Terminé",
        }
    }
}

pub struct Setup {
    step: Step,
    wow_dir: String,
    /// Ask the worker for the server's houses (once per entry on the house step).
    load_houses: bool,
}

impl Setup {
    pub fn new(state: &State) -> Self {
        let wow_dir = state
            .config
            .as_ref()
            .and_then(|c| c.wow_dir.as_ref())
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        Self {
            step: Step::WowDir,
            wow_dir,
            load_houses: false,
        }
    }

    /// Draws the assistant; true once it is finished.
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) -> bool {
        let mut finished = false;
        egui::CentralPanel::default().show(ui, |ui| {
            let area = ui.max_rect();
            let width = WIDTH.min(area.width());
            let rect = egui::Rect::from_min_size(
                egui::pos2(area.center().x - width / 2.0, area.top() + 40.0),
                egui::vec2(width, (area.height() - 40.0).max(0.0)),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                finished = self.page(ui, state, worker);
            });
        });
        finished
    }

    fn page(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) -> bool {
        ui.weak(format!("Étape {} sur 6", self.step as usize + 1));
        ui.heading(self.step.title());
        ui.add_space(10.0);
        match self.step {
            Step::WowDir => self.wow_dir(ui, state, worker),
            Step::Addon => steps::addon(ui, state, worker),
            Step::House => self.house(ui, state, worker),
            Step::GameData => steps::game_data(ui, state, worker),
            Step::Prices => steps::prices(ui, state, worker),
            Step::Done => steps::done(ui),
        }
        ui.add_space(16.0);
        self.buttons(ui, state, worker)
    }

    fn buttons(&mut self, ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) -> bool {
        let mut finished = false;
        ui.horizontal(|ui| {
            if let Some(previous) = self.step.previous()
                && ui.button("Retour").clicked()
            {
                self.go(previous, worker);
            }
            match self.step.next() {
                Some(next) => {
                    let (label, enabled) = match self.step {
                        Step::WowDir => ("Suivant", self.has_wow_dir(state)),
                        // Optional: the tool works without the addon.
                        Step::Addon => ("Passer", true),
                        Step::House => ("Suivant", self.has_house(state)),
                        _ => ("Suivant", true),
                    };
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        self.go(next, worker);
                    }
                }
                None => {
                    if ui.button("Terminer").clicked() {
                        worker.send(Command::FinishSetup);
                        state.addon_history = None;
                        finished = true;
                    }
                }
            }
        });
        finished
    }

    fn has_wow_dir(&self, state: &State) -> bool {
        state.config.as_ref().is_some_and(|c| c.wow_dir.is_some())
    }

    fn has_house(&self, state: &State) -> bool {
        state
            .config
            .as_ref()
            .is_some_and(|c| c.price_server.house.is_some())
    }

    /// Changes step and does what its entry needs.
    fn go(&mut self, step: Step, worker: &WorkerHandle) {
        self.step = step;
        match step {
            Step::Addon => worker.send(Command::CheckScoutAddon),
            Step::House => self.load_houses = true,
            _ => {}
        }
    }
}
