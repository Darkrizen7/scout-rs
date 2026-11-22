//! Price and quantity over time for the selected item (two plots sharing the time axis).

use eframe::egui;
use egui_plot::{GridMark, Line, Plot, PlotResponse, Points};

use crate::state::State;
use crate::ui::coins;
use crate::ui::theme;
use crate::ui::widgets::star_button;
use scout_core::Snapshot;
use scout_core::merge::{self, Origin, Point};
use scout_core::money;
use scout_core::time::{self, format_datetime};
use scout_worker::WorkerHandle;

fn time_axis(mark: GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    time::format_axis(mark.value as i64)
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

    let color = theme::current(ui).series[0];
    let link = egui::Id::unique("scout-prix/time-axis");
    let height = (ui.available_height() - 20.0) / 2.0;
    let prices = merge::merge(history, |s| Some(s.price? as f64 / 10_000.0));
    let quantities = merge::merge(history, |s| Some(s.quantity? as f64));
    let plot = |id: &'static str, y_axis_money: bool| {
        let plot = Plot::new(id)
            .show_grid([false, true])
            .grid_spacing(egui::Rangef::new(40.0, 300.0))
            .link_axis(link, [true, false])
            .link_cursor(link, [true, false])
            .x_axis_formatter(time_axis)
            // The tooltip is drawn by `hover_tooltip`, with coins.
            .label_formatter(|_| None)
            .include_y(0.0);
        if y_axis_money {
            plot.y_axis_formatter(|mark, _| money::format((mark.value * 10_000.0).round() as i64))
        } else {
            plot
        }
    };
    let response = plot("price", true)
        .height(height * 1.3)
        .show(ui, |plot| add_series(plot, &prices, color));
    hover_tooltip(ui, &response, &prices, Unit::Money);
    let response = plot("quantity", false)
        .height(height * 0.7)
        .show(ui, |plot| add_series(plot, &quantities, color));
    hover_tooltip(ui, &response, &quantities, Unit::Count);
}

#[derive(Clone, Copy)]
enum Unit {
    /// Values are in gold (price charts).
    Money,
    Count,
}

fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::Scout => "Scout",
        Origin::Auctionator => "Auctionator",
        Origin::ForeverGuide => "Forever Guide",
        Origin::Average => "Moyenne Auctionator + Forever Guide",
    }
}

/// Distance in pixels under which the pointer counts as hovering a point.
const HOVER_RADIUS: f32 = 12.0;

/// Tooltip on the point nearest to the pointer: source, date, then the value (coins for prices).
fn hover_tooltip(ui: &egui::Ui, plot: &PlotResponse<()>, points: &[Point], unit: Unit) {
    let Some(pointer) = plot.response.hover_pos() else {
        return;
    };
    let screen = |p: &Point| {
        plot.transform
            .position_from_point(&egui_plot::PlotPoint::new(p.ts as f64, p.value))
    };
    let Some(point) = points
        .iter()
        .min_by(|a, b| {
            let (da, db) = (screen(a).distance(pointer), screen(b).distance(pointer));
            da.total_cmp(&db)
        })
        .filter(|p| screen(p).distance(pointer) <= HOVER_RADIUS)
    else {
        return;
    };
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        plot.response.layer_id,
        plot.response.id.with("point"),
        egui::PopupAnchor::Pointer,
    )
    .show(|ui| {
        ui.strong(origin_label(point.origin));
        ui.weak(format_datetime(point.ts));
        match unit {
            Unit::Money => {
                coins::money(ui, (point.value * 10_000.0).round() as i64, None);
            }
            Unit::Count => {
                ui.label(format!("{} en vente", point.value.round()));
            }
        }
    });
}

/// One line through all points, with a marker on each.
fn add_series(plot: &mut egui_plot::PlotUi<'_>, points: &[Point], color: egui::Color32) {
    if points.is_empty() {
        return;
    }
    let line: Vec<[f64; 2]> = points.iter().map(|p| [p.ts as f64, p.value]).collect();
    plot.line(Line::new("", line.clone()).color(color));
    plot.points(Points::new("", line).color(color).radius(3.5));
}
