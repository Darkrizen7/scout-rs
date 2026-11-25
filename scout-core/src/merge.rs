//! Market chart points from Scout scans and Scout API (our price server, from the Battle.net
//! auction API): on a day with a Scout scan, Scout until its last scan, then the Scout API readings
//! that follow it; on other days, Scout API alone.

use std::collections::BTreeMap;

use chrono::NaiveDate;

mod aggregate;
pub use aggregate::{Period, aggregate};

use crate::time::local_day;
use crate::{SCOUT, Snapshot};

/// Prices of the Battle.net auction API, through our price server (`scout-server`).
pub const SCOUT_API: &str = "scout-api";

/// Where a merged point comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Scout,
    ScoutApi,
    /// Mean over several days (weekly or monthly charts).
    Period,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub ts: i64,
    pub value: f64,
    pub origin: Origin,
}

/// Latest reading of a source on a given day.
type Reading = Option<(i64, f64)>;

fn keep_latest(slot: &mut Reading, ts: i64, value: f64) {
    if slot.is_none_or(|(t, _)| ts >= t) {
        *slot = Some((ts, value));
    }
}

/// Merge the sources day by day (local days), in chronological order: the day's latest reading,
/// Scout on a tie.
pub fn merge(history: &[Snapshot], value: impl Fn(&Snapshot) -> Option<f64>) -> Vec<Point> {
    let mut days: BTreeMap<NaiveDate, [Reading; 2]> = BTreeMap::new();
    for s in history {
        let slot = match s.source.as_str() {
            SCOUT => 0,
            SCOUT_API => 1,
            _ => continue,
        };
        let (Some(day), Some(v)) = (local_day(s.ts), value(s)) else {
            continue;
        };
        keep_latest(&mut days.entry(day).or_default()[slot], s.ts, v);
    }
    days.into_values()
        .filter_map(|[scout, api]| {
            let point = |r: Reading, origin| r.map(|(ts, value)| Point { ts, value, origin });
            match (scout, api) {
                (Some((s, _)), Some((a, _))) if a > s => point(api, Origin::ScoutApi),
                (Some(_), _) => point(scout, Origin::Scout),
                _ => point(api, Origin::ScoutApi),
            }
        })
        .collect()
}

/// Points of the chart for a period: `Detail` keeps every reading, the others go through `merge`
/// then `aggregate`.
pub fn series(
    history: &[Snapshot],
    period: Period,
    value: impl Fn(&Snapshot) -> Option<f64> + Copy,
) -> Vec<Point> {
    if period == Period::Detail {
        detailed(history, value)
    } else {
        aggregate(&merge(history, value), period)
    }
}

/// Every reading, with the same choice of source as `merge`, day by day: on the days Scout has
/// readings, those and the Scout API readings after its last one; else Scout API alone.
fn detailed(history: &[Snapshot], value: impl Fn(&Snapshot) -> Option<f64>) -> Vec<Point> {
    let mut days: BTreeMap<NaiveDate, Vec<Point>> = BTreeMap::new();
    for s in history {
        let origin = match s.source.as_str() {
            SCOUT => Origin::Scout,
            SCOUT_API => Origin::ScoutApi,
            _ => continue,
        };
        let (Some(day), Some(v)) = (local_day(s.ts), value(s)) else {
            continue;
        };
        days.entry(day).or_default().push(Point {
            ts: s.ts,
            value: v,
            origin,
        });
    }
    let mut out = Vec::new();
    for mut points in days.into_values() {
        let last_scan = points
            .iter()
            .filter(|p| p.origin == Origin::Scout)
            .map(|p| p.ts)
            .max();
        if let Some(last) = last_scan {
            points.retain(|p| p.origin == Origin::Scout || p.ts > last);
        }
        points.sort_by_key(|p| p.ts);
        out.extend(points);
    }
    out
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
    fn the_latest_reading_of_the_day_wins() {
        let h = [
            snap(SCOUT_API, 1_000_000_000, 10),
            snap(SCOUT, 1_000_000_100, 30),
            snap(SCOUT_API, 1_000_000_200, 20),
        ];
        assert_eq!(merged(&h), [(Origin::ScoutApi, 20.0)]);
        let h = [
            snap(SCOUT_API, 1_000_000_100, 10),
            snap(SCOUT, 1_000_000_100, 30),
        ];
        assert_eq!(merged(&h), [(Origin::Scout, 30.0)]);
    }

    #[test]
    fn scout_api_comes_after_scout() {
        let day = 86_400 * 3;
        let h = [
            snap(SCOUT, 1_000_000_000, 30),
            snap(SCOUT_API, 999_999_900, 29),
            snap(SCOUT_API, 1_000_000_100 + day, 12),
            snap(SCOUT_API, 1_000_000_200 + day, 13),
        ];
        assert_eq!(
            merged(&h),
            [(Origin::Scout, 30.0), (Origin::ScoutApi, 13.0)]
        );
        let detail: Vec<_> = series(&h, Period::Detail, |s| Some(s.price? as f64))
            .iter()
            .map(|p| (p.origin, p.value))
            .collect();
        assert_eq!(
            detail,
            [
                (Origin::Scout, 30.0),
                (Origin::ScoutApi, 12.0),
                (Origin::ScoutApi, 13.0)
            ]
        );
    }

    #[test]
    fn detail_keeps_scout_then_the_api_after_the_last_scan() {
        let h = [
            snap(SCOUT_API, 1_000_000_000, 10),
            snap(SCOUT_API, 1_000_000_100, 12),
            snap(SCOUT, 1_000_000_200, 30),
            snap(SCOUT_API, 1_000_000_250, 14),
            snap(SCOUT, 1_000_000_300, 31),
            snap(SCOUT_API, 1_000_000_400, 15),
        ];
        let values: Vec<f64> = series(&h, Period::Detail, |s| Some(s.price? as f64))
            .iter()
            .map(|p| p.value)
            .collect();
        assert_eq!(values, [30.0, 31.0, 15.0]);
    }
}
