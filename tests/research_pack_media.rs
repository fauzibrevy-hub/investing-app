//! SCMA's pack (`media_scma`) over SCMA-shaped facts: the amounts are SCMA's own XBRL figures
//! for FY2025 to 1H2026, turned into standalone quarters and TTMs the way `derive` does.
//! Dividends paid are untagged in a quarter with no payment, so that ratio must gap, not read
//! zero.

use std::collections::HashMap;

use investing_app::research::packs::{
    evaluate, Ctx, Layer, Outcome, PackSet, Packs, Period, Quarter, Source, View,
};

#[derive(Default)]
struct Facts {
    values: HashMap<(String, i32, u32, &'static str), f64>,
}

fn view_key(view: View) -> &'static str {
    match view {
        View::Instant => "instant",
        View::Standalone => "standalone",
        View::Ttm => "ttm",
        View::Avg4q => "avg4q",
    }
}

impl Facts {
    fn set(&mut self, element: &str, fy: i32, q: u32, view: View, v: f64) {
        self.values
            .insert((element.into(), fy, q, view_key(view)), v);
    }
}

impl Source for Facts {
    fn element(&self, element: &str, dims: &str, q: Quarter, view: View) -> Option<f64> {
        if !dims.is_empty() {
            return None;
        }
        self.values
            .get(&(element.into(), q.fy, q.q, view_key(view)))
            .copied()
    }

    fn kpi(&self, _pack: &str, _metric: &str, _q: Quarter) -> Option<f64> {
        None
    }

    fn unit(&self, _element: &str) -> Option<String> {
        Some("IDR".into())
    }
}

fn eval(set: &PackSet, metric: &str, src: &Facts, q: Quarter) -> Outcome {
    let ctx = Ctx {
        source: src,
        packs: set,
    };
    let pack = set.get("media_scma").unwrap();
    let m = pack.metric(metric).unwrap();
    evaluate(
        &ctx,
        pack,
        m.formula.as_ref().unwrap(),
        m.period.unwrap_or(Period::Standalone),
        q,
    )
}

fn value(o: &Outcome) -> f64 {
    o.value.unwrap_or_else(|| panic!("gap: {:?}", o.gap))
}

const QUARTERS: [(i32, u32); 4] = [(2025, 3), (2025, 4), (2026, 1), (2026, 2)];
const Q1_26: Quarter = Quarter { fy: 2026, q: 1 };
const Q2_26: Quarter = Quarter { fy: 2026, q: 2 };

/// SCMA 2025Q3 to 2026Q2: revenue and >10% customer revenue as standalone quarters, the
/// 30 Jun 2026 balance sheet, and dividend and operating cash TTMs at 2026Q2 only (2026Q1 has
/// no dividends-paid fact, so `derive` leaves its TTM empty).
fn scma() -> Facts {
    let mut f = Facts::default();
    let revenue = [
        1_723_170_465e3,
        1_841_567_631e3,
        1_863_956_570e3,
        1_683_031_254e3,
    ];
    let major = [301_478_079e3, 266_871_408e3, 344_488_015e3, 261_045_748e3];
    for (i, (fy, q)) in QUARTERS.into_iter().enumerate() {
        f.set(
            "idx-cor:SalesAndRevenue",
            fy,
            q,
            View::Standalone,
            revenue[i],
        );
        f.set(
            "idx-cor:SalesAndRevenueMoreThan10Percent",
            fy,
            q,
            View::Standalone,
            major[i],
        );
    }
    f.set(
        "idx-cor:DividendsPaidFromFinancingActivities",
        2026,
        2,
        View::Ttm,
        1_335_214_303e3,
    );
    for q in [1, 2] {
        f.set(
            "idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities",
            2026,
            q,
            View::Ttm,
            1_288_100e6,
        );
    }
    f.set("idx-cor:Goodwill", 2026, 2, View::Instant, 614_400e6);
    f.set(
        "idx-cor:IntangibleAssetsOtherThanGoodwill",
        2026,
        2,
        View::Instant,
        359_100e6,
    );
    f.set("idx-cor:Equity", 2026, 2, View::Instant, 7_396_900e6);
    f
}

#[test]
fn media_scma_claims_no_industry_code_and_keeps_kpis_on_the_disclosed_cadence() {
    let set = Packs::embedded().set();
    let pack = set.get("media_scma").expect("media_scma is embedded");
    assert_eq!(pack.label, "Media & streaming");
    assert!(pack.industry_codes.is_empty());
    assert!(pack.drivers.is_empty());
    // Every KPI is printed each quarter (Investors' Release or interim notes).
    for m in pack.kpi_metrics() {
        assert_eq!(m.cadence.map(|c| c.as_str()), Some("quarterly"), "{}", m.id);
    }
    // Vidio's subscribers, ARPU and rupiah EBITDA are never disclosed, so no row asks for them.
    let kpis: Vec<&str> = pack.kpi_metrics().map(|m| m.id.as_str()).collect();
    for undisclosed in ["vidio_subscribers", "vidio_arpu", "vidio_ebitda"] {
        assert!(!kpis.contains(&undisclosed), "{undisclosed}");
    }
    assert_eq!(kpis.len(), 9);
    assert_eq!(pack.metrics().filter(|m| m.layer == Layer::Xbrl).count(), 7);
}

#[test]
fn major_customer_share_is_the_ttm_of_customers_above_ten_percent_over_revenue() {
    let set = Packs::embedded().set();
    let share = value(&eval(&set, "major_customer_share", &scma(), Q2_26));
    // 1 173,9 bn of 7 111,7 bn TTM revenue (FS FY2025 + 1H2026 - 1H2025).
    let expected = 100.0 * 1_173_883_250e3 / 7_111_725_920e3;
    assert!((share - expected).abs() < 1e-9, "{share}");
    assert!((share - 16.5).abs() < 0.01, "{share}");
}

#[test]
fn dividends_to_ocf_reads_the_derived_ttm_and_gaps_when_a_quarter_paid_nothing() {
    let set = Packs::embedded().set();
    let src = scma();
    let ratio = value(&eval(&set, "dividends_to_ocf", &src, Q2_26));
    assert!((ratio - 100.0 * 1_335_214_303e3 / 1_288_100e6).abs() < 1e-9);
    assert!(ratio > 100.0, "dividends above operating cash: {ratio}");
    // 2026Q1 has no dividends-paid fact: a gap, never a zero ratio.
    let q1 = eval(&set, "dividends_to_ocf", &src, Q1_26);
    assert!(q1.value.is_none(), "{:?}", q1.value);
}

#[test]
fn goodwill_and_brands_over_equity_reads_one_balance_sheet() {
    let set = Packs::embedded().set();
    let r = value(&eval(
        &set,
        "goodwill_intangibles_to_equity",
        &scma(),
        Q2_26,
    ));
    assert!((r - 100.0 * 973_500e6 / 7_396_900e6).abs() < 1e-9, "{r}");
}
