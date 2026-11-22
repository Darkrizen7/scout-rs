//! Price and quantity over time for the selected item (two plots sharing the time axis).

use eframe::egui;
use egui_plot::{GridMark, HoverPosition, Legend, Line, Plot, Points};

use crate::state::State;
use crate::ui::theme;
use crate::ui::widgets::star_button;
use scout_core::SCOUT;
use scout_core::Snapshot;
use scout_core::money;
use scout_core::time::{self, format_datetime};
use scout_worker::WorkerHandle;

fn time_axis(mark: GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    time::format_axis(mark.value as i64)
}

/// Hovered point, prefixed by its series name when on a data point.
fn hovered(pos: &HoverPosition<'_>) -> (String, egui_plot::PlotPoint) {
    match pos {
        HoverPosition::NearDataPoint {
            plot_name,
            position,
            ..
        } => (format!("{plot_name} · "), *position),
        HoverPosition::Elsewhere { position } => (String::new(), *position),
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut State, worker: &WorkerHandle) {
    let Some(item) = state.selected_item() else {
        ui.centered_and_justified(|ui| ui.label("Choisis un objet dans la liste."));
        return;
    };
    let item_id = item.item_id;
    let category = [&item.class_name, &item.subclass_name]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" › ");
    let mut toggle = false;
    ui.horizontal(|ui| {
        toggle = star_button(ui, state.is_favorite(item_id), theme::current(ui)).clicked();
        ui.weak(format!("n° {item_id}"));
        if !category.is_empty() {
            ui.weak(format!("· {category}"));
        }
    });
    if toggle {
        state.toggle_favorite(item_id, worker);
    }
    let history = match &state.history {
        Some((id, history)) if *id == item_id => history,
        _ => {
            ui.spinner();
            return;
        }
    };

    plots(ui, history);
}

/// Price (top) and quantity (bottom) over time, one series per source, sharing the time axis.
pub fn plots(ui: &mut egui::Ui, history: &[Snapshot]) {
    let with_price = history.iter().filter(|s| s.price.is_some()).count();
    ui.label(format!(
        "{} relevés · {with_price} avec un prix",
        history.len()
    ));

    let colors = theme::current(ui).series;
    let link = egui::Id::unique("scout-prix/time-axis");
    let height = (ui.available_height() - 20.0) / 2.0;
    Plot::new("price")
        .height(height * 1.3)
        .show_grid([false, true])
        .grid_spacing(egui::Rangef::new(40.0, 300.0))
        .legend(Legend::default())
        .link_axis(link, [true, false])
        .link_cursor(link, [true, false])
        .x_axis_formatter(time_axis)
        .y_axis_formatter(|mark, _| money::format((mark.value * 10_000.0).round() as i64))
        .label_formatter(|pos| {
            let (source, p) = hovered(pos);
            let price = money::format((p.y * 10_000.0).round() as i64);
            Some(format!("{source}{}\n{price}", format_datetime(p.x as i64)))
        })
        .include_y(0.0)
        .show(ui, |plot| {
            for ((source, label), color) in SOURCES.into_iter().zip(colors) {
                let points = series(history, source, |s| Some(s.price? as f64 / 10_000.0));
                add_series(plot, label, color, points);
            }
        });
    Plot::new("quantity")
        .height(height * 0.7)
        .show_grid([false, true])
        .grid_spacing(egui::Rangef::new(40.0, 300.0))
        .legend(Legend::default())
        .link_axis(link, [true, false])
        .link_cursor(link, [true, false])
        .x_axis_formatter(time_axis)
        .label_formatter(|pos| {
            let (source, p) = hovered(pos);
            Some(format!(
                "{source}{}\n{} en vente",
                format_datetime(p.x as i64),
                p.y.round()
            ))
        })
        .include_y(0.0)
        .show(ui, |plot| {
            for ((source, label), color) in SOURCES.into_iter().zip(colors) {
                let points = series(history, source, |s| Some(s.quantity? as f64));
                add_series(plot, label, color, points);
            }
        });
}

/// (source in the database, legend label); colors come from the theme palette, same order.
const SOURCES: [(&str, &str); 3] = [
    (SCOUT, "Scout"),
    ("auctionator", "Auctionator"),
    ("foreverguide", "Forever Guide"),
];

fn series(
    history: &[Snapshot],
    source: &str,
    value: impl Fn(&Snapshot) -> Option<f64>,
) -> Vec<[f64; 2]> {
    history
        .iter()
        .filter(|s| s.source == source)
        .filter_map(|s| Some([s.ts as f64, value(s)?]))
        .collect()
}

fn add_series(
    plot: &mut egui_plot::PlotUi<'_>,
    label: &str,
    color: egui::Color32,
    points: Vec<[f64; 2]>,
) {
    if points.is_empty() {
        return;
    }
    plot.line(Line::new(label, points.clone()).color(color));
    plot.points(Points::new(label, points).color(color).radius(3.0));
}
