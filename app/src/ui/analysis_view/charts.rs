//! The two charts above the analysis tables: summary line and market price level per day.

use eframe::egui;
use egui_plot::{Bar, BarChart, Plot};
use scout_core::analysis::{self, Analysis};

use crate::ui::theme;

pub(super) fn summary(ui: &mut egui::Ui, analysis: &Analysis) {
    ui.label(format!(
        "{} jours de données : {} de week-end, {} de semaine. Prix d'un jour = médiane des prix min relevés ce jour-là.",
        analysis.days.len(),
        analysis.weekend_days,
        analysis.weekday_days
    ));
    let palette = theme::current(ui);
    match &analysis.market_weekend {
        None => {
            ui.label(format!(
                "Effet week-end du marché : pas assez de données (il faut au moins {} jours de week-end et {} de semaine).",
                analysis::MIN_DAYS_EACH,
                analysis::MIN_DAYS_EACH
            ));
        }
        Some(w) => {
            let verdict = if w.significant() {
                "significatif"
            } else {
                "pas significatif (peut être du hasard)"
            };
            ui.horizontal(|ui| {
                ui.label("Effet week-end du marché :");
                let color = if w.diff_pct < 0.0 {
                    palette.gain
                } else {
                    palette.loss
                };
                ui.colored_label(
                    color,
                    egui::RichText::new(format!("{:+.1} %", w.diff_pct)).strong(),
                );
                ui.label(format!("le week-end, p = {:.3} → {verdict}.", w.p_value));
            });
        }
    }
}

/// Market price index per day, relative to each item's usual price.
pub(super) fn day_chart(ui: &mut egui::Ui, analysis: &Analysis) {
    let palette = theme::current(ui);
    let first = analysis.days.first().map(|d| d.date);
    let bars: Vec<Bar> = analysis
        .days
        .iter()
        .map(|d| {
            let x = first.map_or(0, |f| (d.date - f).num_days()) as f64;
            let kind = if d.weekend { "week-end" } else { "semaine" };
            Bar::new(x, (d.index - 1.0) * 100.0)
                .width(0.7)
                .name(format!(
                    "{} ({kind}, {} objets)",
                    d.date.format("%a %d/%m"),
                    d.items
                ))
                .fill(if d.weekend {
                    palette.series[0]
                } else {
                    palette.series[1]
                })
        })
        .collect();
    let dates: Vec<_> = analysis.days.iter().map(|d| d.date).collect();
    Plot::new("days")
        .height(170.0)
        .show_grid([false, true])
        .grid_spacing(egui::Rangef::new(40.0, 300.0))
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .x_axis_formatter(move |mark, _| {
            let day = first.map(|f| f + chrono::Duration::days(mark.value.round() as i64));
            match day {
                Some(d) if (mark.value - mark.value.round()).abs() < 1e-6 && dates.contains(&d) => {
                    d.format("%d/%m").to_string()
                }
                _ => String::new(),
            }
        })
        .y_axis_formatter(|mark, _| format!("{:+.0} %", mark.value))
        .y_axis_label("vs prix habituel")
        .show(ui, |plot| {
            plot.bar_chart(BarChart::new("Niveau des prix", bars))
        });
    ui.small("Jaune : week-end · bleu : semaine. Barre = écart médian des objets ce jour-là par rapport à leur prix habituel.");
}
