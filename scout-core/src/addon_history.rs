//! Prices of the last days, written for the Scout addon (`History.lua`, format 1).
//!
//! One price per item and local day: the median of the minimum prices seen that day, the same
//! rule as the analysis. The contract is described in `docs/sources.md`.

use std::collections::BTreeMap;
use std::fmt::Write;

use chrono::{Duration, NaiveDate};

use crate::analysis::{Observation, local_date, stats::median};

/// Version of the Lua file's layout (`format` in `ScoutHistory`).
pub const HISTORY_FORMAT: u32 = 1;
/// Days kept, today included.
pub const HISTORY_DAYS: u32 = 30;
/// Name of the file in the addon's folder.
pub const HISTORY_FILE: &str = "History.lua";

/// Days since 1970-01-01 of a local date (the addon computes the same number from its clock).
pub fn epoch_day(date: NaiveDate) -> i64 {
    (date - NaiveDate::default()).num_days()
}

/// First day of the window that ends on `today`.
pub fn first_day(today: NaiveDate, days: u32) -> NaiveDate {
    today - Duration::days(i64::from(days.max(1)) - 1)
}

/// Per item, one price (copper, per unit) for each of the `days` days ending `today`
/// (oldest first; `None` = nothing seen that day). Items without any price in the window are left out.
pub fn daily_prices(
    observations: &[Observation],
    today: NaiveDate,
    days: u32,
) -> BTreeMap<i64, Vec<Option<i64>>> {
    let first = first_day(today, days);
    let len = days.max(1) as usize;
    // item → day index → minimum prices seen that day
    let mut seen: BTreeMap<i64, BTreeMap<usize, Vec<f64>>> = BTreeMap::new();
    for o in observations {
        let Some(date) = local_date(o.ts) else {
            continue;
        };
        if date < first || date > today {
            continue;
        }
        let index = (date - first).num_days() as usize;
        seen.entry(o.item_id)
            .or_default()
            .entry(index)
            .or_default()
            .push(o.price as f64);
    }
    seen.into_iter()
        .map(|(item, per_day)| {
            let mut prices = vec![None; len];
            for (index, mut values) in per_day {
                prices[index] = Some(median(&mut values).round() as i64);
            }
            (item, prices)
        })
        .collect()
}

/// The `History.lua` text: items sorted by id, empty trailing days dropped.
pub fn to_lua(
    prices: &BTreeMap<i64, Vec<Option<i64>>>,
    first_day: NaiveDate,
    generated: i64,
) -> String {
    let mut out = format!(
        "-- Généré par scout-rs : ne pas modifier (réécrit chaque jour).\n\
         ScoutHistory = {{\n  format = {HISTORY_FORMAT},\n  generated = {generated},\n  \
         first_day = {},\n  prices = {{\n",
        epoch_day(first_day)
    );
    for (item, days) in prices {
        let fields: Vec<String> = days
            .iter()
            .map(|p| p.map_or_else(String::new, |p| p.to_string()))
            .collect();
        let line = fields.join(",");
        let _ = writeln!(out, "    [{item}] = \"{}\",", line.trim_end_matches(','));
    }
    out.push_str("  },\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn at(d: u32, hour: u32) -> i64 {
        Local
            .from_local_datetime(&day(d).and_hms_opt(hour, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp()
    }

    fn obs(item_id: i64, d: u32, hour: u32, price: i64) -> Observation {
        Observation {
            item_id,
            ts: at(d, hour),
            price,
            quantity: None,
        }
    }

    #[test]
    fn day_price_is_the_median_of_the_day() {
        let o = [obs(1, 10, 8, 100), obs(1, 10, 12, 200), obs(1, 10, 16, 120)];
        let prices = daily_prices(&o, day(10), 3);
        assert_eq!(prices[&1], vec![None, None, Some(120)]);
        // Even count: mean of the middle two, rounded.
        let o = [obs(1, 10, 8, 100), obs(1, 10, 12, 121)];
        assert_eq!(daily_prices(&o, day(10), 1)[&1], vec![Some(111)]);
    }

    #[test]
    fn empty_days_stay_empty() {
        let o = [obs(1, 8, 9, 5), obs(1, 10, 9, 7)];
        assert_eq!(
            daily_prices(&o, day(10), 3)[&1],
            vec![Some(5), None, Some(7)]
        );
    }

    #[test]
    fn observations_outside_the_window_are_ignored() {
        let o = [obs(1, 5, 9, 5), obs(1, 11, 9, 9), obs(2, 4, 9, 1)];
        let prices = daily_prices(&o, day(10), 3);
        assert!(prices.is_empty(), "{prices:?}");
    }

    #[test]
    fn lua_has_no_trailing_commas() {
        let o = [obs(2, 9, 9, 118), obs(1, 8, 9, 120), obs(1, 10, 9, 121)];
        let prices = daily_prices(&o, day(10), 3);
        let lua = to_lua(&prices, first_day(day(10), 3), 1764170000);
        let first = epoch_day(day(8));
        let expected = format!(
            "-- Généré par scout-rs : ne pas modifier (réécrit chaque jour).\n\
             ScoutHistory = {{\n  format = 1,\n  generated = 1764170000,\n  first_day = {first},\n  \
             prices = {{\n    [1] = \"120,,121\",\n    [2] = \",118\",\n  }},\n}}\n"
        );
        assert_eq!(lua, expected);
    }

    #[test]
    fn trailing_empty_days_are_dropped() {
        let prices = BTreeMap::from([(7, vec![Some(3), None, None])]);
        assert!(to_lua(&prices, day(1), 0).contains("[7] = \"3\","));
    }

    #[test]
    fn epoch_day_counts_from_1970() {
        assert_eq!(epoch_day(NaiveDate::from_ymd_opt(1970, 1, 2).unwrap()), 1);
        assert_eq!(epoch_day(day(1)), 20758);
    }
}
