//! Price and quantity over time for the selected item (two plots sharing the time axis).

mod limits;
mod money_axis;

use eframe::egui;
use egui_plot::{GridInput, GridMark, Line, Plot, PlotResponse, Points};

use crate::state::State;
use crate::ui::coins;
use crate::ui::theme;
use crate::ui::widgets::{self, star_button};
use scout_core::Snapshot;
use scout_core::merge::{self, Origin, Period, Point};
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

    plots(ui, item_id, history, true);
}

/// Price (top) and quantity (bottom) over time, one series per source, sharing the time axis.
/// `periods` adds the day / week / month buttons (Market view only).
pub fn plots(ui: &mut egui::Ui, item_id: i64, history: &[Snapshot], periods: bool) {
    let color = theme::current(ui).series[0];
    // A new item starts from a fresh view, not from the previous item's zoom and offset.
    let seen = egui::Id::NULL.with(("scout-rs/chart-item", ui.scope_id()));
    let changed = ui.data(|d| d.get_temp::<i64>(seen)) != Some(item_id);
    if changed {
        ui.data_mut(|d| d.insert_temp(seen, item_id));
    }
    // Aggregation of the market view, kept across items; the item sheet always shows days.
    let period_id = egui::Id::NULL.with("scout-rs/chart-period");
    let mut period = if periods {
        ui.data(|d| d.get_temp::<Period>(period_id))
            .unwrap_or_default()
    } else {
        Period::Day
    };
    let link = egui::Id::unique("scout-rs/time-axis");
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        if ui.button("Aujourd'hui").clicked() {
            limits::show_end(ui.ctx());
        }
        if periods {
            // Aggregators, apart from the "go to the end" button. Right-to-left layout: added in
            // reverse order.
            widgets::vertical_separator(ui);
            for (label, value, days) in [
                ("Mensuel", Period::Month, 7.0 * 30.0),
                ("Hebdo", Period::Week, 7.0 * 7.0),
                ("Journalier", Period::Day, 7.0),
                ("Détaillé", Period::Detail, 2.0),
            ] {
                if ui
                    .add(egui::Button::new(label).selected(period == value))
                    .clicked()
                {
                    period = value;
                    ui.data_mut(|d| d.insert_temp(period_id, value));
                    limits::show_last(ui.ctx(), days * 86_400.0);
                }
            }
        }
    });
    let height = ((ui.available_height() - 20.0) / 2.0).max(90.0);
    let prices = merge::series(history, period, |s| Some(s.price? as f64 / 10_000.0));
    let quantities = merge::series(history, period, |s| Some(s.quantity? as f64));
    let limits = limits::Limits::of(prices.iter().chain(&quantities));
    let plot = |id: &'static str, y_axis_money: bool| {
        let mut plot = Plot::new(id)
            .show_grid([false, true])
            .grid_spacing(egui::Rangef::new(40.0, 300.0))
            .link_axis(link, [true, false])
            .link_cursor(link, [true, false])
            .x_axis_formatter(time_axis)
            // The tooltip is drawn by `hover_tooltip`, with coins.
            .label_formatter(|_| None)
            .include_y(0.0)
            // Panning and zooming are done by `Limits::navigate`, which keeps them in bounds.
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_axis_zoom_drag(false);
        if changed {
            plot = plot.reset();
        }
        if y_axis_money {
            // Labels and grid are drawn by `money_axis`, with coins.
            plot.show_grid([false, false])
                .y_axis_min_width(money_axis::WIDTH)
                .y_axis_formatter(|_, _| String::new())
        } else {
            plot.y_axis_min_width(money_axis::WIDTH)
                .y_grid_spacer(count_grid)
        }
    };
    let response = plot("price", true).height(height * 1.3).show(ui, |plot| {
        limits.navigate(plot, limits::Role::Price, &prices);
        add_series(plot, &prices, color);
    });
    money_axis::draw(ui, &response);
    hover_tooltip(ui, &response, &prices, Unit::Money);
    let response = plot("quantity", false)
        .height(height * 0.7)
        .show(ui, |plot| {
            limits.navigate(plot, limits::Role::Quantity, &quantities);
            add_series(plot, &quantities, color);
        });
    hover_tooltip(ui, &response, &quantities, Unit::Count);
}

/// Grid of the quantity plot: whole numbers, steps of 1, 2 or 5 × 10^k (egui_plot's default only
/// uses powers of ten, which leaves a small range with no line at all).
fn count_grid(input: GridInput) -> Vec<GridMark> {
    let (lo, hi) = input.bounds;
    let step = [1.0, 2.0, 5.0]
        .into_iter()
        .flat_map(|m| (0..12).map(move |k| m * 10_f64.powi(k)))
        .filter(|s| *s >= input.base_step_size)
        .fold(f64::MAX, f64::min);
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    (first..=last)
        .map(|i| GridMark {
            value: i as f64 * step,
            step_size: step,
        })
        .collect()
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
        Origin::Period => "Moyenne de la période",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(bounds: (f64, f64), base_step_size: f64) -> Vec<f64> {
        count_grid(GridInput {
            bounds,
            base_step_size,
        })
        .iter()
        .map(|m| m.value)
        .collect()
    }

    #[test]
    fn count_grid_uses_whole_steps_of_one_two_or_five() {
        assert_eq!(marks((0.0, 6.0), 1.5), [0.0, 2.0, 4.0, 6.0]);
        assert_eq!(marks((3.2, 7.9), 0.3), [4.0, 5.0, 6.0, 7.0]);
        assert_eq!(marks((0.0, 120.0), 30.0), [0.0, 50.0, 100.0]);
    }
}
