//! Module tabs (top bar), Market header (back, title) and status bar.

use eframe::egui;

use crate::state::{Module, State, View, item_name};
use crate::ui::settings::Settings;
use crate::ui::theme;

/// Top bar: module switcher in the middle (GNOME view switcher style), settings on the right.
pub fn tabs(ui: &mut egui::Ui, state: &mut State, settings: &mut Settings) {
    ui.set_min_height(34.0);
    let full = ui.max_rect();
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(full).layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        ),
        |ui| {
            for (module, icon, label) in [
                (Module::Items, "📦", "Objets"),
                (Module::Recipes, "🔨", "Recettes"),
                (Module::Loot, "💀", "Butin"),
                (Module::Market, "📈", "Market"),
            ] {
                if tab(ui, icon, label, state.module == module).clicked() {
                    state.module = module;
                }
            }
        },
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            if ui
                .button("⚙ Réglages")
                .on_hover_text("Thème, imports, dossier WoW")
                .clicked()
            {
                settings.open(state);
            }
            if state.busy {
                ui.spinner();
            }
        },
    );
}

/// One tab of the switcher: rounded, filled when selected.
fn tab(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let palette = theme::current(ui);
    let text = egui::RichText::new(format!("{icon}  {label}")).size(15.0);
    let fill = if selected {
        palette.sidebar_selected
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(egui::Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(130.0, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Market header: back button and where we are.
pub fn show(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if state.view == View::Chart && ui.button("←").on_hover_text("Retour à la liste").clicked()
        {
            state.view = View::List;
        }
        ui.label(egui::RichText::new(title(state)).heading());
    });
}

/// "Objets › Artisanat › Tissu", "Favoris", "Analyse › Artisanat", an item name…
fn title(state: &State) -> String {
    let base = match state.view {
        View::Chart => return state.selected_item().map(item_name).unwrap_or_default(),
        View::Calculator => return "Rentabilité".into(),
        View::Analysis => "Analyse",
        View::List if state.filter.favorites => return "Favoris".into(),
        View::List => "Tous les objets",
    };
    let f = &state.filter;
    let category = state
        .items
        .iter()
        .find(|i| f.class_id.is_some() && i.class_id == f.class_id);
    let mut parts = vec![base.to_owned()];
    if let Some(item) = category {
        parts.extend(item.class_name.clone());
        if f.subclass_id.is_some() {
            let sub = state
                .items
                .iter()
                .find(|i| i.class_id == f.class_id && i.subclass_id == f.subclass_id);
            parts.extend(sub.and_then(|i| i.subclass_name.clone()));
        }
    }
    parts.join(" › ")
}

pub fn status(ui: &mut egui::Ui, state: &State) {
    ui.horizontal(|ui| match &state.error {
        Some(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        None => {
            ui.weak(format!(
                "{} objets · {} favoris",
                state.items.len(),
                state.favorites.len()
            ));
            if !state.status.is_empty() {
                ui.weak("·");
                ui.weak(&state.status);
            }
        }
    });
}
