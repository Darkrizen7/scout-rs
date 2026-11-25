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
        "Addon dont dérivent le butin des monstres et des coffres et la liste des marchands de la base de \
         référence incluse dans l'outil (GPL-3.0). L'outil ne lit pas l'addon.",
    ),
    (
        "cmangos classic-db et QuestieDB",
        Some("https://github.com/cmangos/classic-db"),
        "Bases d'où Forever Guide tire l'essentiel de son butin (chances, PNJ, noms français).",
    ),
    (
        "API Battle.net de Blizzard Entertainment",
        Some("https://develop.battle.net"),
        "Prix de l'hôtel des ventes (source « Scout API »), relevés par notre serveur de prix. \
         Données fournies « en l'état » ; outil non approuvé par Blizzard Entertainment et non \
         affilié à Blizzard Entertainment.",
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
