//! Number and period wording of event texts (design.md §7, plan §7.4): Indonesian
//! separators, U+2212 for negatives, a scale word for large amounts, `Q3 25` periods.
//! Only the human `text` goes through here; the structured payload keeps the raw numbers.

use serde_json::Value;

/// Said in place of a number the payload does not hold (missing, null or not finite).
pub const UNREADABLE: &str = "nilai tidak terbaca";

/// Amounts of at least this many units get a scale word, so no value shows more than four
/// integer digits.
const SCALE_FROM: f64 = 10_000.0;

const SCALES: [(f64, &str); 4] = [
    (1e12, "triliun"),
    (1e9, "miliar"),
    (1e6, "juta"),
    (1e3, "ribu"),
];

/// A finite number out of a payload value.
pub fn readable(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite())
}

/// `1.234,5` with at most `max_dec` decimals, trailing zeros dropped, U+2212 for negatives.
fn group(v: f64, max_dec: usize) -> String {
    let fixed = format!("{:.*}", max_dec, v.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    let frac = frac.trim_end_matches('0');
    if !frac.is_empty() {
        out.push(',');
        out.push_str(frac);
    }
    // A value that rounds to zero carries no sign.
    let is_zero = out.chars().all(|c| matches!(c, '0' | '.' | ','));
    if v < 0.0 && !is_zero {
        format!("\u{2212}{out}")
    } else {
        out
    }
}

/// Counts, prices and ratios: separators only, no scale word.
pub fn plain(v: f64) -> String {
    group(v, 4)
}

/// Money-sized amounts: `−8 juta`, `12,3 miliar`; under 10.000 the plain number. The scale is
/// picked from the value as rounded for display, so `999.999` reads `1 juta`, not `1.000 ribu`.
pub fn scaled(v: f64) -> String {
    let rounded = (v.abs() * 100.0).round() / 100.0;
    if rounded < SCALE_FROM {
        return group(v, 2);
    }
    let (div, word) = SCALES
        .iter()
        .find(|(d, _)| (rounded / d * 100.0).round() / 100.0 >= 1.0)
        .copied()
        .unwrap_or((1.0, ""));
    format!("{} {word}", group(v / div, 2))
}

/// A day count out of a payload value; a missing or negative one is the unreadable phrase.
pub fn days(v: &Value) -> String {
    v.as_i64()
        .filter(|d| *d >= 0)
        .map_or_else(|| UNREADABLE.to_string(), |d| plain(d as f64))
}

/// `2025Q3` → `Q3 25`; anything else is returned as it came.
pub fn period(label: &str) -> String {
    let parsed = label.split_once('Q').and_then(|(fy, q)| {
        let fy: u32 = fy.parse().ok()?;
        let q: u32 = q.parse().ok().filter(|q| (1..=4).contains(q))?;
        Some(format!("Q{q} {:02}", fy % 100))
    });
    parsed.unwrap_or_else(|| label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_uses_indonesian_separators_and_the_minus_sign() {
        assert_eq!(plain(1500.0), "1.500");
        assert_eq!(plain(1234.5), "1.234,5");
        assert_eq!(plain(-0.25), "\u{2212}0,25");
        assert_eq!(plain(0.0), "0");
        assert_eq!(plain(-0.00001), "0");
    }

    #[test]
    fn scaled_picks_the_scale_word_by_magnitude() {
        assert_eq!(scaled(-8_000_000.0), "\u{2212}8 juta");
        assert_eq!(scaled(105_000_000.0), "105 juta");
        assert_eq!(scaled(12_340_000_000.0), "12,34 miliar");
        assert_eq!(scaled(2_500_000_000_000.0), "2,5 triliun");
        assert_eq!(scaled(10_000.0), "10 ribu");
        assert_eq!(scaled(9_999.0), "9.999");
        assert_eq!(scaled(0.0), "0");
        assert_eq!(scaled(999_999.0), "1 juta");
        assert_eq!(scaled(9_999.999), "10 ribu");
    }

    #[test]
    fn days_reads_whole_non_negative_counts_only() {
        assert_eq!(days(&json!(28)), "28");
        assert_eq!(days(&Value::Null), UNREADABLE);
        assert_eq!(days(&json!(-3)), UNREADABLE);
        assert_eq!(days(&json!("x")), UNREADABLE);
    }

    #[test]
    fn period_becomes_q_and_two_digit_year() {
        assert_eq!(period("2025Q3"), "Q3 25");
        assert_eq!(period("2009Q1"), "Q1 09");
        assert_eq!(period("2025Q9"), "2025Q9");
        assert_eq!(period("?"), "?");
    }

    #[test]
    fn readable_rejects_missing_and_non_finite_values() {
        assert_eq!(readable(&json!(3.5)), Some(3.5));
        assert_eq!(readable(&Value::Null), None);
        assert_eq!(readable(&json!("7")), None);
    }
}
