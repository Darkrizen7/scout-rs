//! Where a reagent is cheapest to buy.

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
}
