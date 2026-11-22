//! Items merchants sell: unit price and, when Scout saw it, which NPC and where. Replaced at each Scout import.

use std::collections::HashMap;

use anyhow::Result;

use super::Db;
use crate::import::{SCOUT, VendorPrice};

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS vendor_prices (
    item_id INTEGER NOT NULL,
    source  TEXT    NOT NULL,  -- scout (seen in game), foreverguide (its reagent list)
    npc_id  INTEGER NOT NULL,  -- 0 when the merchant is unknown
    price   REAL    NOT NULL,  -- copper per unit
    npc     TEXT,
    zone    TEXT,
    x       REAL,              -- position on the zone map, percent
    y       REAL,
    limited INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (item_id, source, npc_id)
) WITHOUT ROWID;
";

/// A merchant selling an item.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VendorOffer {
    pub item_id: i64,
    pub source: String,
    pub npc_id: i64,
    pub price: f64,
    pub npc: Option<String>,
    pub zone: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub limited: bool,
}

/// Merchant prices and offers by item.
#[derive(Debug, Default)]
pub struct VendorData {
    offers: HashMap<i64, Vec<VendorOffer>>,
}

impl VendorData {
    /// Unit price at a merchant: the cheapest seen by Scout, else Forever Guide's list.
    pub fn price(&self, item_id: i64) -> Option<f64> {
        let offers = self.offers.get(&item_id)?;
        let cheapest = |scout: bool| {
            offers
                .iter()
                .filter(|o| (o.source == SCOUT) == scout)
                .map(|o| o.price)
                .min_by(f64::total_cmp)
        };
        cheapest(true).or_else(|| cheapest(false))
    }

    /// Known merchants (NPC recorded) selling the item, cheapest first.
    pub fn merchants(&self, item_id: i64) -> Vec<&VendorOffer> {
        let mut list: Vec<&VendorOffer> = self
            .offers
            .get(&item_id)
            .map(|o| o.iter().filter(|o| o.npc.is_some()).collect())
            .unwrap_or_default();
        list.sort_by(|a, b| a.price.total_cmp(&b.price).then(a.npc_id.cmp(&b.npc_id)));
        list
    }
}

impl Db {
    /// An older `vendor_prices` (without NPCs) is dropped: it is rebuilt at each import anyway.
    pub(super) async fn migrate_vendor(pool: &sqlx::SqlitePool) -> Result<()> {
        let columns: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('vendor_prices')")
                .fetch_all(pool)
                .await?;
        if !columns.is_empty() && !columns.iter().any(|(c,)| c == "npc_id") {
            sqlx::raw_sql("DROP TABLE vendor_prices")
                .execute(pool)
                .await?;
        }
        sqlx::raw_sql(SCHEMA).execute(pool).await?;
        Ok(())
    }

    pub async fn save_vendor_prices(&self, prices: &[VendorPrice]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM vendor_prices")
            .execute(&mut *tx)
            .await?;
        for p in prices {
            sqlx::query(
                "INSERT OR REPLACE INTO vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(p.item_id)
            .bind(&p.source)
            .bind(p.npc_id.unwrap_or(0))
            .bind(p.price)
            .bind(p.npc.as_deref().filter(|n| !n.is_empty()))
            .bind(p.zone.as_deref().filter(|z| !z.is_empty()))
            .bind(p.x)
            .bind(p.y)
            .bind(p.limited == 1)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn vendor_data(&self) -> Result<VendorData> {
        let rows: Vec<VendorOffer> = sqlx::query_as(
            "SELECT item_id, source, npc_id, price, npc, zone, x, y, limited FROM vendor_prices",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut offers: HashMap<i64, Vec<VendorOffer>> = HashMap::new();
        for row in rows {
            offers.entry(row.item_id).or_default().push(row);
        }
        Ok(VendorData { offers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(source: &str, npc: Option<&str>, price: f64) -> VendorOffer {
        VendorOffer {
            item_id: 1,
            source: source.into(),
            npc_id: npc.map_or(0, |_| 7),
            price,
            npc: npc.map(str::to_owned),
            zone: None,
            x: None,
            y: None,
            limited: false,
        }
    }

    #[test]
    fn scout_prices_win_and_only_known_npcs_are_listed() {
        let data = VendorData {
            offers: HashMap::from([(
                1,
                vec![
                    offer("foreverguide", None, 10.0),
                    offer(SCOUT, Some("Bolero"), 9.0),
                ],
            )]),
        };
        assert_eq!(data.price(1), Some(9.0));
        assert_eq!(data.merchants(1).len(), 1);
        let guide_only = VendorData {
            offers: HashMap::from([(1, vec![offer("foreverguide", None, 10.0)])]),
        };
        assert_eq!(guide_only.price(1), Some(10.0));
        assert!(guide_only.merchants(1).is_empty());
    }
}
