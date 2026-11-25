//! Settings section of the price server (source « Scout API »): address, access key, auction house.

use eframe::egui;

use crate::state::State;
use crate::ui::theme;
use scout_core::time::format_datetime;
use scout_worker::{Command, Task, WorkerHandle};

/// What is being typed, sent to the worker by « Enregistrer ».
#[derive(Default)]
pub struct Form {
    url: String,
    key: String,
    /// Ask the worker for the server's houses (once per opening of the settings).
    load_houses: bool,
}

impl Form {
    pub fn open(&mut self, state: &State) {
        let server = state.config.as_ref().map(|c| &c.price_server);
        self.url = server.map(|s| s.url.clone()).unwrap_or_default();
        self.key = server.and_then(|s| s.key.clone()).unwrap_or_default();
        self.load_houses = server.is_some_and(|s| s.key.is_some());
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
        if std::mem::take(&mut self.load_houses) {
            worker.send(Command::LoadServerHouses);
        }
        let Some(config) = &state.config else {
            return;
        };
        let server = &config.price_server;
        egui::Grid::new("price_server")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Adresse")
                    .on_hover_text("Serveur qui relève l'hôtel des ventes par l'API Battle.net.");
                ui.add(egui::TextEdit::singleline(&mut self.url).desired_width(300.0));
                ui.end_row();
                ui.label("Clé d'accès")
                    .on_hover_text("Clé personnelle donnée par l'administrateur du serveur.");
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
            });
        let changed =
            self.url.trim() != server.url || Some(self.key.trim()) != server.key.as_deref();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(changed, egui::Button::new("Enregistrer"))
                .clicked()
            {
                worker.send(Command::SetPriceServer {
                    url: self.url.clone(),
                    key: Some(self.key.clone()),
                    house: server.house,
                });
            }
            let ready = server.ready().is_some();
            let refresh = ui
                .add_enabled(
                    ready && !state.busy(Task::Prices),
                    egui::Button::new("⟳ Mettre à jour"),
                )
                .on_hover_text(
                    "Derniers prix de tous les objets, sans attendre (sinon : toutes les heures). \
                     L'historique d'un objet se télécharge à l'ouverture de sa fiche.",
                );
            if refresh.clicked() {
                worker.send(Command::SyncPrices);
            }
            match server.last_sync {
                Some(ts) => ui.weak(format!("Derniers prix : {}", format_datetime(ts))),
                None => ui.weak("Jamais téléchargés"),
            };
        });
        house_choice(ui, state, worker, self);
    }
}

/// The auction house whose prices are kept, among the server's.
fn house_choice(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle, form: &Form) {
    let Some(config) = &state.config else {
        return;
    };
    let server = &config.price_server;
    if server.key.is_none() {
        return;
    }
    match &state.server_houses {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Hôtels des ventes du serveur…");
            });
        }
        Some(Err(error)) => {
            ui.colored_label(theme::current(ui).loss, format!("⚠ {error}"));
        }
        Some(Ok(houses)) => {
            let label = |id: u32| {
                houses.iter().find(|h| h.id == id).map_or_else(
                    || format!("n° {id}"),
                    |h| format!("n° {} · {}", h.id, h.name),
                )
            };
            let selected = server.house.map_or_else(|| "à choisir".to_owned(), label);
            ui.horizontal(|ui| {
                ui.label("Hôtel des ventes").on_hover_text(
                    "Les 15 maisons de WoW Forever : 5 serveurs, et sur chacun Alliance, Horde et \
                     Flots noirs. Choisis celle où tu vends.",
                );
                egui::ComboBox::from_id_salt("server_house")
                    .selected_text(selected)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        for house in houses {
                            let text = match house.last_update {
                                Some(ts) => format!(
                                    "n° {} · {} ({})",
                                    house.id,
                                    house.name,
                                    format_datetime(ts)
                                ),
                                None => format!("n° {} · {} (vide)", house.id, house.name),
                            };
                            if ui
                                .selectable_label(server.house == Some(house.id), text)
                                .clicked()
                                && server.house != Some(house.id)
                            {
                                worker.send(Command::SetPriceServer {
                                    url: form.url.clone(),
                                    key: Some(form.key.clone()),
                                    house: Some(house.id),
                                });
                            }
                        }
                    });
            });
        }
    }
}
