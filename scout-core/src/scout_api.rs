//! Prices from our price server (`scout-server`: the Battle.net auction API), source `scout-api`.
//!
//! Two kinds of downloads:
//! - the **latest** prices of every item, at most once an hour: kept only when they change, plus
//!   the first of each local day while the item is on sale (so every day has its price for the daily
//!   charts and the analysis), which avoids ~7 000 rows an hour;
//! - the **whole history** of one item when its market sheet opens, kept as is (a cache).

use std::collections::{HashMap, HashSet};

use crate::Row;
use crate::merge::SCOUT_API;
use crate::time::local_day;

/// Our price server, unless the settings name another one.
pub const DEFAULT_SERVER: &str = "https://scout.arthurdevtest.duckdns.org";
/// Latest prices and item histories are downloaded again once they are this old.
pub const REFRESH_SECS: i64 = 3600;

/// An auction house as the server lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ServerHouse {
    pub id: u32,
    /// « Hôtel des ventes de la Horde »… (five houses share each name).
    pub name: String,
    /// Newest prices of the house (Unix seconds).
    pub last_update: Option<i64>,
}

/// One line of the server's CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceLine {
    pub ts: i64,
    pub item_id: i64,
    /// Copper; `None` when every auction is bid-only, or the item is gone.
    pub min_unit: Option<i64>,
    /// 0 = no longer listed.
    pub quantity: i64,
}

impl PriceLine {
    /// The market's price: none when the item is gone.
    fn price(&self) -> Option<i64> {
        if self.quantity == 0 {
            None
        } else {
            self.min_unit
        }
    }

    fn row(&self) -> Row {
        Row {
            source: SCOUT_API.to_owned(),
            item_id: self.item_id,
            name: None,
            timestamp: self.ts,
            price: self.price(),
            quantity: Some(self.quantity),
        }
    }
}

/// Newest stored Scout API price of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastPrice {
    pub ts: i64,
    pub price: Option<i64>,
}

/// The rows to store from the latest prices, given the newest stored price of each item.
/// An item still priced here but missing from them left the auction house meanwhile.
pub fn latest_rows(last: &HashMap<i64, LastPrice>, latest: &[PriceLine]) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in latest {
        let price = line.price();
        let keep = match last.get(&line.item_id) {
            // Nothing to say about an item that was never seen on sale.
            None => price.is_some(),
            Some(before) => {
                before.price != price
                    || (price.is_some() && local_day(before.ts) != local_day(line.ts))
            }
        };
        if keep && last.get(&line.item_id).is_none_or(|b| line.ts > b.ts) {
            rows.push(line.row());
        }
    }
    let Some(newest) = latest.iter().map(|l| l.ts).max() else {
        return rows;
    };
    let listed: HashSet<i64> = latest.iter().map(|l| l.item_id).collect();
    let mut gone: Vec<_> = last
        .iter()
        .filter(|(id, before)| before.price.is_some() && before.ts < newest && !listed.contains(id))
        .map(|(&item_id, _)| item_id)
        .collect();
    gone.sort_unstable();
    rows.extend(gone.into_iter().map(|item_id| {
        PriceLine {
            ts: newest,
            item_id,
            min_unit: None,
            quantity: 0,
        }
        .row()
    }));
    rows
}

/// One item's history, stored whole.
pub fn history_rows(lines: &[PriceLine]) -> Vec<Row> {
    lines.iter().map(PriceLine::row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: i64, item_id: i64, min_unit: Option<i64>, quantity: i64) -> PriceLine {
        PriceLine {
            ts,
            item_id,
            min_unit,
            quantity,
        }
    }

    fn kept(rows: &[Row]) -> Vec<(i64, i64, Option<i64>)> {
        rows.iter()
            .map(|r| (r.timestamp, r.item_id, r.price))
            .collect()
    }

    fn last(entries: &[(i64, i64, Option<i64>)]) -> HashMap<i64, LastPrice> {
        entries
            .iter()
            .map(|&(item_id, ts, price)| (item_id, LastPrice { ts, price }))
            .collect()
    }

    #[test]
    fn latest_keeps_changes_and_the_first_of_each_day() {
        let t = 1_000_000_000;
        let days = 86_400 * 3;
        let stored = last(&[(765, t - 3600, Some(42)), (2589, t - 3600, Some(27))]);
        let latest = [
            line(t, 765, Some(42), 90),
            line(t, 2589, Some(25), 400),
            line(t, 118, Some(8), 5),
        ];
        assert_eq!(
            kept(&latest_rows(&stored, &latest)),
            [(t, 2589, Some(25)), (t, 118, Some(8))]
        );
        let tomorrow = [line(t + days, 765, Some(42), 90)];
        let rows = latest_rows(&stored, &tomorrow);
        // Same price, new day: kept; 2589 is missing from the listing: it left.
        assert_eq!(
            kept(&rows),
            [(t + days, 765, Some(42)), (t + days, 2589, None)]
        );
        assert_eq!(rows[0].source, SCOUT_API);
        assert_eq!(rows[1].quantity, Some(0));
    }

    #[test]
    fn gone_items_are_written_once_and_old_lines_never() {
        let t = 1_000_000_000;
        let stored = last(&[(7, t - 3600, None), (9, t + 60, Some(5))]);
        let latest = [
            line(t, 7, None, 0),
            line(t, 8, None, 0),
            line(t, 9, Some(4), 1),
        ];
        // 7 already gone, 8 never seen, 9 has a newer price stored (from its history).
        assert!(latest_rows(&stored, &latest).is_empty());
        assert!(latest_rows(&stored, &[]).is_empty());
    }

    #[test]
    fn history_is_kept_whole() {
        let lines = [line(100, 765, Some(42), 9), line(200, 765, Some(41), 0)];
        assert_eq!(
            kept(&history_rows(&lines)),
            [(100, 765, Some(42)), (200, 765, None)]
        );
    }
}
