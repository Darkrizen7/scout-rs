//! Game reference data (WoW Forever client tables): every item, the professions and their recipes.
//!
//! Downloaded from wago.tools in French and assembled into these types (`scout-sources`), stored in SQLite
//! with the Forever build it comes from (`scout-db`), indexed in memory for the UI.

use std::collections::HashMap;

use crate::map::{Faction, GameMap};

/// Version of what is stored from the client tables: bumped when new columns are kept,
/// so the next update downloads again even if the Forever build did not change.
/// 2: merchant buy / sell prices. 3: item category names. 4: maps. 5: recipe items from Wowhead. 6: zone factions.
pub const GAME_FORMAT: i64 = 6;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct GameItem {
    #[cfg_attr(feature = "sqlx", sqlx(rename = "item_id"))]
    pub id: i64,
    pub name: String,
    /// 0 poor (grey) … 5 legendary.
    pub quality: i64,
    pub item_level: i64,
    pub required_level: i64,
    pub class_id: Option<i64>,
    pub subclass_id: Option<i64>,
    /// Merchant price of a lot of `vendor_stack` (copper). Every item has one, even those no merchant sells.
    pub buy_price: i64,
    pub vendor_stack: i64,
    /// What a merchant pays for one (copper).
    pub sell_price: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Profession {
    /// Skill line id (171 Alchimie, 164 Forge…).
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reagent {
    pub item_id: i64,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Recipe {
    pub spell_id: i64,
    pub name: String,
    pub profession: i64,
    /// Skill needed to learn it, when known (recipe item, or trainer minimum above 1).
    pub required: Option<i64>,
    /// Skill-up colors: yellow from `yellow`, grey from `grey`.
    pub yellow: i64,
    pub grey: i64,
    /// Item made and how many (None for enchantments and the like).
    pub creates: Option<Reagent>,
    pub reagents: Vec<Reagent>,
    /// Items (patterns, plans, formulas…) that teach it.
    pub taught_by: Vec<i64>,
}

impl Recipe {
    /// Skill from which the recipe turns green: the game puts it halfway between yellow and grey.
    pub fn green(&self) -> i64 {
        (self.yellow + self.grey) / 2
    }
}

/// An item category: a class (`subclass_id` None) or one of its subclasses, named in French.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub class_id: i64,
    pub subclass_id: Option<i64>,
    pub name: String,
}

/// What the client tables give, before indexing.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Tables {
    pub items: Vec<GameItem>,
    pub professions: Vec<Profession>,
    pub recipes: Vec<Recipe>,
    pub categories: Vec<Category>,
    pub maps: Vec<GameMap>,
    /// Zone name (`map::name_key`) → faction, for the zones that have one.
    pub factions: HashMap<String, Faction>,
    /// Category (class, subclass) of the items `Item` lists but `ItemSparse` lacks: the
    /// hotfixed ones, named elsewhere (Wowhead).
    pub unnamed_categories: HashMap<i64, (i64, i64)>,
}

/// Everything the Objects and Recipes modules show, with lookups.
#[derive(Debug, Default)]
pub struct GameData {
    /// Forever client build the data comes from, e.g. `1.60.1.71037`.
    pub build: String,
    /// When it was downloaded (Unix seconds).
    pub updated: i64,
    /// What the tool kept from the client tables (`GAME_FORMAT` when downloaded by this version).
    pub format: i64,
    /// False when the download stopped short (Wowhead refused some items): the next update runs again.
    pub complete: bool,
    pub items: HashMap<i64, GameItem>,
    pub professions: Vec<Profession>,
    pub recipes: Vec<Recipe>,
    /// item → recipes (indices) that make it.
    pub made_by: HashMap<i64, Vec<usize>>,
    /// item → recipes (indices) that use it as a reagent.
    pub used_in: HashMap<i64, Vec<usize>>,
    /// item → recipes (indices) it teaches.
    pub teaches: HashMap<i64, Vec<usize>>,
    pub categories: Vec<Category>,
    /// Continents and zones (Loot module's map).
    pub maps: Vec<GameMap>,
    /// Zone name (`map::name_key`) → faction, for the zones that have one.
    pub factions: HashMap<String, Faction>,
}

impl Tables {
    /// Items recipes make, use or are taught by, missing from the client tables (sorted).
    pub fn missing_recipe_items(&self) -> Vec<i64> {
        let known: std::collections::HashSet<i64> = self.items.iter().map(|i| i.id).collect();
        let mut missing: Vec<i64> = self
            .recipes
            .iter()
            .flat_map(|r| {
                r.creates
                    .iter()
                    .chain(&r.reagents)
                    .map(|x| x.item_id)
                    .chain(r.taught_by.iter().copied())
            })
            .filter(|id| !known.contains(id))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        missing
    }

    /// Adds items named elsewhere, keeping Forever's category when the client tables have one.
    pub fn add_items(&mut self, items: impl IntoIterator<Item = GameItem>) {
        for mut item in items {
            if let Some(&(class, subclass)) = self.unnamed_categories.get(&item.id) {
                (item.class_id, item.subclass_id) = (Some(class), Some(subclass));
            }
            self.items.push(item);
        }
    }
}

impl GameData {
    /// Faction of a zone by its name (None: contested, a dungeon, or unknown).
    pub fn faction_of(&self, zone: &str) -> Option<Faction> {
        self.factions.get(&crate::map::name_key(zone)).copied()
    }

    pub fn new(build: String, updated: i64, tables: Tables) -> Self {
        let mut data = Self {
            build,
            updated,
            format: GAME_FORMAT,
            complete: true,
            items: tables.items.into_iter().map(|i| (i.id, i)).collect(),
            professions: tables.professions,
            recipes: tables.recipes,
            categories: tables.categories,
            maps: tables.maps,
            factions: tables.factions,
            ..Self::default()
        };
        for (index, recipe) in data.recipes.iter().enumerate() {
            if let Some(made) = recipe.creates {
                data.made_by.entry(made.item_id).or_default().push(index);
            }
            for reagent in &recipe.reagents {
                data.used_in.entry(reagent.item_id).or_default().push(index);
            }
            for &item in &recipe.taught_by {
                data.teaches.entry(item).or_default().push(index);
            }
        }
        data
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    pub fn item_name(&self, item_id: i64) -> String {
        self.items
            .get(&item_id)
            .map_or_else(|| format!("objet #{item_id}"), |i| i.name.clone())
    }

    /// Name of a class (`subclass_id` None) or subclass.
    pub fn category_name(&self, class_id: i64, subclass_id: Option<i64>) -> Option<&str> {
        self.categories
            .iter()
            .find(|c| c.class_id == class_id && c.subclass_id == subclass_id)
            .map(|c| c.name.as_str())
    }

    pub fn profession_name(&self, id: i64) -> &str {
        self.professions
            .iter()
            .find(|p| p.id == id)
            .map_or("?", |p| p.name.as_str())
    }
}

/// Marks Blizzard puts anywhere in the names of items left in the tables but not in the game, lower case:
/// deprecated, unused, placeholder (`[PH]`), old version, "do not translate" (internal).
const OBSOLETE_WORDS: [&str; 9] = [
    "deprecated",
    "unused",
    "inutilisé",
    "périmé",
    "[ph]",
    "(old)",
    "[old]",
    "(dnt)",
    "[dnt]",
];

/// Name starts of the same kind, case kept: `OLD` glued to the old version's name, `Monster -` for the weapons
/// monsters hold (only shown, never looted).
const OBSOLETE_PREFIXES: [&str; 3] = ["OLD", "Monster -", "DNT "];

/// Whether an item name marks it as never obtainable (obsolete, placeholder, monster model…).
pub fn is_obsolete(name: &str) -> bool {
    let lower = name.to_lowercase();
    OBSOLETE_WORDS.iter().any(|w| lower.contains(w))
        || OBSOLETE_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_is_halfway_between_yellow_and_grey() {
        let recipe = Recipe {
            spell_id: 1,
            name: String::new(),
            profession: 171,
            required: None,
            yellow: 50,
            grey: 90,
            creates: None,
            reagents: vec![],
            taught_by: vec![],
        };
        assert_eq!(recipe.green(), 70);
    }

    #[test]
    fn recipe_items_missing_from_the_tables() {
        let item = |id| GameItem {
            id,
            name: String::new(),
            quality: 1,
            item_level: 0,
            required_level: 0,
            class_id: None,
            subclass_id: None,
            buy_price: 0,
            vendor_stack: 1,
            sell_price: 0,
        };
        let reagent = |item_id| Reagent { item_id, count: 1 };
        let tables = Tables {
            items: vec![item(1), item(2)],
            recipes: vec![Recipe {
                spell_id: 10,
                name: String::new(),
                profession: 165,
                required: None,
                yellow: 0,
                grey: 0,
                creates: Some(reagent(5)),
                reagents: vec![reagent(1), reagent(4), reagent(4)],
                taught_by: vec![2, 3],
            }],
            ..Tables::default()
        };
        assert_eq!(tables.missing_recipe_items(), vec![3, 4, 5]);

        let mut tables = Tables {
            unnamed_categories: HashMap::from([(4, (7, 6))]),
            ..tables
        };
        let wowhead = |id| GameItem {
            class_id: Some(7),
            subclass_id: Some(0),
            ..item(id)
        };
        tables.add_items([wowhead(4), wowhead(5)]);
        let category = |id| {
            let i = tables.items.iter().find(|i| i.id == id).unwrap();
            (i.class_id, i.subclass_id)
        };
        assert_eq!(category(4), (Some(7), Some(6)));
        assert_eq!(category(5), (Some(7), Some(0)));
    }

    #[test]
    fn obsolete_names() {
        for name in [
            "Deprecated Book of Cyclone",
            "Unused Black Night Elf Pants",
            "Torka's Egg Cracker [UNUSED]",
            "[INUTILISÉ] Fiole rayée",
            "Gantelets en mithril électrifiés INUTILISÉS",
            "Poison périmé",
            "[PH] Hakkar'i Urn",
            "OLDRecruit's Belt",
            "(OLD)Medium Throwing Knife",
            "Large Moneybag (old)",
            "Monster - Sword, Short Rusty",
            "Monster Weapon [DNT]",
            "Hidden Belt (DNT)",
            "DNT HEAD",
        ] {
            assert!(is_obsolete(name), "{name}");
        }
        for name in [
            "Ration de la semaine passée",
            "Charge explosive",
            "Lingot de fer",
            "Old Blanchy",
            "Bourse d'or",
        ] {
            assert!(!is_obsolete(name), "{name}");
        }
    }
}
