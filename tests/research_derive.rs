//! Property tests for the derivation rules (research plan §7.2): Σ standalone = FY when
//! four inputs exist, TTM(Q4) = FY, and TTM for Q ≥ 2 does not depend on Q1 when a later
//! comparative exists. Inputs are generated; nothing here comes from a filing.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use investing_app::research::derive::{derive, restatement_tolerance, FactIn, Flag};
use investing_app::research::xbrl::CtxKind;
use proptest::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn ytd(ffy: i32, fq: u32, fy: i32, q: u32, value: f64, decimals: i32) -> FactIn {
    let end = ["03-31", "06-30", "09-30", "12-31"][(q - 1) as usize];
    FactIn {
        filing_id: format!(
            "ZZZZ_{ffy}_{}",
            if fq == 4 {
                "FY".to_string()
            } else {
                format!("Q{fq}")
            }
        ),
        filing_fy: ffy,
        filing_q: fq,
        element: "idx-cor:Revenue".into(),
        ctx_kind: CtxKind::Duration,
        start: Some(d(&format!("{fy}-01-01"))),
        end: d(&format!("{fy}-{end}")),
        dims: String::new(),
        value,
        decimals: Some(decimals),
    }
}

/// Rounds like a filing that declares `decimals`.
fn round(v: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (v * scale).round() / scale
}

/// Four standalone quarters per year become YTD figures reported by each quarter's own
/// filing; the filing for (fy, q) also carries the comparatives (fy−1, q) and (fy−1, FY),
/// as the IDX interim filings do.
fn series(standalone: &[[f64; 4]], first_fy: i32, decimals: i32) -> Vec<FactIn> {
    let mut ytd_of: BTreeMap<(i32, u32), f64> = BTreeMap::new();
    for (i, year) in standalone.iter().enumerate() {
        let fy = first_fy + i as i32;
        let mut acc = 0.0;
        for (qi, s) in year.iter().enumerate() {
            acc += s;
            ytd_of.insert((fy, qi as u32 + 1), round(acc, decimals));
        }
    }
    let mut facts = Vec::new();
    for (&(fy, q), &v) in &ytd_of {
        facts.push(ytd(fy, q, fy, q, v, decimals));
        for (p_fy, p_q) in [(fy - 1, q), (fy - 1, 4)] {
            if let Some(&pv) = ytd_of.get(&(p_fy, p_q)) {
                facts.push(ytd(fy, q, p_fy, p_q, pv, decimals));
            }
        }
    }
    facts
}

fn quarter() -> impl Strategy<Value = f64> {
    // Revenue-like magnitudes, negative allowed (losses), in millions when decimals = −6.
    prop_oneof![
        (-500i64..5_000).prop_map(|v| v as f64 * 1e6),
        (-5_000i64..50_000).prop_map(|v| v as f64 * 1e5),
    ]
}

fn year() -> impl Strategy<Value = [f64; 4]> {
    [quarter(), quarter(), quarter(), quarter()]
}

