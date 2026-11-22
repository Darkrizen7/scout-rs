//! Inter (SIL OFL 1.1, see `assets/fonts/Inter-LICENSE.txt`) for the interface text,
//! with egui's bundled fonts kept as fallback for symbols and emoji icons.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");

/// Family used for headings.
fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

/// Text in the semibold face (egui's `strong()` only changes the color).
pub fn bold(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(heading_family())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("inter".into(), Arc::new(FontData::from_static(REGULAR)));
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(SEMIBOLD)),
    );

    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    let mut heading = vec!["inter-semibold".to_owned()];
    heading.extend(fallback);
    fonts.families.insert(heading_family(), heading);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, heading_family())),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
    });
}
