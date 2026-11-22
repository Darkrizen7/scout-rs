//! Pan and zoom of the price / quantity plots, kept inside the data: nothing below zero, nothing
//! beyond the first and last data day (plus a margin so the ends stay readable).
//!
//! egui_plot has no such limit and applies its own interactions after the build closure, so a
//! clamp done there would show one frame of overshoot. The plots therefore turn their built-in
//! interactions off and `navigate` does them, clamped, before anything is drawn.

use eframe::egui::{self, PointerButton};
use egui_plot::PlotUi;
use scout_core::merge::Point;

pub struct Limits {
    /// Time range allowed on the x axis, in seconds.
    time: Option<(f64, f64)>,
}

impl Limits {
    /// Smallest margin on each side, in seconds (half a day).
    const MIN_MARGIN: f64 = 12.0 * 3600.0;

    pub fn of<'a>(points: impl Iterator<Item = &'a Point>) -> Self {
        let (min, max) = points.fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.ts as f64), hi.max(p.ts as f64))
        });
        let time = (min <= max).then(|| {
            let margin = ((max - min) * 0.05).max(Self::MIN_MARGIN);
            (min - margin, max + margin)
        });
        Self { time }
    }

    /// Applies drag, scroll and zoom to the visible area. Past the limits the view still follows
    /// the pointer but ever more slowly; once the input stops it eases back inside.
    /// Axes still fitted automatically are left alone: they already show the data.
    ///
    /// The time axis is computed once per frame, by the price plot (drawn first, over both plots'
    /// area), and copied by the quantity plot so both move together.
    pub fn navigate(&self, plot: &mut PlotUi<'_>, role: Role, points: &[Point]) {
        let ctx = plot.ctx().clone();
        let id = nav_id();
        let pass = ctx.cumulative_pass_nr();
        let mut shared: Shared = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
        let frame = *plot.transform().frame();
        shared.rects[role as usize] = frame;
        let bounds = plot.plot_bounds();
        let auto = plot.auto_bounds();
        let prev = [
            (bounds.min()[0], bounds.max()[0]),
            (bounds.min()[1], bounds.max()[1]),
        ];
        if prev
            .iter()
            .any(|(lo, hi)| !(lo.is_finite() && hi.is_finite() && lo < hi))
        {
            ctx.data_mut(|d| d.insert_temp(id, shared));
            return;
        }
        let allowed = [self.time.unwrap_or((f64::MIN, f64::MAX)), (0.0, f64::MAX)];
        let input = Input::read(&ctx);
        let in_any = shared
            .rects
            .iter()
            .any(|r| input.hover.is_some_and(|p| r.contains(p)));
        let in_own = input.hover.is_some_and(|p| frame.contains(p));
        let drag_any = input.dragging
            && input
                .origin
                .is_some_and(|p| shared.rects.iter().any(|r| r.contains(p)));
        let drag_own = input.dragging && input.origin.is_some_and(|p| frame.contains(p));

        // "Aujourd'hui" or a period button was clicked in this very frame: move to the end of the data.
        let reset = ctx.data(|d| d.get_temp::<u64>(end_id())) == Some(pass);
        let mut next = prev;
        let mut active = [false; 2];
        // Time axis: the price plot works it out, the quantity plot follows.
        if role == Role::Quantity && shared.pass == pass {
            next[0] = shared.x;
        } else {
            let mut shift = 0.0;
            if drag_any {
                shift -= f64::from(input.delta.x);
            }
            if in_any {
                shift -= f64::from(input.scroll.x);
            }
            if shift != 0.0 {
                let d = shift * (prev[0].1 - prev[0].0) / f64::from(frame.width());
                let k = resistance(prev[0], allowed[0], d);
                next[0] = (prev[0].0 + d * k, prev[0].1 + d * k);
                active[0] = true;
            }
            if let (Some(pos), true) = (input.hover, in_any && input.zoom.x != 1.0) {
                let c = plot.plot_from_screen(pos).x;
                next[0] = zoom_range(next[0], c, input.zoom.x);
                let max_width = allowed[0].1 - allowed[0].0;
                if next[0].1 - next[0].0 > max_width {
                    let mid = (next[0].0 + next[0].1) / 2.0;
                    next[0] = (mid - max_width / 2.0, mid + max_width / 2.0);
                }
                active[0] = true;
            }
            if let (true, Some((_, hi))) = (reset, self.time) {
                // Same time scale, right edge on the last data day.
                let wanted = ctx.data(|d| d.get_temp::<f64>(width_id()));
                let width = wanted
                    .unwrap_or(next[0].1 - next[0].0)
                    .min(allowed[0].1 - allowed[0].0);
                next[0] = (hi - width, hi);
                active[0] = true;
            }
            active[0] |= drag_any;
            if !active[0] && !auto.x {
                next[0] = ease_back(next[0], allowed[0], input.dt, &ctx);
            }
            shared.pass = pass;
            shared.x = next[0];
        }
        // Value axis: only the plot under the pointer.
        let mut shift = 0.0;
        if drag_own {
            shift -= f64::from(input.delta.y);
        }
        if in_own {
            shift -= f64::from(input.scroll.y);
        }
        if shift != 0.0 {
            let d = -shift * (prev[1].1 - prev[1].0) / f64::from(frame.height());
            let k = resistance(prev[1], allowed[1], d);
            next[1] = (prev[1].0 + d * k, prev[1].1 + d * k);
            active[1] = true;
        }
        if let (Some(pos), true) = (input.hover, in_own && input.zoom.y != 1.0) {
            let c = plot.plot_from_screen(pos).y;
            next[1] = zoom_range(next[1], c, input.zoom.y);
            active[1] = true;
        }
        if reset {
            // Zoom the value axis out when some visible point would be off screen.
            let (from, to) = (next[0].0, next[0].1);
            let visible = points
                .iter()
                .filter(|p| (from..=to).contains(&(p.ts as f64)));
            let (lo, hi) = visible.fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                (lo.min(p.value), hi.max(p.value))
            });
            if lo <= hi {
                let pad = (hi - lo).max(hi * 0.1) * 0.08;
                next[1] = (next[1].0.min((lo - pad).max(0.0)), next[1].1.max(hi + pad));
                active[1] = true;
            }
        }
        active[1] |= drag_own;
        if !active[1] && !auto.y {
            next[1] = ease_back(next[1], allowed[1], input.dt, &ctx);
        }
        ctx.data_mut(|d| d.insert_temp(id, shared));
        let auto = [auto.x, auto.y];
        for axis in 0..2 {
            if next[axis] != prev[axis]
                && (!auto[axis] || active[axis] || axis == 0 && role == Role::Quantity)
            {
                let range = next[axis].0..=next[axis].1;
                if axis == 0 {
                    plot.set_plot_bounds_x(range);
                } else {
                    plot.set_plot_bounds_y(range);
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Price = 0,
    Quantity = 1,
}

/// Temporary data shared by the two plots within a frame (`Shared`).
fn nav_id() -> egui::Id {
    egui::Id::NULL.with("scout-rs/chart-nav")
}

/// Pass in which the view was asked to move to the end of the data (`show_end`, `show_last`).
fn end_id() -> egui::Id {
    egui::Id::NULL.with("scout-rs/chart-end")
}

/// Time window asked by `show_last`, in seconds.
fn width_id() -> egui::Id {
    egui::Id::NULL.with("scout-rs/chart-width")
}

/// Asks the plots to show the end of the curve: same time scale, value scale zoomed out if needed.
pub fn show_end(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<f64>(width_id()));
    request_end(ctx);
}

/// Like `show_end`, but with a time window of `seconds` ending on the last data day.
pub fn show_last(ctx: &egui::Context, seconds: f64) {
    ctx.data_mut(|d| d.insert_temp(width_id(), seconds));
    request_end(ctx);
}

fn request_end(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(end_id(), pass));
    ctx.request_repaint();
}

