//! Items merchants sell: unit price and, when Scout saw it, which NPC and where. Replaced at each Scout import.

use anyhow::Result;

use super::Db;
use scout_core::{VendorData, VendorOffer, VendorPrice};

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
        Ok(VendorData::new(rows))
    }
}
