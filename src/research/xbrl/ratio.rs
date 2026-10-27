//! Ratio facts that a filing types as money.
//!
//! The canonical ratio fact is **percent points** under the unit `pure` (`2.35` is 2,35 %): that
//! is what the 2023–2024 filings, the taxonomy fixtures and the pack formulas (`NpfGross / 100`)
//! use. Some banks tag the same ratio with `unitRef="IDR"` instead, and their authoring tool
//! then applies the "Jutaan" step (÷ 10⁶) to it once per export pass: the one ratio shows up as
//! `1.97`, `1.9e-6`, `1.9e-12` or `1.9e-18` in consecutive filings, and from 2026 as a fraction
//! (`0.018`, `1.79e-20` = 0.0179 × 10⁻¹⁸). Left as filed, one ratio sits at several scales and
//! any formula over it is wrong by up to eighteen orders of magnitude.
//!
//! Only the elements listed in `PERCENT_RATIOS` are touched, and only when their unit is a
//! currency; a ratio filed as `pure` is trusted as is. The original value stays in the source
//! file (`filings.source_path`).
//!
//! A net ratio can be under 0,1 % (0.05 in percent points looks like a fraction), so it takes
//! the scale of the gross ratio of the same context: one export pass scales both alike, and
//! a gross ratio is never that low. Only a net ratio filed without its gross one is read alone.

use std::collections::HashMap;

use super::parse::Fact;

const GROSS: &str = "idx-cor:NonPerformingFinancingGross";
const NET: &str = "idx-cor:NonPerformingFinancingNet";

/// Elements quoted in percent points whose filings were seen with a monetary unit.
pub const PERCENT_RATIOS: &[&str] = &[GROSS, NET];

/// One export pass of the "Jutaan" step.
const STEP: f64 = 1e6;
/// A value below this after undoing the steps is not a plausible fraction of a percent.
const FLOOR: f64 = 1e-4;
/// Below this, a value is a fraction (0.018 = 1.8 %), not percent points: no bank reports a
/// gross non-performing ratio under 0,1 % (net ones can, hence the gross scale above).
const FRACTION_CEILING: f64 = 0.1;
const MAX_STEPS: u32 = 5;

fn is_currency(unit: &str) -> bool {
    unit.len() == 3 && unit.bytes().all(|b| b.is_ascii_uppercase())
}

/// The factor that brings `value` to percent points: the ÷ 10⁶ steps undone until the number
/// is a plausible fraction or percent, then × 100 for a fraction.
fn factor(value: f64) -> f64 {
    let mut f = 1.0;
    let mut steps = 0;
    while (value * f).abs() < FLOOR && steps < MAX_STEPS {
        f *= STEP;
        steps += 1;
    }
    if (value * f).abs() < FRACTION_CEILING {
        f *= 100.0;
    }
    f
}

/// Six decimals, to drop float noise.
fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// The value in percent points, read alone (see `factor`).
pub fn to_percent_points(value: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    round6(value * factor(value))
}

/// The context of a fact: period and dimensions.
fn context(f: &Fact) -> (Option<String>, String, String) {
    (f.start.clone(), f.end.clone(), f.dims.clone())
}

/// Rewrites ratio facts typed as money to percent points under `pure`; returns how many.
pub fn normalise(facts: &mut [Fact]) -> usize {
    let gross: HashMap<_, f64> = facts
        .iter()
        .filter(|f| f.element == GROSS && is_currency(&f.unit))
        .filter(|f| f.value != 0.0 && f.value.is_finite())
        .map(|f| (context(f), factor(f.value)))
        .collect();
    let mut changed = 0;
    for f in facts.iter_mut() {
        if is_currency(&f.unit) && PERCENT_RATIOS.contains(&f.element.as_str()) {
            f.value = match gross.get(&context(f)) {
                Some(g) if f.element == NET && f.value.is_finite() => round6(f.value * g),
                _ => to_percent_points(f.value),
            };
            f.unit = "pure".into();
            // The declared precision belonged to the original scale.
            f.decimals = None;
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_ratio_filed_at_every_scale_comes_out_as_the_same_percent() {
        // BRIS non-performing financing, gross, as the scraper delivered it.
        let filed = [
            ("2024Q1", 2.01, 2.01),
            ("2024FY", 0.0000019, 1.9),
            ("2024Q2", 0.00000000000199, 1.99),
            ("2025Q1", 0.00000000000000000188, 1.88),
            ("2025Q3", 0.00000000000184, 1.84),
            ("2026Q1", 0.0000000000000000000179, 1.79),
            ("2026Q2", 0.018, 1.8),
        ];
        for (period, value, percent) in filed {
            assert!(
                (to_percent_points(value) - percent).abs() < 1e-9,
                "{period}: {value} -> {}",
                to_percent_points(value)
            );
        }
        // net ratio, fraction in 2026
        assert!((to_percent_points(0.0033) - 0.33).abs() < 1e-9);
        assert_eq!(to_percent_points(0.0), 0.0);
    }

    fn fact(element: &str, unit: &str, value: f64) -> Fact {
        Fact {
            element: element.into(),
            ctx_kind: super::super::parse::CtxKind::Instant,
            start: None,
            end: "2026-03-31".into(),
            dims: String::new(),
            value,
            unit: unit.into(),
            decimals: None,
            cite: None,
        }
    }

    #[test]
    fn only_listed_ratios_typed_as_money_are_rewritten() {
        let mut facts = vec![
            fact("idx-cor:NonPerformingFinancingGross", "IDR", 1.79e-20),
            fact("idx-cor:NonPerformingFinancingGross", "pure", 2.35),
            fact("idx-cor:Cash", "IDR", 1.2e-7),
        ];
        assert_eq!(normalise(&mut facts), 1);
        assert_eq!((facts[0].unit.as_str(), facts[0].value), ("pure", 1.79));
        assert_eq!((facts[1].unit.as_str(), facts[1].value), ("pure", 2.35));
        assert_eq!((facts[2].unit.as_str(), facts[2].value), ("IDR", 1.2e-7));
    }

    #[test]
    fn a_net_ratio_under_a_tenth_of_a_percent_takes_the_gross_scale() {
        // Net 0.05 % next to gross 1.5 %: as percent points, after one export pass, and as
        // fractions. Read alone, 0.05 would look like a fraction and become 5 %.
        for (gross, net) in [(1.5, 0.05), (1.5e-6, 0.05e-6), (0.015, 0.0005)] {
            let mut facts = vec![fact(NET, "IDR", net), fact(GROSS, "IDR", gross)];
            assert_eq!(normalise(&mut facts), 2);
            assert_eq!(
                (facts[0].value, facts[1].value),
                (0.05, 1.5),
                "{gross} {net}"
            );
        }
        // Another context's gross ratio does not count: the net one is read alone.
        let mut other = fact(GROSS, "IDR", 1.5);
        other.end = "2025-12-31".into();
        let mut facts = vec![fact(NET, "IDR", 0.0033), other];
        normalise(&mut facts);
        assert_eq!(facts[0].value, 0.33);
    }
}
