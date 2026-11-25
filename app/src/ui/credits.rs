//! "Sources des données" window: where the data comes from, with links (out of courtesy to the
//! people who share it). Opened from the settings.

use eframe::egui;
use scout_core::credits::SOURCES;

pub fn window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Sources des données")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.label(
                "Les prix, noms, marchands et butin réels viennent de l'addon Scout. \
                 Le reste vient de ces sources, merci à elles :",
            );
            for source in SOURCES {
                let (name, url, what) = (source.name, source.url, source.what);
                ui.add_space(8.0);
                match url {
                    Some(url) => ui.hyperlink_to(egui::RichText::new(name).strong(), url),
                    None => ui.strong(name),
                };
                ui.weak(what);
            }
        });
}
