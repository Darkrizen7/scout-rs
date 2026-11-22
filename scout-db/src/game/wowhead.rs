//! Items fetched from Wowhead (absent from the client tables). Kept across game data updates,
//! so each item is asked for only once.

use std::collections::HashMap;

use anyhow::Result;

use crate::Db;
use scout_core::gamedata::GameItem;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS wowhead_items (
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
";

impl Db {
    /// Every item already fetched from Wowhead.
    pub async fn wowhead_items(&self) -> Result<HashMap<i64, GameItem>> {
        let items: Vec<GameItem> = sqlx::query_as(
            "SELECT item_id, name, quality, item_level, required_level, class_id, subclass_id,
                    buy_price, vendor_stack, sell_price
             FROM wowhead_items",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(items.into_iter().map(|i| (i.id, i)).collect())
    }

    pub async fn save_wowhead_item(&self, i: &GameItem) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO wowhead_items (item_id, name, quality, item_level, required_level,
                                                   class_id, subclass_id, buy_price, vendor_stack, sell_price)
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