/// Cases per property: 200, or `PROPTEST_CASES` for a longer search (an explicit count in the
/// config would otherwise override the variable).
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn sum_of_standalone_equals_fy_and_ttm_q4_is_fy(years in prop::collection::vec(year(), 1..4), decimals in -6i32..=0) {
        let facts = series(&years, 2020, decimals);
        let dv = derive(&facts);
        let tol = 4.0 * 0.5 * 10f64.powi(-decimals) + 1e-6;
        for (i, _) in years.iter().enumerate() {
            let fy = 2020 + i as i32;
            let cells: Vec<_> = dv.cells.iter().filter(|c| c.fy == fy).collect();
            prop_assert_eq!(cells.len(), 4);
            let sum: f64 = cells.iter().map(|c| c.standalone.unwrap()).sum();
            let q4 = cells.iter().find(|c| c.q == 4).unwrap();
            prop_assert!((sum - q4.annual.unwrap()).abs() <= tol, "fy {fy}: {sum} vs {:?}", q4.annual);
            prop_assert_eq!(q4.ttm, q4.annual);
            prop_assert!(q4.flags.contains(&Flag::DerivedFy));
            prop_assert!(!cells.iter().any(|c| c.flags.contains(&Flag::Gap)));
        }
        prop_assert!(dv.reconciliation_failures.is_empty(), "{:?}", dv.reconciliation_failures);
        prop_assert!(dv.restatements.is_empty());
    }

    #[test]
    fn ttm_after_q1_does_not_depend_on_q1_when_comparatives_exist(y0 in year(), y1 in year(), decimals in -6i32..=0) {
        let full = series(&[y0, y1], 2024, decimals);
        let dv_full = derive(&full);
        // Drop the Q1 2025 filing entirely (its YTD Q1 and its comparatives).
        let without_q1: Vec<FactIn> = full.iter().filter(|f| f.filing_id != "ZZZZ_2025_Q1").cloned().collect();
        let dv = derive(&without_q1);
        for q in 2..=3u32 {
            let a = dv_full.cells.iter().find(|c| c.fy == 2025 && c.q == q).unwrap();
            let b = dv.cells.iter().find(|c| c.fy == 2025 && c.q == q).unwrap();
            prop_assert!(a.ttm.is_some());
            prop_assert_eq!(a.ttm, b.ttm, "Q{} TTM changed when Q1 went missing", q);
        }
        // Q1 itself is now a gap and so is Q2 standalone (needs YTD Q1), never a guess.
        let q1 = dv.cells.iter().find(|c| c.fy == 2025 && c.q == 1).unwrap();
        prop_assert!(q1.flags.contains(&Flag::Gap));
        prop_assert_eq!(q1.standalone, None);
    }

    #[test]
    fn a_standalone_value_never_mixes_filing_vintages(years in prop::collection::vec(year(), 2..4), lost in prop::collection::vec((0i32..3, 1u32..=4), 0..5), bump in 0i64..3) {
        // Lose some original filings (their comparatives in the next year survive) and
        // let the comparatives drift from the originals.
        let mut facts = series(&years, 2020, -6);
        facts.retain(|f| !lost.contains(&(f.filing_fy - 2020, f.filing_q)));
        for f in facts.iter_mut().filter(|f| f.end.year() < f.filing_fy) {
            f.value += bump as f64 * 1e6;
        }
        let dv = derive(&facts);
        for c in dv.cells.iter().filter(|c| c.standalone.is_some()) {
            // Vintage: the fiscal year of the reporting filing (`ZZZZ_<year>_<period>`).
            let vintages: BTreeSet<&str> = c
                .inputs
                .standalone
                .iter()
                .map(|i| &i.fact.filing_id[5..9])
                .collect();
            prop_assert_eq!(vintages.len(), 1, "{} {}Q{}: {:?}", c.element, c.fy, c.q, c.inputs.standalone);
        }
        // Not vacuous: with every filing present, drifting comparatives make no gap.
        if lost.is_empty() {
            prop_assert!(dv.cells.iter().all(|c| c.standalone.is_some()));
        }
    }

    #[test]
    fn per_share_elements_never_fail_reconciliation(years in prop::collection::vec(year(), 2..4), bump in 1i64..1_000) {
        let mut facts = series(&years, 2020, -6);
        for f in &mut facts {
            f.element = "idx-cor:BasicEarningsLossPerShareFromContinuingOperations".into();
            // Every comparative restated: a revenue-like element fails every checked year.
            if f.end.year() < f.filing_fy {
                f.value += bump as f64 * 1e6;
            }
        }
        let dv = derive(&facts);
        prop_assert!(dv.reconciliation_failures.is_empty());
        prop_assert!(dv.cells.iter().any(|c| c.flags.contains(&Flag::PerShareApprox)));
    }

    #[test]
    fn restatements_never_change_the_displayed_original(y0 in year(), y1 in year(), bump in 1i64..1_000) {
        restated_comparative_keeps_the_original(y0, y1, bump)?;
    }
}

/// Restates the Q2 2024 comparative inside the Q2 2025 filing by `bump` millions (more than
/// the rounding) and checks the displayed original survives. A bump that lands on exactly
/// −original is a sign conflict, not a restatement (research plan §4.4, "Sign conflict";
/// `derive` module docs): flagged `sign_conflict` instead of `restated`, the original still
/// displayed and the restatement row still written.
fn restated_comparative_keeps_the_original(
    y0: [f64; 4],
    y1: [f64; 4],
    bump: i64,
) -> Result<(), TestCaseError> {
    let decimals = -6;
    let mut facts = series(&[y0, y1], 2024, decimals);
    let target = facts
        .iter_mut()
        .find(|f| f.filing_id == "ZZZZ_2025_Q2" && f.end == d("2024-06-30"))
        .unwrap();
    let original = target.value;
    target.value += bump as f64 * 1e6;
    let later = target.value;
    let dv = derive(&facts);
    let q2 = dv.cells.iter().find(|c| c.fy == 2024 && c.q == 2).unwrap();
    let sign_flip = (later + original).abs() <= restatement_tolerance(Some(decimals));
    let (flag, not) = if sign_flip {
        (Flag::SignConflict, Flag::Restated)
    } else {
        (Flag::Restated, Flag::SignConflict)
    };
    prop_assert!(
        q2.flags.contains(&flag) && !q2.flags.contains(&not),
        "{original} -> {later}: {:?}",
        q2.flags
    );
    let q1 = dv.cells.iter().find(|c| c.fy == 2024 && c.q == 1).unwrap();
    prop_assert!((q2.standalone.unwrap() + q1.standalone.unwrap() - original).abs() < 1e-3);
    prop_assert_eq!(dv.restatements.len(), 1);
    prop_assert_eq!(dv.restatements[0].original, original);
    prop_assert_eq!(dv.restatements[0].restated, later);
    Ok(())
}

/// The counterexample proptest found (2026-10-29): YTD Q2 2024 rounds to −480M and the bump
/// of 960M makes the comparative +480M. The zero quarters do not matter (bump 959 restates);
/// the exact sign flip does, also with no rounding and no zero quarter (−250M to +250M).
#[test]
fn a_comparative_restated_to_minus_the_original_is_a_sign_conflict_and_keeps_the_original() {
    for (y0, y1, bump) in [
        ([-38_500_000.0, -441_000_000.0, 0.0, 0.0], [0.0; 4], 960),
        ([-38_500_000.0, -441_000_000.0, 0.0, 0.0], [0.0; 4], 959),
        ([120e6, -370e6, 15e6, 40e6], [10e6, 20e6, 30e6, 40e6], 500),
    ] {
        restated_comparative_keeps_the_original(y0, y1, bump).unwrap();
    }
}
