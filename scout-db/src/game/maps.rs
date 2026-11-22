//! Continents and zones of the game data (replaced with it).

use anyhow::Result;
use sqlx::SqliteConnection;

use crate::Db;
use scout_core::map::GameMap;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS game_maps (
    id     INTEGER PRIMARY KEY,  -- UiMap id
    name   TEXT    NOT NULL,
    parent INTEGER NOT NULL,
    kind   INTEGER NOT NULL,     -- UiMap type: 2 continent, 3 zone
    x_min  REAL NOT NULL, x_max REAL NOT NULL,  -- world area (x north, y west)
    y_min  REAL NOT NULL, y_max REAL NOT NULL
);
";

/// Replaces the maps, inside the game data transaction.
pub(super) async fn save(tx: &mut SqliteConnection, maps: &[GameMap]) -> Result<()> {
    sqlx::query("DELETE FROM game_maps")
        .execute(&mut *tx)
        .await?;
    for m in maps {
        sqlx::query(
            "INSERT INTO game_maps (id, name, parent, kind, x_min, x_max, y_min, y_max)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(m.id)
        .bind(&m.name)
        .bind(m.parent)
        .bind(m.kind)
        .bind(m.x_min)
        .bind(m.x_max)
        .bind(m.y_min)
        .bind(m.y_max)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

impl Db {
    pub(super) async fn load_maps(&self) -> Result<Vec<GameMap>> {
        Ok(sqlx::query_as(
            "SELECT id, name, parent, kind, x_min, x_max, y_min, y_max FROM game_maps ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}
