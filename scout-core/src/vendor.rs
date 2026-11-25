//! Items merchants sell: unit price and, when Scout saw it, which NPC and where.

use std::collections::HashMap;

use crate::SCOUT;

/// A merchant selling an item.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
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

impl VendorOffer {
    /// "Nom — Zone (x, y)".
    pub fn place(&self) -> String {
        let mut text = self.npc.clone().unwrap_or_default();
        if let Some(zone) = &self.zone {
            text += &format!(" — {zone}");
        }
        if let (Some(x), Some(y)) = (self.x, self.y) {
            text += &format!(" ({x:.1}, {y:.1})");
        }
        text
    }
}

/// Merchant prices and offers by item.
#[derive(Debug, Default)]
pub struct VendorData {
    offers: HashMap<i64, Vec<VendorOffer>>,
}

impl VendorData {
    pub fn new(rows: Vec<VendorOffer>) -> Self {
        let mut offers: HashMap<i64, Vec<VendorOffer>> = HashMap::new();
        for row in rows {
            offers.entry(row.item_id).or_default().push(row);
        }
        Self { offers }
    }

    /// Unit price at a merchant: the cheapest seen by Scout, else the reference database's list (`foreverguide`).
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
