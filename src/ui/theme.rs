//! GNOME / libadwaita colors (as in the Files app), light and dark, and the colors panels pick from.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

use crate::config::Theme;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub struct Palette {
    pub dark: bool,
    /// Main content (lists, charts).
    pub view: Color32,
    /// Side bar on the left.
    pub sidebar: Color32,
    /// Header bar and dialogs.
    pub header: Color32,
    /// Buttons, text fields.
    pub widget: Color32,
    /// Hovered row or button.
    pub hover: Color32,
    /// Hovered / selected side bar entry.
    pub sidebar_hover: Color32,
    pub sidebar_selected: Color32,
    /// Selected list row.
    pub row_selected: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim_text: Color32,
    pub accent: Color32,
    pub star: Color32,
    pub gain: Color32,
    pub loss: Color32,
    /// Chart series: Scout, Auctionator, Forever Guide.
    pub series: [Color32; 3],
}

const LIGHT: Palette = Palette {
    dark: false,
    view: rgb(0xffffff),
    sidebar: rgb(0xebebed),
    header: rgb(0xfafafb),
    widget: rgb(0xe8e8ea),
    hover: rgb(0xf0f0f2),
    sidebar_hover: rgb(0xe2e2e4),
    sidebar_selected: rgb(0xdadadc),
    row_selected: rgb(0xdbe7f8),
    border: rgb(0xdededf),
    text: rgb(0x2e2e32),
    dim_text: rgb(0x77777b),
    accent: rgb(0x3584e4),
    star: rgb(0xe5a50a),
    gain: rgb(0x26a269),
    loss: rgb(0xc01c28),
    series: [rgb(0x3584e4), rgb(0xe66100), rgb(0x2ec27e)],
};

const DARK: Palette = Palette {
    dark: true,
    view: rgb(0x1d1d20),
    sidebar: rgb(0x2e2e32),
    header: rgb(0x2a2a2e),
    widget: rgb(0x3a3a3e),
    hover: rgb(0x2b2b2f),
    sidebar_hover: rgb(0x38383d),
    sidebar_selected: rgb(0x46464b),
    row_selected: rgb(0x26374d),
    border: rgb(0x3a3a3e),
    text: rgb(0xf0f0f2),
    dim_text: rgb(0x9a9a9e),
    accent: rgb(0x78aeed),
    star: rgb(0xf6d32d),
    gain: rgb(0x8ff0a4),
    loss: rgb(0xf66151),
    series: [rgb(0x78aeed), rgb(0xffa348), rgb(0x57e389)],
};

pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Light => &LIGHT,
        Theme::Dark => &DARK,
    }
}

/// Palette matching the visuals currently in use.
pub fn current(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let p = palette(theme);
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.view;
    v.window_fill = p.header;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.extreme_bg_color = p.widget;
    v.text_edit_bg_color = Some(p.widget);
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.row_selected;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.error_fg_color = p.loss;
    v.override_text_color = Some(p.text);

    let radius = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.view;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill) in [
        (&mut w.inactive, p.widget),
        (&mut w.hovered, p.sidebar_selected),
        (&mut w.active, p.sidebar_selected),
        (&mut w.open, p.widget),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = radius;
        state.bg_stroke = Stroke::NONE;
        state.fg_stroke = Stroke::new(1.0, p.text);
    }
    w.active.bg_stroke = Stroke::new(1.0, p.accent);

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
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size.y = 26.0;
        // Selectable labels swallow clicks meant for table rows.
        style.interaction.selectable_labels = false;
    });
}
