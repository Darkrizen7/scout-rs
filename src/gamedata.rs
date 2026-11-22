//! Game reference data (WoW Forever client tables): every item, the professions and their recipes.
//!
//! Downloaded from wago.tools in French (`download`), assembled into these types (`assemble`),
//! stored in SQLite with the Forever build it comes from (`db::game`), indexed in memory for the UI.

mod assemble;
mod download;

use std::collections::HashMap;

pub use assemble::assemble;
pub use download::{TABLES, latest_build, table_csv};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameItem {
    #[sqlx(rename = "item_id")]
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
}

/// Everything the Objects and Recipes modules show, with lookups.
#[derive(Debug, Default)]
pub struct GameData {
    /// Forever client build the data comes from, e.g. `1.60.1.71037`.
    pub build: String,
    /// When it was downloaded (Unix seconds).
    pub updated: i64,
    /// What the tool kept from the client tables (`db::GAME_FORMAT` when downloaded by this version).
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
}

impl GameData {
    pub fn new(build: String, updated: i64, tables: Tables) -> Self {
        let mut data = Self {
            build,
            updated,
            format: crate::db::GAME_FORMAT,
            items: tables.items.into_iter().map(|i| (i.id, i)).collect(),
            professions: tables.professions,
            recipes: tables.recipes,
            categories: tables.categories,
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

/// Full download from wago.tools into a throwaway database:
/// `cargo test gamedata::online -- --ignored --nocapture`.
#[cfg(test)]
mod online {
    use super::*;

    #[tokio::test]
    #[ignore = "downloads about 12 MB from wago.tools"]
    async fn downloads_assembles_and_stores_latest_build() -> anyhow::Result<()> {
        let build = latest_build()?;
        let mut tables = HashMap::new();
        for table in TABLES {
            tables.insert(table, table_csv(table, &build)?);
        }
        let game = GameData::new(build.clone(), 1, assemble(&tables)?);

        let path = std::env::temp_dir().join(format!("scout-prix-game-{}.db", std::process::id()));
        let db = crate::db::Db::open(&path).await?;
        db.save_game(&game).await?;
        let loaded = db.load_game().await?;
        std::fs::remove_file(&path)?;

        assert_eq!(
            db.game_version().await?,
            Some((build.clone(), crate::db::GAME_FORMAT))
        );
        assert_eq!(
            (loaded.recipes.len(), loaded.items.len()),
            (game.recipes.len(), game.items.len())
        );
        println!(
            "Forever {build}: {} recettes, {} objets, {} métiers",
            loaded.recipes.len(),
            loaded.items.len(),
            loaded.professions.len()
        );
        for p in &loaded.professions {
            println!(
                "  {} : {}",
                p.name,
                loaded
                    .recipes
                    .iter()
                    .filter(|r| r.profession == p.id)
                    .count()
            );
        }
        let linen = 2589;
        println!(
            "{} : utilisé dans {} recettes",
            loaded.item_name(linen),
            loaded.used_in.get(&linen).map_or(0, Vec::len)
        );
        let boots = loaded
            .recipes
            .iter()
            .find(|r| r.spell_id == 1301456)
            .expect("recette Forever");
        println!(
            "{} : requis {:?}, composants {:?}, patron {:?}",
            boots.name, boots.required, boots.reagents, boots.taught_by
        );
        Ok(())
    }
}
