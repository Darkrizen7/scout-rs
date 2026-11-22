//! Averages of the daily points over longer periods (weeks, months), for the market charts.

use chrono::{Datelike, Local, NaiveDate, TimeZone};

use super::{Origin, Point};

/// How many days one chart point stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    /// Every reading, not only the last of each day (see `series`).
    Detail,
    /// One point per day: the merged data as is.
    #[default]
    Day,
    /// Mean over each ISO week (Monday to Sunday, local time).
    Week,
    /// Mean over each calendar month (local time).
    Month,
}

impl Period {
    /// First day of the period containing `ts`.
    fn bucket(self, ts: i64) -> Option<NaiveDate> {
        let day = Local.timestamp_opt(ts, 0).single()?.date_naive();
        match self {
            Self::Day | Self::Detail => Some(day),
            Self::Week => {
                Some(day - chrono::Duration::days(i64::from(day.weekday().num_days_from_monday())))
            }
            Self::Month => day.with_day(1),
        }
    }
}

/// Mean value of `points` (chronological) over each `period`. A point sits at the mean time of
/// its readings; `Origin::Period` marks it as a mean over several days.
pub fn aggregate(points: &[Point], period: Period) -> Vec<Point> {
    if matches!(period, Period::Day | Period::Detail) {
        return points.to_vec();
    }
    let mut out = Vec::new();
    let mut group: Vec<&Point> = Vec::new();
    let mut current = None;
    let flush = |group: &mut Vec<&Point>, out: &mut Vec<Point>| {
        if group.is_empty() {
            return;
        }
        let n = group.len() as f64;
        out.push(Point {
            ts: (group.iter().map(|p| p.ts as f64).sum::<f64>() / n).round() as i64,
            value: group.iter().map(|p| p.value).sum::<f64>() / n,
            origin: Origin::Period,
        });
        group.clear();
    };
    for p in points {
        let bucket = period.bucket(p.ts);
        if bucket != current {
            flush(&mut group, &mut out);
            current = bucket;
        }
        group.push(p);
    }
    flush(&mut group, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, value: f64) -> Point {
        let ts = Local
            .with_ymd_and_hms(2026, 11, day, 12, 0, 0)
            .unwrap()
            .timestamp();
        Point {
            ts,
            value,
            origin: Origin::Scout,
        }
    }

    #[test]
    fn day_keeps_every_point() {
        let points = [at(2, 1.0), at(3, 2.0)];
        assert_eq!(aggregate(&points, Period::Day), points);
    }

    #[test]
    fn weeks_split_on_monday() {
        // 2026-11-01 is a Sunday: the 2nd (Monday) starts a new week.
        let points = [at(1, 10.0), at(2, 20.0), at(4, 40.0), at(9, 90.0)];
        let values: Vec<f64> = aggregate(&points, Period::Week)
            .iter()
            .map(|p| p.value)
            .collect();
        assert_eq!(values, [10.0, 30.0, 90.0]);
    }

    #[test]
    fn month_averages_the_whole_month() {
        let points = [at(1, 10.0), at(30, 30.0)];
        let monthly = aggregate(&points, Period::Month);
        assert_eq!(monthly.len(), 1);
        assert_eq!(monthly[0].value, 20.0);
    }
}
