//! How much an item really sells at an auction house, from the daily sales totals of the price
//! server: a listed price means little for an item nobody buys.

use std::collections::{HashMap, HashSet};

/// Days of sales looked at (and asked from the server).
pub const WINDOW_DAYS: i64 = 7;
/// Sales are known for a house once it has at least this many distinct days.
pub const MIN_DAYS: usize = 2;

/// One item's sales over a UTC day, as the server sends them (running totals of the day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct SalesItem {
    pub item_id: i64,
    /// Sales seen as such (the auction was bought).
    pub sold_proven: i64,
    /// Auctions that vanished before their end: sold, or cancelled.
    pub sold_estimated: i64,
    /// Sum of quantity x price of the sales that have a price.
    pub sold_value: i64,
}

/// The sales of one UTC day (`YYYY-MM-DD`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct SalesDay {
    pub day: String,
    pub items: Vec<SalesItem>,
}

/// The answer of the server's `sales` route.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SalesAnswer {
    pub days: Vec<SalesDay>,
}

/// A stored row: one item on one day.
pub type SalesRow = (String, SalesItem);

/// What one item sells per day, over the days covered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SalesRate {
    pub per_day: f64,
    pub proven: u64,
    pub estimated: u64,
    /// Mean unit price of the sales; underestimated when some sales have no price.
    pub mean_unit: Option<f64>,
}

/// An estimated sale may be a cancellation: it counts for half.
const ESTIMATED_WEIGHT: f64 = 0.5;

/// The sales of one house over its covered days.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sales {
    rates: HashMap<i64, SalesRate>,
    days: usize,
}

impl Sales {
    /// Rates from stored rows (the last `WINDOW_DAYS` days at most are expected).
    pub fn from_rows(rows: &[SalesRow]) -> Self {
        let days: HashSet<&str> = rows.iter().map(|(day, _)| day.as_str()).collect();
        let covered = days.len().min(WINDOW_DAYS as usize);
        let mut totals: HashMap<i64, [i64; 3]> = HashMap::new();
        for (_, item) in rows {
            let t = totals.entry(item.item_id).or_default();
            t[0] += item.sold_proven;
            t[1] += item.sold_estimated;
            t[2] += item.sold_value;
        }
        let rates = totals
            .into_iter()
            .map(|(id, [proven, estimated, value])| {
                let sold = (proven + estimated).max(0);
                let rate = SalesRate {
                    per_day: (proven as f64 + ESTIMATED_WEIGHT * estimated as f64)
                        / covered.max(1) as f64,
                    proven: proven.max(0) as u64,
                    estimated: estimated.max(0) as u64,
                    mean_unit: (sold > 0 && value > 0).then(|| value as f64 / sold as f64),
                };
                (id, rate)
            })
            .collect();
        Self {
            rates,
            days: covered,
        }
    }

    /// Whether enough days are covered to tell that an absent item does not sell.
    pub fn known(&self) -> bool {
        self.days >= MIN_DAYS
    }

    /// The rate of an item: None while sales are unknown, zero for an item absent from known sales.
    pub fn rate(&self, item_id: i64) -> Option<SalesRate> {
        self.known().then(|| {
            self.rates.get(&item_id).copied().unwrap_or(SalesRate {
                per_day: 0.0,
                proven: 0,
                estimated: 0,
                mean_unit: None,
            })
        })
    }

    /// Days covered (at most `WINDOW_DAYS`).
    pub fn days(&self) -> usize {
        self.days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(day: &str, item_id: i64, proven: i64, estimated: i64, value: i64) -> SalesRow {
        (
            day.into(),
            SalesItem {
                item_id,
                sold_proven: proven,
                sold_estimated: estimated,
                sold_value: value,
            },
        )
    }

    #[test]
    fn rate_over_covered_days() {
        let sales = Sales::from_rows(&[
            row("2026-11-24", 1, 4, 2, 500),
            row("2026-11-25", 1, 2, 2, 100),
            row("2026-11-25", 2, 0, 1, 0),
        ]);
        assert!(sales.known());
        let r = sales.rate(1).unwrap();
        // (6 + 0.5 x 4) / 2 days
        assert_eq!((r.per_day, r.proven, r.estimated), (4.0, 6, 4));
        assert_eq!(r.mean_unit, Some(60.0));
        let r = sales.rate(2).unwrap();
        assert_eq!((r.per_day, r.mean_unit), (0.25, None));
        // Known sales: an absent item sells 0 a day.
        assert_eq!(sales.rate(9).unwrap().per_day, 0.0);
    }

    #[test]
    fn one_day_is_not_enough() {
        let sales = Sales::from_rows(&[row("2026-11-25", 1, 4, 0, 10)]);
        assert!(!sales.known());
        assert_eq!(sales.rate(1), None);
        assert_eq!(Sales::default().rate(1), None);
    }
}
