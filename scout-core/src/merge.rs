//! One point per day for the market charts: Scout wins, else the two other sources are averaged.

use std::collections::BTreeMap;

use chrono::{Local, NaiveDate, TimeZone};

use crate::{SCOUT, Snapshot};

pub const AUCTIONATOR: &str = "auctionator";
pub const FOREVER_GUIDE: &str = "foreverguide";

/// Where a merged point comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Scout,
    Auctionator,
    ForeverGuide,
    /// Mean of Auctionator and Forever Guide on the same day.
    Average,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub ts: i64,
    pub value: f64,
    pub origin: Origin,
}

/// Latest reading of a source on a given day.
type Reading = Option<(i64, f64)>;

fn local_day(ts: i64) -> Option<NaiveDate> {
    Some(Local.timestamp_opt(ts, 0).single()?.date_naive())
}

fn keep_latest(slot: &mut Reading, ts: i64, value: f64) {
    if slot.is_none_or(|(t, _)| ts >= t) {
        *slot = Some((ts, value));
    }
}

/// Merge the sources day by day (local days), in chronological order.
/// Scout alone if present; otherwise the mean of Auctionator and Forever Guide, or whichever exists.
pub fn merge(history: &[Snapshot], value: impl Fn(&Snapshot) -> Option<f64>) -> Vec<Point> {
    let mut days: BTreeMap<NaiveDate, [Reading; 3]> = BTreeMap::new();
    for s in history {
        let slot = match s.source.as_str() {
            SCOUT => 0,
            AUCTIONATOR => 1,
            FOREVER_GUIDE => 2,
            _ => continue,
        };
        let (Some(day), Some(v)) = (local_day(s.ts), value(s)) else {
            continue;
        };
        keep_latest(&mut days.entry(day).or_default()[slot], s.ts, v);
    }
    days.into_values()
        .filter_map(|[scout, auction, guide]| {
            let point = |r: Reading, origin| r.map(|(ts, value)| Point { ts, value, origin });
            point(scout, Origin::Scout).or_else(|| match (auction, guide) {
                (Some((t1, v1)), Some((t2, v2))) => Some(Point {
                    ts: t1.max(t2),
                    value: (v1 + v2) / 2.0,
                    origin: Origin::Average,
                }),
                (a, g) => point(a, Origin::Auctionator).or_else(|| point(g, Origin::ForeverGuide)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(source: &str, ts: i64, price: i64) -> Snapshot {
        Snapshot {
            source: source.into(),
            ts,
            price: Some(price),
            quantity: None,
        }
    }

    fn merged(history: &[Snapshot]) -> Vec<(Origin, f64)> {
        merge(history, |s| Some(s.price? as f64))
            .into_iter()
            .map(|p| (p.origin, p.value))
            .collect()
    }

    #[test]
    fn scout_hides_the_others_on_the_same_day() {
        let h = [
            snap(AUCTIONATOR, 1_000_000_000, 10),
            snap(SCOUT, 1_000_000_100, 30),
            snap(FOREVER_GUIDE, 1_000_000_200, 20),
        ];
        assert_eq!(merged(&h), [(Origin::Scout, 30.0)]);
    }

    #[test]
    fn averages_without_scout_and_keeps_a_lone_source() {
        let day = 86_400 * 3;
        let h = [
            snap(AUCTIONATOR, 1_000_000_000, 10),
            snap(FOREVER_GUIDE, 1_000_000_100, 20),
            snap(FOREVER_GUIDE, 1_000_000_000 + day, 50),
        ];
        assert_eq!(
            merged(&h),
            [(Origin::Average, 15.0), (Origin::ForeverGuide, 50.0)]
        );
    }
}
