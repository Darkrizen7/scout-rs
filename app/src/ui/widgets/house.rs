//! Which auction house every page shows (header and settings): the price server's houses of the
//! chosen region.

use eframe::egui;

use crate::state::State;
use scout_core::time::format_datetime;
use scout_worker::{Command, WorkerHandle};

/// Combo box of the houses; choosing one tells the worker (prices, items and charts follow).
pub fn house_picker(ui: &mut egui::Ui, state: &State, worker: &WorkerHandle, width: f32) {
    let Some(config) = &state.config else {
        return;
    };
    let server = &config.price_server;
    let region = server.region.to_uppercase();
    let houses = match &state.server_houses {
        Some(Ok(choice)) if choice.region == server.region => &choice.houses[..],
        _ => &[],
    };
    let current = server.house.map(|id| {
        houses
            .iter()
            .find(|h| h.id == id)
            .map_or_else(|| id.to_string(), |h| h.label())
    });
    let selected = match &current {
        Some(label) => format!("{region} {label}"),
        None => "Hôtel des ventes : à choisir".to_owned(),
    };
    let combo = egui::ComboBox::from_id_salt(("house_picker", width as u32))
        .selected_text(selected)
        .width(width)
        .show_ui(ui, |ui| {
            if houses.is_empty() {
                ui.weak("Serveur de prix à régler (Réglages) ou injoignable");
            }
            for house in houses {
                let when = house
                    .last_update
                    .map_or_else(|| "vide".to_owned(), format_datetime);
                let text = format!("{region} {} — {when}", house.label());
                if ui
                    .selectable_label(server.house == Some(house.id), text)
                    .clicked()
                    && server.house != Some(house.id)
                {
                    worker.send(Command::SetPriceServer {
                        url: server.url.clone(),
                        key: server.key.clone(),
                        region: server.region.clone(),
                        house: Some(house.id),
                    });
                }
            }
        });
    combo.response.on_hover_text(
        "Hôtel des ventes dont toutes les pages montrent les prix (Scout, Scout API, Auctionator, \
         Forever Guide). Chaque région a 15 maisons : 5 groupes (sans doute un par type de royaume), \
         chacun avec Alliance, Horde et Flots noirs.",
    );
}