/// What the two plots share within one frame.
#[derive(Clone, Copy)]
struct Shared {
    pass: u64,
    x: (f64, f64),
    rects: [egui::Rect; 2],
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            pass: u64::MAX,
            x: (0.0, 1.0),
            rects: [egui::Rect::NOTHING; 2],
        }
    }
}

struct Input {
    hover: Option<egui::Pos2>,
    origin: Option<egui::Pos2>,
    dragging: bool,
    delta: egui::Vec2,
    scroll: egui::Vec2,
    zoom: egui::Vec2,
    dt: f64,
}

impl Input {
    fn read(ctx: &egui::Context) -> Self {
        ctx.input(|i| Self {
            hover: i.pointer.latest_pos(),
            origin: i.pointer.press_origin(),
            dragging: i.pointer.button_down(PointerButton::Primary)
                && i.pointer.is_decidedly_dragging(),
            delta: i.pointer.delta(),
            scroll: i.smooth_scroll_delta,
            zoom: i.zoom_delta_2d(),
            dt: f64::from(i.stable_dt.min(0.1)),
        })
    }
}

fn zoom_range((lo, hi): (f64, f64), center: f64, zoom: f32) -> (f64, f64) {
    let z = f64::from(zoom);
    (center + (lo - center) / z, center + (hi - center) / z)
}

