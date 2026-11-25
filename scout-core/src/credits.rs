//! Where the data comes from: one list shared by the app's "Sources" window and the server's
//! public `/thanks` page.

/// One data source, credited out of courtesy to the people who share it.
pub struct Source {
    pub name: &'static str,
    pub url: Option<&'static str>,
    /// What the tool takes from it (French, shown as is).
    pub what: &'static str,
}

pub const SOURCES: [Source; 6] = [
    Source {
        name: "wago.tools",
        url: Some("https://wago.tools"),
        what: "Tables du client WoW Forever, en français : objets, recettes, métiers, cartes.",
    },
    Source {
        name: "Wowhead",
        url: Some("https://www.wowhead.com/classic/fr"),
        what: "Objets absents de ces tables (envoyés par le serveur de Forever, surtout repris de \
         Season of Discovery) : nom, qualité, niveaux, revente.",
    },
    Source {
        name: "cmangos classic-db et QuestieDB",
        url: Some("https://github.com/cmangos/classic-db"),
        what: "Bases d'où vient l'essentiel du butin de la base de référence incluse dans l'outil \
         (chances, PNJ, noms français).",
    },
    Source {
        name: "API Battle.net de Blizzard Entertainment",
        url: Some("https://develop.battle.net"),
        what: "Prix de l'hôtel des ventes (source « Scout API »), relevés par notre serveur de prix. \
         Données fournies « en l'état » ; outil non approuvé par Blizzard Entertainment et non \
         affilié à Blizzard Entertainment.",
    },
    Source {
        name: "OpenMap",
        url: Some("https://github.com/openmap-world/openmap"),
        what: "Addon : contours des zones de la carte (lus dans le dossier de l'addon, jamais copiés).",
    },
    Source {
        name: "Inter",
        url: Some("https://rsms.me/inter/"),
        what: "Police de l'interface (licence SIL Open Font License).",
    },
];
