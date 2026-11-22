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
}
