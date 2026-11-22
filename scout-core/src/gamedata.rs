//! Game reference data (WoW Forever client tables): every item, the professions and their recipes.
//!
//! Downloaded from wago.tools in French and assembled into these types (`scout-sources`), stored in SQLite
//! with the Forever build it comes from (`scout-db`), indexed in memory for the UI.

use std::collections::HashMap;

use crate::map::GameMap;

/// Version of what is stored from the client tables: bumped when new columns are kept,
/// so the next update downloads again even if the Forever build did not change.
/// 2: merchant buy / sell prices. 3: item category names. 4: maps.
pub const GAME_FORMAT: i64 = 4;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Profession {
    /// Skill line id (171 Alchimie, 164 Forge…).
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reagent {
    pub item_id: i64,
    pub count: i64,
}

#[derive(Debug, Clone)]
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

/// An item category: a class (`subclass_id` None) or one of its subclasses, named in French.
#[derive(Debug, Clone)]
pub struct Category {
    pub class_id: i64,
    pub subclass_id: Option<i64>,
    pub name: String,
}

/// What the client tables give, before indexing.
#[derive(Debug, Default)]
pub struct Tables {
    pub items: Vec<GameItem>,
    pub professions: Vec<Profession>,
    pub recipes: Vec<Recipe>,
    pub categories: Vec<Category>,
    pub maps: Vec<GameMap>,
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
}

impl GameData {
    pub fn new(build: String, updated: i64, tables: Tables) -> Self {
        let mut data = Self {
            build,
            updated,
            format: GAME_FORMAT,
            items: tables.items.into_iter().map(|i| (i.id, i)).collect(),
            professions: tables.professions,
            recipes: tables.recipes,
            categories: tables.categories,
            maps: tables.maps,
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

/// Words Blizzard puts in the names of items left in the tables but not in the game
/// ("Deprecated …", "Unused …", "[INUTILISÉ] …", "Poison périmé").
const OBSOLETE_WORDS: [&str; 4] = ["deprecated", "unused", "inutilisé", "périmé"];

/// Whether an item name marks it as obsolete (never obtainable).
pub fn is_obsolete(name: &str) -> bool {
    let name = name.to_lowercase();
    OBSOLETE_WORDS.iter().any(|w| name.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_names() {
        for name in [
            "Deprecated Book of Cyclone",
            "Unused Black Night Elf Pants",
            "Torka's Egg Cracker [UNUSED]",
            "[INUTILISÉ] Fiole rayée",
            "Gantelets en mithril électrifiés INUTILISÉS",
            "Poison périmé",
        ] {
            assert!(is_obsolete(name), "{name}");
        }
        for name in [
            "Ration de la semaine passée",
            "Charge explosive",
            "Lingot de fer",
        ] {
            assert!(!is_obsolete(name), "{name}");
        }
    }
}
