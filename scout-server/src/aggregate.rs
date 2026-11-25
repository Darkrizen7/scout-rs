//! Prices of one listing, per item (variants such as random suffixes are merged for now).
//!
//! Percentiles are weighted by quantity: the 25th percentile is the unit price at which a quarter
//! of the units on sale can be bought, so one cheap single unit does not hide a dear stack.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One row of `prices`.
#[derive(Debug, PartialEq, Eq)]
pub struct ItemPrices {
    pub item_id: u32,
    /// Unit prices in copper; `None` when every auction of the item is bid-only.
    pub min_unit: Option<u64>,
    pub p25_unit: Option<u64>,
    pub median_unit: Option<u64>,
    /// Units on sale, bid-only auctions included.
    pub quantity: u64,
    pub auctions: u32,
    /// Auctions without a buyout: in `quantity` and `auctions`, not in the prices.
    pub buyout_less: u32,
}

impl ItemPrices {
    /// An item of the previous version of the listing that is no longer on sale.
    pub fn gone(item_id: u32) -> Self {
        Self {
            item_id,
            min_unit: None,
            p25_unit: None,
            median_unit: None,
            quantity: 0,
            auctions: 0,
            buyout_less: 0,
        }
    }
}

/// One item's auctions while a listing is read.
#[derive(Default)]
struct Acc {
    /// (unit price, quantity) of the auctions with a price.
    priced: Vec<(u64, u64)>,
    quantity: u64,
    auctions: u32,
    buyout_less: u32,
}

/// The listing's number of auctions and its prices, by item id.
pub fn aggregate(bytes: &[u8]) -> Result<(u64, Vec<ItemPrices>)> {
    #[derive(Deserialize)]
    struct Listing {
        // An empty house has no `auctions` field at all.
        #[serde(default)]
        auctions: Vec<Auction>,
    }
    #[derive(Deserialize)]
    struct Auction {
        item: Item,
        quantity: u64,
        /// Commodities.
        unit_price: Option<u64>,
        /// Other items: the price of the whole lot.
        buyout: Option<u64>,
    }
    #[derive(Deserialize)]
    struct Item {
        id: u32,
    }

    let listing: Listing = serde_json::from_slice(bytes).context("enchères illisibles")?;
    let count = listing.auctions.len() as u64;
    let mut items: BTreeMap<u32, Acc> = BTreeMap::new();
    for auction in listing.auctions {
        let quantity = auction.quantity.max(1);
        let acc = items.entry(auction.item.id).or_default();
        acc.quantity += quantity;
        acc.auctions += 1;
        match auction.unit_price.or(auction.buyout.map(|b| b / quantity)) {
            Some(unit) => acc.priced.push((unit, quantity)),
            None => acc.buyout_less += 1,
        }
    }
    let rows = items
        .into_iter()
        .map(|(item_id, mut acc)| {
            acc.priced.sort_unstable();
            ItemPrices {
                item_id,
                min_unit: acc.priced.first().map(|&(unit, _)| unit),
                p25_unit: weighted_percentile(&acc.priced, 1, 4),
                median_unit: weighted_percentile(&acc.priced, 1, 2),
                quantity: acc.quantity,
                auctions: acc.auctions,
                buyout_less: acc.buyout_less,
            }
        })
        .collect();
    Ok((count, rows))
}

/// The unit price reached once `num / den` of the units of `sorted` (unit price, quantity) are counted.
fn weighted_percentile(sorted: &[(u64, u64)], num: u64, den: u64) -> Option<u64> {
    let total: u64 = sorted.iter().map(|&(_, q)| q).sum();
    let target = (total * num).div_ceil(den).max(1);
    let mut seen = 0;
    sorted.iter().find_map(|&(unit, quantity)| {
        seen += quantity;
        (seen >= target).then_some(unit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_items_and_commodities() {
        let json = r#"{"_links":{},"auctions":[
            {"id":1,"item":{"id":768},"buyout":147,"quantity":1,"time_left":"LONG"},
            {"id":2,"item":{"id":768,"bonus_lists":[1]},"buyout":300,"quantity":1,"time_left":"LONG"},
            {"id":3,"item":{"id":768},"bid":50,"quantity":1,"time_left":"SHORT"},
            {"id":4,"item":{"id":118},"unit_price":5,"quantity":1,"time_left":"LONG"},
            {"id":5,"item":{"id":118},"unit_price":8,"quantity":9,"time_left":"LONG"}]}"#;
        let (count, rows) = aggregate(json.as_bytes()).unwrap();
        assert_eq!(count, 5);
        assert_eq!(
            rows,
            [
                ItemPrices {
                    item_id: 118,
                    min_unit: Some(5),
                    // 10 units: the 3rd and the 5th are both at 8, not the lone 5.
                    p25_unit: Some(8),
                    median_unit: Some(8),
                    quantity: 10,
                    auctions: 2,
                    buyout_less: 0,
                },
                ItemPrices {
                    item_id: 768,
                    min_unit: Some(147),
                    p25_unit: Some(147),
                    median_unit: Some(147),
                    quantity: 3,
                    auctions: 3,
                    buyout_less: 1,
                },
            ]
        );
    }

    #[test]
    fn lot_buyout_is_divided_and_empty_houses_have_nothing() {
        let lot = r#"{"auctions":[{"id":1,"item":{"id":2589},"buyout":100,"quantity":4}]}"#;
        let (_, rows) = aggregate(lot.as_bytes()).unwrap();
        assert_eq!(rows[0].min_unit, Some(25));
        let empty = r#"{"_links":{},"id":112,"name":"Hôtel","commodities":{"href":"x"}}"#;
        assert_eq!(aggregate(empty.as_bytes()).unwrap(), (0, vec![]));
    }

    #[test]
    fn bid_only_items_have_no_price() {
        let json = r#"{"auctions":[{"id":1,"item":{"id":7},"bid":10,"quantity":1}]}"#;
        let (_, rows) = aggregate(json.as_bytes()).unwrap();
        assert_eq!(
            (rows[0].min_unit, rows[0].median_unit, rows[0].buyout_less),
            (None, None, 1)
        );
    }

    #[test]
    fn weighted_percentiles() {
        let sorted = [(1, 1), (2, 1), (3, 1), (4, 1)];
        assert_eq!(weighted_percentile(&sorted, 1, 4), Some(1));
        assert_eq!(weighted_percentile(&sorted, 1, 2), Some(2));
        assert_eq!(weighted_percentile(&[], 1, 2), None);
    }
}
