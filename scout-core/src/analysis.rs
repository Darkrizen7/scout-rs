//! Price analysis on daily prices: weekend vs weekdays, and today's price vs the other days.
//!
//! An item's price for a day is the median of the minimum prices seen that day (all sources).
//! Days are local calendar days; the weekend is Saturday and Sunday.

mod stats;

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Local, NaiveDate, TimeZone, Weekday};

use stats::{median, permutation_p_value, robust_spread};

/// Fewest weekend days and weekdays an item (or the market) needs for a weekend comparison.
pub const MIN_DAYS_EACH: usize = 2;
/// Fewest other days needed to judge whether the latest day is abnormal.
pub const MIN_BASELINE_DAYS: usize = 3;
/// p-value under which a weekend difference counts as significant.
pub const SIGNIFICANCE: f64 = 0.05;
/// Robust z-score from which the latest price counts as abnormal.
pub const OUTLIER_Z: f64 = 3.0;
/// Smallest relative difference worth reporting, in percent.
pub const MIN_DIFF_PCT: f64 = 10.0;

/// One priced observation, as stored in the database.
pub struct Observation {
    pub item_id: i64,
    pub ts: i64,
    pub price: i64,
    pub quantity: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct WeekendDiff {
    pub weekend_price: i64,
    pub weekday_price: i64,
    /// Weekend relative to weekdays, in percent (negative = cheaper on weekends).
    pub diff_pct: f64,
    pub weekend_days: usize,
    pub weekday_days: usize,
    pub p_value: f64,
}

impl WeekendDiff {
    pub fn significant(&self) -> bool {
        self.p_value <= SIGNIFICANCE && self.diff_pct.abs() >= MIN_DIFF_PCT
    }
}

#[derive(Debug, Clone)]
pub struct Outlier {
    pub date: NaiveDate,
    pub price: i64,
    /// Median price of the other days.
    pub baseline: i64,
    pub diff_pct: f64,
    pub z: f64,
}

impl Outlier {
    pub fn significant(&self) -> bool {
        self.z.abs() >= OUTLIER_Z && self.diff_pct.abs() >= MIN_DIFF_PCT
    }
}

#[derive(Debug, Clone)]
pub struct ItemAnalysis {
    pub item_id: i64,
    /// Median units listed per observation (liquidity), when known.
    pub median_quantity: Option<i64>,
    pub weekend: Option<WeekendDiff>,
    pub latest: Option<Outlier>,
}

/// Market-wide price level of one day: median, over items, of price / item's usual price.
#[derive(Debug, Clone)]
pub struct DayIndex {
    pub date: NaiveDate,
    pub weekend: bool,
    pub index: f64,
    pub items: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub days: Vec<DayIndex>,
    pub weekend_days: usize,
    pub weekday_days: usize,
    /// Market weekend effect, tested with days as the unit (items move together within a day).
    pub market_weekend: Option<WeekendDiff>,
    pub items: Vec<ItemAnalysis>,
}

fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn local_date(ts: i64) -> Option<NaiveDate> {
    Some(Local.timestamp_opt(ts, 0).single()?.date_naive())
}

fn pct(value: f64, reference: f64) -> f64 {
    (value / reference - 1.0) * 100.0
}

pub fn analyze(observations: &[Observation]) -> Analysis {
    // item → day → prices seen that day; item → quantities.
    let mut seen: HashMap<i64, BTreeMap<NaiveDate, Vec<f64>>> = HashMap::new();
    let mut quantities: HashMap<i64, Vec<f64>> = HashMap::new();
    for o in observations {
        let Some(date) = local_date(o.ts) else {
            continue;
        };
        seen.entry(o.item_id)
            .or_default()
            .entry(date)
            .or_default()
            .push(o.price as f64);
        if let Some(q) = o.quantity {
            quantities.entry(o.item_id).or_default().push(q as f64);
        }
    }

    let mut items = Vec::with_capacity(seen.len());
    let mut relative: BTreeMap<NaiveDate, Vec<f64>> = BTreeMap::new();
    for (item_id, by_day) in seen {
        let daily: Vec<(NaiveDate, f64)> = by_day
            .into_iter()
            .map(|(d, mut p)| (d, median(&mut p)))
            .collect();
        let usual = median(&mut daily.iter().map(|&(_, p)| p).collect::<Vec<_>>());
        if daily.len() >= MIN_BASELINE_DAYS && usual > 0.0 {
            for &(date, price) in &daily {
                relative.entry(date).or_default().push(price / usual);
            }
        }
        items.push(ItemAnalysis {
            item_id,
            median_quantity: quantities
                .get_mut(&item_id)
                .map(|q| median(q).round() as i64),
            weekend: weekend_diff(&daily, item_id as u64),
            latest: latest_outlier(&daily),
        });
    }
    items.sort_by_key(|i| i.item_id);

    let days: Vec<DayIndex> = relative
        .into_iter()
        .map(|(date, mut rel)| DayIndex {
            date,
            weekend: is_weekend(date),
            items: rel.len(),
            index: median(&mut rel),
        })
        .collect();
    let market: Vec<(NaiveDate, f64)> = days.iter().map(|d| (d.date, d.index)).collect();
    Analysis {
        weekend_days: days.iter().filter(|d| d.weekend).count(),
        weekday_days: days.iter().filter(|d| !d.weekend).count(),
        market_weekend: weekend_diff(&market, 0),
        days,
        items,
    }
}

/// Weekend vs weekday daily prices; `None` without `MIN_DAYS_EACH` days on each side.
fn weekend_diff(daily: &[(NaiveDate, f64)], seed: u64) -> Option<WeekendDiff> {
    let (mut weekend, mut weekday): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    for &(date, price) in daily {
        if is_weekend(date) {
            &mut weekend
        } else {
            &mut weekday
        }
        .push(price);
    }
    if weekend.len() < MIN_DAYS_EACH || weekday.len() < MIN_DAYS_EACH {
        return None;
    }
    // Test on log prices: a price that doubles weighs as much as one that halves.
    let logs: Vec<f64> = weekend
        .iter()
        .chain(&weekday)
        .map(|p| p.max(1e-9).ln())
        .collect();
    let p_value = permutation_p_value(&logs, weekend.len(), seed);
    let (weekend_days, weekday_days) = (weekend.len(), weekday.len());
    let (we, wd) = (median(&mut weekend), median(&mut weekday));
    Some(WeekendDiff {
        weekend_price: we.round() as i64,
        weekday_price: wd.round() as i64,
        diff_pct: pct(we, wd),
        weekend_days,
        weekday_days,
        p_value,
    })
}

/// Latest day against the median of the other days (robust z-score).
fn latest_outlier(daily: &[(NaiveDate, f64)]) -> Option<Outlier> {
    let (&(date, price), others) = daily.split_last()?;
    if others.len() < MIN_BASELINE_DAYS {
        return None;
    }
    let mut baseline: Vec<f64> = others.iter().map(|&(_, p)| p).collect();
    let spread = robust_spread(&mut baseline);
    let reference = median(&mut baseline);
    Some(Outlier {
        date,
        price: price.round() as i64,
        baseline: reference.round() as i64,
        diff_pct: pct(price, reference),
        z: (price - reference) / spread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noon local time on 2026-11-`day` (the 7th is a Saturday).
    fn ts(day: u32) -> i64 {
        Local
            .with_ymd_and_hms(2026, 11, day, 12, 0, 0)
            .unwrap()
            .timestamp()
    }

    fn obs(item_id: i64, day: u32, price: i64) -> Observation {
        Observation {
            item_id,
            ts: ts(day),
            price,
            quantity: Some(10),
        }
    }

    #[test]
    fn finds_cheaper_weekends() {
        // Two weeks: weekends at 80, weekdays at 100 (+ a little noise).
        let mut data = Vec::new();
        for day in 7..=20 {
            let date = NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
            let price = if is_weekend(date) {
                80
            } else {
                100 + i64::from(day % 3)
            };
            data.push(obs(1, day, price));
        }
        let a = analyze(&data);
        let w = a.items[0].weekend.as_ref().unwrap();
        assert_eq!((w.weekend_days, w.weekday_days), (4, 10));
        assert!(w.diff_pct < -19.0 && w.diff_pct > -21.0, "{}", w.diff_pct);
        assert!(w.significant(), "p = {}", w.p_value);
        assert_eq!(a.weekend_days, 4);
    }

    #[test]
    fn needs_enough_days_and_ignores_noise() {
        let few = analyze(&[obs(1, 7, 80), obs(1, 9, 100), obs(1, 10, 100)]);
        assert!(
            few.items[0].weekend.is_none(),
            "a single weekend day is not enough"
        );

        // Same price distribution every day: no significant difference.
        let flat: Vec<Observation> = (7..=20)
            .map(|d| obs(1, d, 100 + i64::from(d % 2) * 5))
            .collect();
        assert!(
            !analyze(&flat).items[0]
                .weekend
                .as_ref()
                .unwrap()
                .significant()
        );
    }

    #[test]
    fn flags_abnormal_latest_price_only() {
        let mut data: Vec<Observation> = (9..=13)
            .map(|d| obs(1, d, 100 + i64::from(d % 2)))
            .collect();
        data.push(obs(1, 14, 60));
        let latest = analyze(&data).items[0].latest.clone().unwrap();
        assert_eq!((latest.price, latest.baseline), (60, 101));
        assert!(latest.significant() && latest.z < 0.0);

        data.push(obs(1, 15, 101));
        assert!(
            !analyze(&data).items[0]
                .latest
                .clone()
                .unwrap()
                .significant()
        );
    }

    #[test]
    fn daily_price_is_the_median_of_the_day() {
        // Day 9 is worth 110 (median of 100, 300, 110), not 170 (mean): baseline = median(110, 50, 400).
        let data = [
            obs(1, 9, 100),
            obs(1, 9, 300),
            obs(1, 9, 110),
            obs(1, 10, 50),
            obs(1, 11, 400),
            obs(1, 12, 200),
        ];
        assert_eq!(
            analyze(&data).items[0].latest.as_ref().unwrap().baseline,
            110
        );
    }
}
