//! Where a reagent is cheapest to buy, and what an item is worth once looted.

/// Auction house cut on a sale.
pub const AUCTION_CUT: f64 = 0.05;

/// Where a unit can be bought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Merchant,
    Auction,
}

/// Cheapest unit price (copper) between a merchant and the auction house; the merchant wins a tie.
pub fn cheapest(merchant: Option<f64>, auction: Option<i64>) -> Option<(f64, Source)> {
    let merchant = merchant.map(|p| (p, Source::Merchant));
    let auction = auction.map(|p| (p as f64, Source::Auction));
    match (merchant, auction) {
        (Some(m), Some(a)) => Some(if m.0 <= a.0 { m } else { a }),
        (m, a) => m.or(a),
    }
}

/// What one unit brings (copper): the auction price after the cut, or the merchant's if higher.
/// None when neither is known (no auction price and the merchant buys nothing).
pub fn sale_value(auction: Option<i64>, merchant_sell: i64) -> Option<f64> {
    let auction = auction.map(|p| p as f64 * (1.0 - AUCTION_CUT));
    let merchant = (merchant_sell > 0).then_some(merchant_sell as f64);
    match (auction, merchant) {
        (Some(a), Some(m)) => Some(a.max(m)),
        (a, m) => a.or(m),
    }
}

/// Fewer units than this on the auction house make an offer "isolated" (thresholds to refine, see APP-22).
pub const ISOLATED_QUANTITY: i64 = 5;
/// An isolated offer above this multiple of the merchant price is not a real market (see APP-22).
pub const ISOLATED_RATIO: f64 = 20.0;

/// Item classes valued at the merchant unless a recipe uses them: weapon, armor, miscellaneous.
const GEAR_CLASSES: [i64; 3] = [2, 4, 15];

/// What the loot value is based on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootPricing {
    /// Auction price after the cut (or the merchant's if higher).
    Auction,
    /// Only the merchant price is known or exists.
    Merchant,
    /// Grey item: merchant price only.
    Poor,
    /// Auction price discarded: a few units far above the merchant price.
    Isolated,
    /// Auction price discarded: weapon, armor or miscellaneous that no recipe uses.
    NotReagent,
}

/// What we know about an item to value it once looted.
#[derive(Debug, Clone, Copy)]
pub struct LootItem {
    pub auction: Option<i64>,
    pub quantity: Option<i64>,
    pub merchant_sell: i64,
    pub quality: i64,
    pub class_id: Option<i64>,
    pub is_reagent: bool,
}

/// Value of one looted unit (copper), why, and the auction price set aside if any.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LootValue {
    pub copper: f64,
    pub pricing: LootPricing,
    pub discarded_auction: Option<i64>,
}

/// Value of one looted unit: the merchant price when the auction price is not a real market, else `sale_value`.
/// None when nothing gives a price.
pub fn loot_value(item: LootItem) -> Option<LootValue> {
    let merchant = (item.merchant_sell > 0).then_some(item.merchant_sell as f64);
    let gear = item.class_id.is_some_and(|c| GEAR_CLASSES.contains(&c)) && !item.is_reagent;
    let isolated = match (item.auction, merchant) {
        (Some(a), Some(m)) => {
            item.quantity.is_some_and(|q| q < ISOLATED_QUANTITY) && a as f64 > ISOLATED_RATIO * m
        }
        _ => false,
    };
    let reason = if item.quality == 0 {
        Some(LootPricing::Poor)
    } else if gear {
        Some(LootPricing::NotReagent)
    } else if isolated {
        Some(LootPricing::Isolated)
    } else {
        None
    };
    match reason {
        Some(pricing) => Some(LootValue {
            copper: merchant?,
            pricing,
            discarded_auction: item.auction,
        }),
        None => {
            let copper = sale_value(item.auction, item.merchant_sell)?;
            // `sale_value` keeps the higher of the two: the merchant wins when it is at least as much.
            let pricing = if merchant.is_some_and(|m| m >= copper) {
                LootPricing::Merchant
            } else {
                LootPricing::Auction
            };
            Some(LootValue {
                copper,
                pricing,
                discarded_auction: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_cheaper_source() {
        assert_eq!(cheapest(Some(5.0), Some(9)), Some((5.0, Source::Merchant)));
        assert_eq!(cheapest(Some(9.0), Some(5)), Some((5.0, Source::Auction)));
        assert_eq!(cheapest(Some(5.0), Some(5)), Some((5.0, Source::Merchant)));
        assert_eq!(cheapest(None, Some(5)), Some((5.0, Source::Auction)));
        assert_eq!(cheapest(None, None), None);
    }

    #[test]
    fn values_a_looted_item() {
        assert_eq!(sale_value(Some(100), 10), Some(95.0));
        assert_eq!(sale_value(Some(10), 50), Some(50.0));
        assert_eq!(sale_value(None, 50), Some(50.0));
        assert_eq!(sale_value(None, 0), None);
    }

    fn item(auction: Option<i64>, quantity: Option<i64>, sell: i64) -> LootItem {
        LootItem {
            auction,
            quantity,
            merchant_sell: sell,
            quality: 1,
            class_id: Some(7),
            is_reagent: false,
        }
    }

    #[test]
    fn grey_and_gear_use_the_merchant() {
        let grey = LootItem {
            quality: 0,
            ..item(Some(5121), Some(50), 20)
        };
        let v = loot_value(grey).unwrap();
        assert_eq!(
            (v.copper, v.pricing, v.discarded_auction),
            (20.0, LootPricing::Poor, Some(5121))
        );
        let dagger = LootItem {
            class_id: Some(2),
            ..item(Some(1999), Some(1), 10)
        };
        let v = loot_value(dagger).unwrap();
        assert_eq!((v.copper, v.pricing), (10.0, LootPricing::NotReagent));
        assert_eq!(
            loot_value(LootItem {
                quality: 0,
                ..item(Some(9), None, 0)
            }),
            None
        );
    }

    #[test]
    fn a_reagent_of_misc_class_keeps_the_auction() {
        let misc = LootItem {
            class_id: Some(15),
            is_reagent: true,
            ..item(Some(1000), Some(1), 100)
        };
        let v = loot_value(misc).unwrap();
        assert_eq!((v.copper, v.pricing), (950.0, LootPricing::Auction));
    }

    #[test]
    fn isolated_offers_are_discarded() {
        let v = loot_value(item(Some(1000), Some(2), 10)).unwrap();
        assert_eq!(
            (v.copper, v.pricing, v.discarded_auction),
            (10.0, LootPricing::Isolated, Some(1000))
        );
        let v = loot_value(item(Some(1000), Some(5), 10)).unwrap();
        assert_eq!((v.copper, v.pricing), (950.0, LootPricing::Auction));
        assert_eq!(loot_value(item(None, None, 0)), None);
        let v = loot_value(item(None, None, 30)).unwrap();
        assert_eq!((v.copper, v.pricing), (30.0, LootPricing::Merchant));
        // Many units just under the merchant price: the merchant wins.
        let v = loot_value(item(Some(100), Some(10), 98)).unwrap();
        assert_eq!((v.copper, v.pricing), (98.0, LootPricing::Merchant));
    }
}
