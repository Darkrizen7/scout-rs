//! Y axis of the price plot, drawn by hand so amounts show with coins (egui_plot axes are text only).

use eframe::egui;
use egui_plot::PlotResponse;

use crate::ui::coins;
use crate::ui::theme;

/// Width reserved left of the plot for the labels.
pub const WIDTH: f32 = 84.0;
/// Minimum distance in pixels between two ticks.
const MIN_SPACING: f32 = 30.0;

/// Smallest "nice" step (1, 2 or 5 × 10^k copper) giving at least `MIN_SPACING` pixels between ticks.
fn nice_step(copper_per_px: f64) -> i64 {
    let wanted = (copper_per_px * f64::from(MIN_SPACING)).max(1.0);
    let mut magnitude = 1_i64;
    loop {
        for mult in [1, 2, 5] {
            let step = magnitude * mult;
            if step as f64 >= wanted {
                return step;
            }
        }
        magnitude *= 10;
    }
}

/// Grid lines and coin labels for the visible range; call right after `Plot::show`.
pub fn draw(ui: &mut egui::Ui, plot: &PlotResponse<()>) {
    let frame = *plot.transform.frame();
    let bounds = plot.transform.bounds();
    let (low, high) = (bounds.min()[1] * 10_000.0, bounds.max()[1] * 10_000.0);
    if frame.height() <= 0.0 || high <= low {
        return;
    }
    let step = nice_step((high - low) / f64::from(frame.height()));
    let palette = theme::current(ui);
    let painter = ui.painter_at(frame.expand2(egui::vec2(WIDTH + 4.0, 8.0)));
    let mut tick = (low / step as f64).ceil() as i64 * step;
    while (tick as f64) <= high {
        let y = plot
            .transform
            .position_from_point(&egui_plot::PlotPoint::new(0.0, tick as f64 / 10_000.0))
            .y;
        painter.hline(
            frame.x_range(),
            y,
            egui::Stroke::new(1.0, palette.dim_text.gamma_multiply(0.3)),
        );
        // Measured by an invisible pass of the same drawing, so the label ends right on the axis.
        let mut probe = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(frame)
                .sizing_pass()
                .invisible(),
        );
        let width = coins::money(&mut probe, tick, None).rect.width();
        let area = egui::Rect::from_min_size(
            egui::pos2(frame.left() - width - 4.0, y - 9.0),
            egui::vec2(width, 18.0),
        );
        let mut label = ui.new_child(egui::UiBuilder::new().max_rect(area));
        label.set_clip_rect(area.expand(6.0));
        coins::money(&mut label, tick, None);
        tick += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_one_two_or_five_times_a_power_of_ten() {
        // 30 px at 1 copper per pixel: 30 wanted, 50 given.
        assert_eq!(nice_step(1.0), 50);
        assert_eq!(nice_step(0.01), 1);
        assert_eq!(nice_step(6.0), 200);
        assert_eq!(nice_step(300.0), 10_000);
    }
}
