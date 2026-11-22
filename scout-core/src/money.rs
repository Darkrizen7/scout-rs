//! Copper amounts: display, parsing and the profit calculation.

/// `12345` → `1g 23s 45c` (zero parts omitted, sign kept).
pub fn format(copper: i64) -> String {
    let sign = if copper < 0 { "-" } else { "" };
    let c = copper.unsigned_abs();
    let (g, s, c) = (c / 10_000, c / 100 % 100, c % 100);
    let parts: Vec<String> = [(g, "g"), (s, "s"), (c, "c")]
        .into_iter()
        .filter(|&(v, _)| v > 0)
        .map(|(v, unit)| format!("{v}{unit}"))
        .collect();
    if parts.is_empty() {
        "0c".into()
    } else {
        format!("{sign}{}", parts.join(" "))
    }
}

/// Parses `1g 20s 5c`, `1g20s`, `75s`… A bare number is gold (`1.5` = 1g 50s).
pub fn parse(text: &str) -> Option<i64> {
    let text = text.trim().replace(',', ".").to_lowercase();
    if text.is_empty() {
        return None;
    }
    if let Ok(gold) = text.parse::<f64>() {
        return (gold >= 0.0).then(|| (gold * 10_000.0).round() as i64);
    }
    let mut total = 0;
    let mut number = String::new();
    for ch in text.chars().filter(|c| !c.is_whitespace()) {
        match ch {
            '0'..='9' => number.push(ch),
            'g' | 's' | 'c' => {
                let value: i64 = number.parse().ok()?;
                total += value
                    * match ch {
                        'g' => 10_000,
                        's' => 100,
                        _ => 1,
                    };
                number.clear();
            }
            _ => return None,
        }
    }
    number.is_empty().then_some(total)
}

/// Result of buying `quantity` units and reselling them at the auction house.
#[derive(Debug, PartialEq)]
pub struct Profit {
    /// Cut taken by the auction house on the whole sale.
    pub commission: i64,
    /// Net gain on the whole operation (negative = loss).
    pub total: i64,
    /// Net gain per unit.
    pub per_unit: f64,
    /// Gain relative to the money spent, in percent (`None` when buying is free).
    pub roi: Option<f64>,
    /// Lowest unit sell price that does not lose money.
    pub break_even: i64,
}

pub fn profit(buy: i64, sell: i64, quantity: i64, cut_percent: f64) -> Profit {
    let cut = cut_percent / 100.0;
    let cost = buy * quantity;
    let revenue = sell * quantity;
    let commission = (revenue as f64 * cut).round() as i64;
    let total = revenue - commission - cost;
    Profit {
        commission,
        total,
        per_unit: total as f64 / quantity.max(1) as f64,
        roi: (cost > 0).then(|| total as f64 / cost as f64 * 100.0),
        break_even: if cut < 1.0 {
            (buy as f64 / (1.0 - cut)).ceil() as i64
        } else {
            i64::MAX
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format(12_345), "1g 23s 45c");
        assert_eq!(format(10_005), "1g 5c");
        assert_eq!(format(0), "0c");
        assert_eq!(format(-250), "-2s 50c");
    }

    #[test]
    fn parses() {
        assert_eq!(parse("1g 20s 5c"), Some(12_005));
        assert_eq!(parse("75S"), Some(7_500));
        assert_eq!(parse("1,5"), Some(15_000));
        assert_eq!(parse("3"), Some(30_000));
        assert_eq!(parse("12x"), None);
        assert_eq!(parse("12g 5"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn computes_profit_with_cut() {
        // Buy 10 × 1g, sell 10 × 1g 50s, 5 % cut: 15g - 75s - 10g = 4g 25s.
        let p = profit(10_000, 15_000, 10, 5.0);
        assert_eq!(p.commission, 7_500);
        assert_eq!(p.total, 42_500);
        assert_eq!(p.per_unit, 4_250.0);
        assert_eq!(p.roi, Some(42.5));
        assert_eq!(p.break_even, 10_527);
    }
}
