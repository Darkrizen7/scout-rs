//! Settings section of the price server (source « Scout API »): address (dev builds only), access key, region and
//! auction house.

use eframe::egui;

use crate::state::State;
use crate::ui::{theme, widgets};
use scout_core::time::format_datetime;
use scout_worker::{Command, Task, WorkerHandle};

/// What is being typed, sent to the worker by « Enregistrer ».
#[derive(Default)]
pub struct Form {
    url: String,
    key: String,
    /// Ask the worker for the server's regions and houses (once per opening of the settings).
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
                // Users keep the default server: the address is only editable in dev builds.
                if cfg!(debug_assertions) {
                    ui.label("Adresse").on_hover_text(
                        "Serveur qui relève l'hôtel des ventes par l'API Battle.net.",
                    );
                    ui.add(egui::TextEdit::singleline(&mut self.url).desired_width(300.0));
                    ui.end_row();
                }
                ui.label("Clé d'accès")
                    .on_hover_text("Clé personnelle donnée par l'administrateur du serveur.");
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
                if server.key.is_some() {
                    ui.label("Région");
                    region_choice(ui, state, worker);
                    ui.end_row();
                    ui.label("Hôtel des ventes");
                    widgets::house_picker(ui, state, worker, 300.0);
                    ui.end_row();
                }
            });
        if let Some(Err(error)) = &state.server_houses {
            ui.colored_label(theme::current(ui).loss, format!("⚠ {error}"));
        }
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
                    region: server.region.clone(),
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
    }
}

/// The Battle.net regions where the server found Forever auction houses.
fn region_choice(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle) {
    let Some(config) = &state.config else {
        return;
    };
    let server = &config.price_server;
    let regions = match &state.server_houses {
        Some(Ok(choice)) => &choice.regions[..],
        _ => &[],
    };
    egui::ComboBox::from_id_salt("server_region")
        .selected_text(server.region.to_uppercase())
        .width(80.0)
        .show_ui(ui, |ui| {
            if regions.is_empty() {
                ui.weak("Liste des régions pas encore reçue");
            }
            for region in regions {
                if ui
                    .selectable_label(*region == server.region, region.to_uppercase())
                    .clicked()
                    && *region != server.region
                {
                    worker.send(Command::SetPriceServer {
                        url: server.url.clone(),
                        key: server.key.clone(),
                        region: region.clone(),
                        house: None,
                    });
                }
            }
        })
        .response
        .on_hover_text(
            "Régions Battle.net où le serveur a trouvé des hôtels des ventes WoW Forever. \
             En changer demande de choisir à nouveau l'hôtel des ventes.",
        );
}
