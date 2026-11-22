//! Game reference data in SQLite, replaced as a whole at each update, with the Forever build it comes from.

use std::collections::HashMap;

use anyhow::Result;

use super::Db;
use crate::gamedata::{Category, GameData, GameItem, Profession, Reagent, Recipe, Tables};

/// Version of what is stored from the client tables: bumped when new columns are kept,
/// so the next update downloads again even if the Forever build did not change.
/// 2: merchant buy / sell prices. 3: item category names.
pub const GAME_FORMAT: i64 = 3;

/// Columns added to `game_items` after its creation: (name, statement adding it when missing).
const ITEM_COLUMNS: [(&str, &str); 3] = [
    (
        "buy_price",
        "ALTER TABLE game_items ADD COLUMN buy_price INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "vendor_stack",
        "ALTER TABLE game_items ADD COLUMN vendor_stack INTEGER NOT NULL DEFAULT 1",
    ),
    (
        "sell_price",
        "ALTER TABLE game_items ADD COLUMN sell_price INTEGER NOT NULL DEFAULT 0",
    ),
];

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS game_meta (
    key   TEXT PRIMARY KEY,  -- build (Forever client build), updated (Unix seconds), format (GAME_FORMAT)
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS game_items (
    item_id        INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    quality        INTEGER NOT NULL,
    item_level     INTEGER NOT NULL,
    required_level INTEGER NOT NULL,
    class_id       INTEGER,
    subclass_id    INTEGER,
    buy_price      INTEGER NOT NULL DEFAULT 0,
    vendor_stack   INTEGER NOT NULL DEFAULT 1,
    sell_price     INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS game_categories (
    class_id    INTEGER NOT NULL,
    subclass_id INTEGER NOT NULL,  -- -1 for the class itself
    name        TEXT NOT NULL,
    PRIMARY KEY (class_id, subclass_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS game_professions (
    id   INTEGER PRIMARY KEY,  -- skill line
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS game_recipes (
    spell_id      INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    profession    INTEGER NOT NULL,
    required      INTEGER,
    yellow        INTEGER NOT NULL,
    grey          INTEGER NOT NULL,
    creates_item  INTEGER,
    creates_count INTEGER
);
CREATE TABLE IF NOT EXISTS game_reagents (
    spell_id INTEGER NOT NULL,
    slot     INTEGER NOT NULL,
    item_id  INTEGER NOT NULL,
    count    INTEGER NOT NULL,
    PRIMARY KEY (spell_id, slot)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS game_recipe_items (
    spell_id INTEGER NOT NULL,  -- recipe
    item_id  INTEGER NOT NULL,  -- pattern / plan / formula teaching it
    PRIMARY KEY (spell_id, item_id)
) WITHOUT ROWID;
";

impl Db {
    /// Adds the `game_items` columns missing from an older database.
    pub(super) async fn migrate_game(pool: &sqlx::SqlitePool) -> Result<()> {
        let existing: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('game_items')")
                .fetch_all(pool)
                .await?;
        for (column, add) in ITEM_COLUMNS {
            if !existing.iter().any(|(name,)| name == column) {
                sqlx::raw_sql(add).execute(pool).await?;
            }
        }
        Ok(())
    }

    /// Forever build and format of the stored game data, if any.
    pub async fn game_version(&self) -> Result<Option<(String, i64)>> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM game_meta WHERE key IN ('build', 'format')")
                .fetch_all(&self.pool)
                .await?;
        let get = |k: &str| {
            rows.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        Ok(get("build").map(|build| {
            (
                build,
                get("format").and_then(|f| f.parse().ok()).unwrap_or(1),
            )
        }))
    }

    /// Replaces all game data in one transaction.
    pub async fn save_game(&self, data: &GameData) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for sql in [
            "DELETE FROM game_meta",
            "DELETE FROM game_items",
            "DELETE FROM game_categories",
            "DELETE FROM game_professions",
            "DELETE FROM game_recipes",
            "DELETE FROM game_reagents",
            "DELETE FROM game_recipe_items",
        ] {
            sqlx::query(sql).execute(&mut *tx).await?;
        }
        for (key, value) in [
            ("build", data.build.clone()),
            ("updated", data.updated.to_string()),
            ("format", GAME_FORMAT.to_string()),
        ] {
            sqlx::query("INSERT INTO game_meta (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await?;
        }
        for i in data.items.values() {
            sqlx::query(
                "INSERT INTO game_items (item_id, name, quality, item_level, required_level, class_id, subclass_id,
                                         buy_price, vendor_stack, sell_price)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(i.id)
            .bind(&i.name)
            .bind(i.quality)
            .bind(i.item_level)
            .bind(i.required_level)
            .bind(i.class_id)
            .bind(i.subclass_id)
            .bind(i.buy_price)
            .bind(i.vendor_stack)
            .bind(i.sell_price)
            .execute(&mut *tx)
            .await?;
        }
        for c in &data.categories {
            sqlx::query("INSERT OR REPLACE INTO game_categories (class_id, subclass_id, name) VALUES (?, ?, ?)")
                .bind(c.class_id)
                .bind(c.subclass_id.unwrap_or(-1))
                .bind(&c.name)
                .execute(&mut *tx)
                .await?;
        }
        for p in &data.professions {
            sqlx::query("INSERT INTO game_professions (id, name) VALUES (?, ?)")
                .bind(p.id)
                .bind(&p.name)
                .execute(&mut *tx)
                .await?;
        }
        for r in &data.recipes {
            sqlx::query(
                "INSERT INTO game_recipes (spell_id, name, profession, required, yellow, grey, creates_item, creates_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(r.spell_id)
            .bind(&r.name)
            .bind(r.profession)
            .bind(r.required)
            .bind(r.yellow)
            .bind(r.grey)
            .bind(r.creates.map(|c| c.item_id))
            .bind(r.creates.map(|c| c.count))
            .execute(&mut *tx)
            .await?;
            for (slot, reagent) in r.reagents.iter().enumerate() {
                sqlx::query("INSERT INTO game_reagents (spell_id, slot, item_id, count) VALUES (?, ?, ?, ?)")
                    .bind(r.spell_id)
                    .bind(slot as i64)
                    .bind(reagent.item_id)
                    .bind(reagent.count)
                    .execute(&mut *tx)
                    .await?;
            }
            for item in &r.taught_by {
                sqlx::query("INSERT INTO game_recipe_items (spell_id, item_id) VALUES (?, ?)")
                    .bind(r.spell_id)
                    .bind(item)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Stored game data (empty when never downloaded).
    pub async fn load_game(&self) -> Result<GameData> {
        let meta: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM game_meta")
            .fetch_all(&self.pool)
            .await?;
        let meta: HashMap<String, String> = meta.into_iter().collect();
        let items: Vec<GameItem> = sqlx::query_as(
            "SELECT item_id, name, quality, item_level, required_level, class_id, subclass_id,
                    buy_price, vendor_stack, sell_price
             FROM game_items",
        )
        .fetch_all(&self.pool)
        .await?;
        let professions: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, name FROM game_professions ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
        type RecipeRow = (
            i64,
            String,
            i64,
            Option<i64>,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
        );
        let recipes: Vec<RecipeRow> = sqlx::query_as(
            "SELECT spell_id, name, profession, required, yellow, grey, creates_item, creates_count
             FROM game_recipes ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        let reagent_rows: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT spell_id, item_id, count FROM game_reagents ORDER BY spell_id, slot",
        )
        .fetch_all(&self.pool)
        .await?;
        let taught_rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT spell_id, item_id FROM game_recipe_items")
                .fetch_all(&self.pool)
                .await?;

        let mut reagents: HashMap<i64, Vec<Reagent>> = HashMap::new();
        for (spell, item_id, count) in reagent_rows {
            reagents
                .entry(spell)
                .or_default()
                .push(Reagent { item_id, count });
        }
        let categories: Vec<(i64, i64, String)> =
            sqlx::query_as("SELECT class_id, subclass_id, name FROM game_categories")
                .fetch_all(&self.pool)
                .await?;
        let mut taught: HashMap<i64, Vec<i64>> = HashMap::new();
        for (spell, item) in taught_rows {
            taught.entry(spell).or_default().push(item);
        }
        let tables = Tables {
            items,
            professions: professions
                .into_iter()
                .map(|(id, name)| Profession { id, name })
                .collect(),
            recipes: recipes
                .into_iter()
                .map(
                    |(spell_id, name, profession, required, yellow, grey, item, count)| Recipe {
                        spell_id,
                        name,
                        profession,
                        required,
                        yellow,
                        grey,
                        creates: item.map(|item_id| Reagent {
                            item_id,
                            count: count.unwrap_or(1),
                        }),
                        reagents: reagents.remove(&spell_id).unwrap_or_default(),
                        taught_by: taught.remove(&spell_id).unwrap_or_default(),
                    },
                )
                .collect(),
            categories: categories
                .into_iter()
                .map(|(class_id, sub, name)| Category {
                    class_id,
                    subclass_id: (sub >= 0).then_some(sub),
                    name,
                })
                .collect(),
        };
        let mut game = GameData::new(
            meta.get("build").cloned().unwrap_or_default(),
            meta.get("updated")
                .and_then(|u| u.parse().ok())
                .unwrap_or(0),
            tables,
        );
        game.format = meta.get("format").and_then(|f| f.parse().ok()).unwrap_or(1);
        Ok(game)
    }
}
