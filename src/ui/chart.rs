//! Price and quantity over time for the selected item (two plots sharing the time axis).

use chrono::{Local, TimeZone};
use eframe::egui;
use egui_plot::{GridMark, HoverPosition, Legend, Line, Plot, Points};

use crate::app::{State, item_name};
use crate::db::Snapshot;
use crate::import;
use crate::money;
use crate::ui::theme;

pub fn format_datetime(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|d| d.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

fn time_axis(mark: GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    Local
        .timestamp_opt(mark.value as i64, 0)
        .single()
        .map(|d| d.format("%d/%m %Hh").to_string())
        .unwrap_or_default()
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

pub fn show(ui: &mut egui::Ui, state: &State) {
    let Some(item) = state.selected_item() else {
        ui.centered_and_justified(|ui| ui.label("Choisis un objet dans l'onglet Objets."));
        return;
    };
    ui.heading(item_name(item));
    let history = match &state.history {
        Some((id, history)) if *id == item.item_id => history,
        _ => {
            ui.spinner();
            return;
        }
    };

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
    (import::SCOUT, "Scout"),
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
