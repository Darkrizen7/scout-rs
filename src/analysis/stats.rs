//! Small robust statistics helpers.

/// Median (sorts the slice); 0 when empty.
pub fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Spread comparable to a standard deviation, robust to outliers: 1.4826 × MAD.
/// Floored at 3 % of the median (and 1 copper) so stable prices do not give huge z-scores.
pub fn robust_spread(values: &mut [f64]) -> f64 {
    let m = median(values);
    let mut deviations: Vec<f64> = values.iter().map(|v| (v - m).abs()).collect();
    (1.4826 * median(&mut deviations))
        .max(0.03 * m.abs())
        .max(1.0)
}

/// Above this many label assignments, the test samples instead of enumerating.
const EXACT_LIMIT: u64 = 20_000;
const SAMPLES: u32 = 10_000;

/// Two-sided permutation test: are the first `k` values (e.g. weekend days) different on average
/// from the others? Exact when the number of splits is small, Monte Carlo (seeded) otherwise.
pub fn permutation_p_value(values: &[f64], k: usize, seed: u64) -> f64 {
    let n = values.len();
    let total: f64 = values.iter().sum();
    let statistic =
        |group_sum: f64| (group_sum / k as f64 - (total - group_sum) / (n - k) as f64).abs();
    let observed = statistic(values[..k].iter().sum()) - 1e-9;

    if binomial(n as u64, k as u64) <= EXACT_LIMIT {
        let (mut extreme, mut count) = (0u64, 0u64);
        for_each_subset_sum(values, k, &mut |sum| {
            count += 1;
            if statistic(sum) >= observed {
                extreme += 1;
            }
        });
        return extreme as f64 / count as f64;
    }

    let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut shuffled = values.to_vec();
    let mut extreme = 0u32;
    for _ in 0..SAMPLES {
        // Partial Fisher–Yates: only the first k positions are needed.
        for i in 0..k {
            let j = i + (rng.next() % (n - i) as u64) as usize;
            shuffled.swap(i, j);
        }
        if statistic(shuffled[..k].iter().sum()) >= observed {
            extreme += 1;
        }
    }
    f64::from(extreme + 1) / f64::from(SAMPLES + 1)
}

fn binomial(n: u64, k: u64) -> u64 {
    let k = k.min(n - k);
    (0..k).fold(1u64, |acc, i| acc.saturating_mul(n - i) / (i + 1))
}

/// Calls `f` with the sum of every subset of `k` values.
fn for_each_subset_sum(values: &[f64], k: usize, f: &mut impl FnMut(f64)) {
    fn go(values: &[f64], k: usize, sum: f64, f: &mut impl FnMut(f64)) {
        if k == 0 {
            return f(sum);
        }
        for i in 0..=values.len() - k {
            go(&values[i + 1..], k - 1, sum + values[i], f);
        }
    }
    go(values, k, 0.0, f);
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medians() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(&mut []), 0.0);
    }

    #[test]
    fn exact_permutation_bounds() {
        // 3 vs 3 fully separated: only 2 of the 20 splits are as extreme → p = 0.1.
        let p = permutation_p_value(&[1.0, 1.1, 1.2, 5.0, 5.1, 5.2], 3, 0);
        assert!((p - 0.1).abs() < 1e-9, "{p}");
        // Identical groups: every split is as extreme.
        assert_eq!(permutation_p_value(&[1.0, 2.0, 1.0, 2.0], 2, 0), 1.0);
    }

    #[test]
    fn sampled_permutation_detects_shift() {
        let values: Vec<f64> = (0..40)
            .map(|i| if i < 15 { 10.0 } else { 12.0 } + (i % 3) as f64 * 0.1)
            .collect();
        assert!(binomial(40, 15) > EXACT_LIMIT);
        assert!(permutation_p_value(&values, 15, 7) < 0.001);
    }
}
