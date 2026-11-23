//! "Sources des données" window: where the data comes from, with links (out of courtesy to the
//! people who share it). Opened from the settings.

use eframe::egui;

/// (name, link, what the tool takes from it).
const SOURCES: [(&str, Option<&str>, &str); 7] = [
    (
        "wago.tools",
        Some("https://wago.tools"),
        "Tables du client WoW Forever, en français : objets, recettes, métiers, cartes.",
    ),
    (
        "Wowhead",
        Some("https://www.wowhead.com/classic/fr"),
        "Objets absents de ces tables (envoyés par le serveur de Forever, surtout repris de \
         Season of Discovery) : nom, qualité, niveaux, revente.",
    ),
    (
        "Forever Guide",
        None,
        "Addon : butin des monstres et des coffres, prix des marchands, dernier prix vu à l'hôtel des ventes. \
         Son butin est inclus dans l'outil (base de référence, GPL-3.0).",
    ),
    (
        "cmangos classic-db et QuestieDB",
        Some("https://github.com/cmangos/classic-db"),
        "Bases d'où Forever Guide tire l'essentiel de son butin (chances, PNJ, noms français).",
    ),
    (
        "Auctionator",
        Some("https://www.curseforge.com/wow/addons/auctionator"),
        "Addon : historique des prix de l'hôtel des ventes.",
    ),
    (
        "OpenMap",
        Some("https://github.com/openmap-world/openmap"),
        "Addon : contours des zones de la carte (lus dans le dossier de l'addon, jamais copiés).",
    ),
    (
        "Inter",
        Some("https://rsms.me/inter/"),
        "Police de l'interface (licence SIL Open Font License).",
    ),
];

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
            for (name, url, what) in SOURCES {
                ui.add_space(8.0);
                match url {
                    Some(url) => ui.hyperlink_to(egui::RichText::new(name).strong(), url),
                    None => ui.strong(name),
                };
                ui.weak(what);
            }
        });
}