/// One step of the ease back inside the limits (exponential, about 0.2 s); repaints while moving.
fn ease_back(
    (lo, hi): (f64, f64),
    (min, max): (f64, f64),
    dt: f64,
    ctx: &egui::Context,
) -> (f64, f64) {
    let width = hi - lo;
    let goal = lo.clamp(min, (max - width).max(min));
    if (goal - lo).abs() <= 1e-9 {
        return (lo, hi);
    }
    ctx.request_repaint();
    let left = if (goal - lo).abs() < width * 1e-3 {
        goal
    } else {
        lo + (goal - lo) * (1.0 - (-12.0 * dt).exp())
    };
    (left, left + width)
}

/// Share of a move that goes through: 1 inside the limits or when moving back in, fading out
/// the further the view is already past a limit.
fn resistance((lo, hi): (f64, f64), (min, max): (f64, f64), delta: f64) -> f64 {
    let width = hi - lo;
    let soft = (width * 0.2).max(f64::EPSILON);
    let over = if delta < 0.0 { min - lo } else { hi - max };
    if over > 0.0 {
        (-over / soft).exp()
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(ts: i64) -> Point {
        Point {
            ts,
            value: 1.0,
            origin: scout_core::merge::Origin::Scout,
        }
    }

    #[test]
    fn time_limits_keep_a_margin() {
        let day = 86_400.0;
        let points = [point(0), point(100 * 86_400)];
        let (lo, hi) = Limits::of(points.iter()).time.unwrap();
        // 5 % of the range on each side.
        assert_eq!((lo, hi), (-5.0 * day, 105.0 * day));
        // A single point still gets half a day on each side.
        let (lo, hi) = Limits::of([point(0)].iter()).time.unwrap();
        assert_eq!((lo, hi), (-Limits::MIN_MARGIN, Limits::MIN_MARGIN));
        assert!(Limits::of(std::iter::empty()).time.is_none());
    }

    #[test]
    fn zoom_keeps_the_center_in_place() {
        assert_eq!(zoom_range((0.0, 10.0), 5.0, 2.0), (2.5, 7.5));
        assert_eq!(zoom_range((0.0, 10.0), 0.0, 0.5), (0.0, 20.0));
    }

    #[test]
    fn resistance_grows_past_the_limits() {
        let allowed = (0.0, 100.0);
        // Inside, or moving back in: the whole move goes through.
        assert_eq!(resistance((10.0, 20.0), allowed, -5.0), 1.0);
        assert_eq!(resistance((-5.0, 5.0), allowed, 2.0), 1.0);
        // Past the left limit and moving further out: ever less.
        let a = resistance((-1.0, 9.0), allowed, -1.0);
        let b = resistance((-5.0, 5.0), allowed, -1.0);
        assert!(1.0 > a && a > b && b > 0.0);
    }

    #[test]
    fn ease_back_moves_toward_the_limits() {
        let ctx = egui::Context::default();
        let allowed = (0.0, 100.0);
        // Already inside: unchanged.
        assert_eq!(ease_back((10.0, 20.0), allowed, 0.1, &ctx), (10.0, 20.0));
        // Past the right limit: back left, same width, not all the way at once.
        let (lo, hi) = ease_back((95.0, 105.0), allowed, 0.016, &ctx);
        assert!((hi - lo - 10.0).abs() < 1e-9);
        assert!(lo < 95.0 && lo > 90.0);
        // Close enough: snaps onto the limit.
        assert_eq!(
            ease_back((90.005, 100.005), allowed, 0.016, &ctx),
            (90.0, 100.0)
        );
    }
}
