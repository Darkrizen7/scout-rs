//! The two color themes and the colors panels pick from them.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

use crate::config::Theme;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Colors for one theme; widgets get them through `apply`, panels through `palette`.
pub struct Palette {
    pub dark: bool,
    pub panel: Color32,
    pub window: Color32,
    /// Text fields, plot and table background.
    pub deep: Color32,
    /// Table stripes.
    pub stripe: Color32,
    pub widget: Color32,
    pub hovered: Color32,
    pub pressed: Color32,
    pub border: Color32,
    pub text: Color32,
    pub strong_text: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub gain: Color32,
    pub loss: Color32,
    /// Chart series: Scout, Auctionator, Forever Guide, quantity.
    pub series: [Color32; 3],
}

const SLATE: Palette = Palette {
    dark: true,
    panel: rgb(0x2b313b),
    window: rgb(0x323945),
    deep: rgb(0x222730),
    stripe: rgb(0x2f3640),
    widget: rgb(0x3b4350),
    hovered: rgb(0x48525f),
    pressed: rgb(0x55606f),
    border: rgb(0x4a5361),
    text: rgb(0xd5dae2),
    strong_text: rgb(0xf2f4f7),
    accent: rgb(0xe3b45a),
    selection: rgb(0x6b5427),
    gain: rgb(0x7fd38a),
    loss: rgb(0xf07a6e),
    series: [rgb(0xe3b45a), rgb(0x6fb3e8), rgb(0x8fd39a)],
};

const PARCHMENT: Palette = Palette {
    dark: false,
    panel: rgb(0xf2e8d5),
    window: rgb(0xf8f1e3),
    deep: rgb(0xfffaf0),
    stripe: rgb(0xece0c8),
    widget: rgb(0xe4d5b7),
    hovered: rgb(0xd9c6a0),
    pressed: rgb(0xcbb487),
    border: rgb(0xc4ad84),
    text: rgb(0x3d3122),
    strong_text: rgb(0x1f170d),
    accent: rgb(0x8a5a12),
    selection: rgb(0xe8cf98),
    gain: rgb(0x2e7d32),
    loss: rgb(0xb3261e),
    series: [rgb(0xb07d12), rgb(0x2f6fa8), rgb(0x3d8b4a)],
};

pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Slate => &SLATE,
        Theme::Parchment => &PARCHMENT,
    }
}

/// Palette matching the visuals currently in use.
pub fn current(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &SLATE
    } else {
        &PARCHMENT
    }
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let p = palette(theme);
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.panel;
    v.window_fill = p.window;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.deep;
    v.faint_bg_color = p.stripe;
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.error_fg_color = p.loss;
    v.window_corner_radius = CornerRadius::same(8);

    let radius = CornerRadius::same(5);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill) in [
        (&mut w.inactive, p.widget),
        (&mut w.hovered, p.hovered),
        (&mut w.active, p.pressed),
        (&mut w.open, p.hovered),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = radius;
    }
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.strong_text);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.strong_text);

    ctx.set_theme(if p.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.set_visuals_of(
        if p.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        },
        v,
    );
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
    });
}
