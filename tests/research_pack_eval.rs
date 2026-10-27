//! Pack formula evaluator, pure and without SQLite (research plan §4.3, §7.2): every
//! function, element alternates per period, missing operand → gap, division by zero → gap,
//! currency mismatch → gap, `metric:` and `kpi:` bindings, literal fallbacks.

use std::collections::HashMap;

use investing_app::research::packs::{
    evaluate, Ctx, Gap, Pack, PackSet, Period, Quarter, Source, View,
};

/// `(element, dims, fy, q)`.
type Key = (String, String, i32, u32);
/// `(standalone, ttm, is instant)`.
type Cell = (Option<f64>, Option<f64>, bool);

#[derive(Default)]
struct MapSource {
    elements: HashMap<Key, Cell>,
    units: HashMap<String, String>,
    kpis: HashMap<(String, String, i32, u32), f64>,
}

impl MapSource {
    fn flow(&mut self, element: &str, fy: i32, q: u32, standalone: f64, ttm: Option<f64>) {
        self.elements.insert(
            (element.into(), String::new(), fy, q),
            (Some(standalone), ttm, false),
        );
        self.units.entry(element.into()).or_insert("IDR".into());
    }

    fn instant(&mut self, element: &str, dims: &str, fy: i32, q: u32, value: f64) {
        self.elements.insert(
            (element.into(), dims.into(), fy, q),
            (Some(value), None, true),
        );
        self.units.entry(element.into()).or_insert("IDR".into());
    }

    fn set_kpi(&mut self, pack: &str, metric: &str, fy: i32, q: u32, value: f64) {
        self.kpis.insert((pack.into(), metric.into(), fy, q), value);
    }
}

impl Source for MapSource {
    fn element(&self, element: &str, dims: &str, q: Quarter, view: View) -> Option<f64> {
        let (standalone, ttm, instant) =
            *self
                .elements
                .get(&(element.into(), dims.into(), q.fy, q.q))?;
        match view {
            View::Instant => instant.then_some(standalone).flatten(),
            View::Standalone => standalone,
            View::Ttm => {
                if instant {
                    None
                } else {
                    ttm
                }
            }
            View::Avg4q => None,
        }
    }

    fn kpi(&self, pack: &str, metric: &str, q: Quarter) -> Option<f64> {
        self.kpis
            .get(&(pack.into(), metric.into(), q.fy, q.q))
            .copied()
    }

    fn unit(&self, element: &str) -> Option<String> {
        self.units.get(element).cloned()
    }
}

const PACK: &str = r#"
id = "demo"
version = "1"
label = "Demo"

[[dimension]]
id = "d1"
label = "Satu"
kind = "volume"
  [[dimension.metric]]
  id = "revenue"
  label = "Pendapatan"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "standalone"
  formula = "Revenue"
  elements.Revenue = ["idx-cor:Old", "idx-cor:New"]
  [[dimension.metric]]
  id = "revenue_ttm"
  label = "Pendapatan TTM"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "ttm"
  formula = "ttm(Revenue)"
  elements.Revenue = ["idx-cor:Old", "idx-cor:New"]
  [[dimension.metric]]
  id = "growth"
  label = "Pertumbuhan"
  layer = "xbrl"
  unit = "%"
  direction = "higher"
  why = "Alasan."
  period = "standalone"
  formula = "100 * yoy(Rev)"
  elements.Rev = ["metric:revenue"]
  [[dimension.metric]]
  id = "prev"
  label = "Kuartal lalu"
  layer = "xbrl"
  unit = "ccy"
  direction = "neutral"
  why = "Alasan."
  period = "standalone"
  formula = "lag(Rev, 1)"
  elements.Rev = ["metric:revenue"]
[[dimension]]
id = "d2"
label = "Dua"
kind = "pricing"
  [[dimension.metric]]
  id = "per_unit"
  label = "Per unit"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "ttm"
  formula = "ttm(Revenue) / ttm(Units)"
  elements.Revenue = ["idx-cor:New"]
  elements.Units = ["kpi:units"]
  [[dimension.metric]]
  id = "units"
  label = "Unit"
  layer = "kpi"
  unit = "unit"
  direction = "higher"
  why = "Alasan."
  cadence = "quarterly"
[[dimension]]
id = "d3"
label = "Tiga"
kind = "cost"
  [[dimension.metric]]
  id = "turnover"
  label = "Perputaran"
  layer = "xbrl"
  unit = "x"
  direction = "higher"
  why = "Alasan."
  period = "ttm"
  formula = "ttm(Revenue) / avg4q(Assets + Extra)"
  elements.Revenue = ["idx-cor:New"]
  elements.Assets = ["idx-cor:Assets"]
  elements.Extra = ["idx-cor:Missing", "0"]
[[dimension]]
id = "d4"
label = "Empat"
kind = "quality"
  [[dimension.metric]]
  id = "mixed"
  label = "Campur"
  layer = "xbrl"
  unit = "x"
  direction = "neutral"
  why = "Alasan."
  period = "standalone"
  formula = "Usd / Idr"
  elements.Usd = ["idx-cor:UsdThing"]
  elements.Idr = ["idx-cor:New"]
[[dimension]]
id = "d5"
label = "Lima"
kind = "balance"
  [[dimension.metric]]
  id = "vessels"
  label = "Kapal"
  layer = "xbrl"
  unit = "ccy"
  direction = "neutral"
  why = "Alasan."
  period = "instant"
  formula = "V"
  elements.V = ["idx-cor:Ppe[idx-cor:Class=idx-cor:Vessels;idx-cor:Amount=idx-cor:Gross]"]
  [[dimension.metric]]
  id = "ratio_zero"
  label = "Nol"
  layer = "xbrl"
  unit = "x"
  direction = "neutral"
  why = "Alasan."
  period = "standalone"
  formula = "Rev / Zero"
  elements.Rev = ["metric:revenue"]
  elements.Zero = ["0"]
"#;

const CORE: &str = r#"
id = "core"
version = "1"
label = "Inti"
[[dimension]]
id = "c"
label = "C"
kind = "core"
  [[dimension.metric]]
  id = "assets"
  label = "Aset"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "instant"
  formula = "A"
  elements.A = ["idx-cor:Assets"]
"#;

fn packs() -> PackSet {
    PackSet::new(vec![Pack::parse(CORE).unwrap(), Pack::parse(PACK).unwrap()]).unwrap()
}

fn source() -> MapSource {
    let mut s = MapSource::default();
    // 2024: the old element name; 2025: the new one (a taxonomy rename).
    for (i, q) in (1..=4).enumerate() {
        s.flow("idx-cor:Old", 2024, q, 100.0 + i as f64, None);
    }
    for (i, q) in (1..=4).enumerate() {
        let v = 110.0 + i as f64;
        s.flow("idx-cor:New", 2025, q, v, Some(v * 4.0));
    }
    for q in 1..=4 {
        s.instant("idx-cor:Assets", "", 2024, q, 1000.0);
        s.instant("idx-cor:Assets", "", 2025, q, 1000.0 + q as f64 * 10.0);
        s.set_kpi("demo", "units", 2025, q, 10.0);
    }
    s.instant(
        "idx-cor:Ppe",
        "idx-cor:Amount=idx-cor:Gross;idx-cor:Class=idx-cor:Vessels",
        2025,
        4,
        555.0,
    );
    s.flow("idx-cor:UsdThing", 2025, 4, 5.0, None);
    s.units.insert("idx-cor:UsdThing".into(), "USD".into());
    s
}

fn run(metric: &str, q: Quarter) -> investing_app::research::packs::Outcome {
    let set = packs();
    let src = source();
    let ctx = Ctx {
        source: &src,
        packs: &set,
    };
    let pack = set.get("demo").unwrap();
    let m = pack.metric(metric).unwrap();
    evaluate(
        &ctx,
        pack,
        m.formula.as_ref().unwrap(),
        m.period.unwrap_or(Period::Standalone),
        q,
    )
}

#[test]
fn alternates_are_tried_in_order_per_period() {
    // 2024 only has the old name, 2025 only the new one: both periods populate.
    let old = run("revenue", Quarter::new(2024, 2));
    assert_eq!(old.value, Some(101.0));
    assert_eq!(old.inputs[0].source, "idx-cor:Old");
    let new = run("revenue", Quarter::new(2025, 2));
    assert_eq!(new.value, Some(111.0));
    assert_eq!(new.inputs[0].source, "idx-cor:New");
    assert_eq!(new.inputs[0].view, View::Standalone);
}

#[test]
fn ttm_sums_four_standalone_quarters_across_a_rename() {
    // 2025Q2 TTM = 2024Q3 + 2024Q4 + 2025Q1 + 2025Q2 (old, old, new, new).
    let o = run("revenue_ttm", Quarter::new(2025, 2));
    assert_eq!(o.value, Some(102.0 + 103.0 + 110.0 + 111.0));
    assert_eq!(o.inputs.len(), 4);
}

#[test]
fn yoy_and_lag_read_earlier_quarters_through_metric_references() {
    let g = run("growth", Quarter::new(2025, 3));
    let expected = (112.0 / 102.0 - 1.0) * 100.0;
    assert!((g.value.unwrap() - expected).abs() < 1e-9);
    let p = run("prev", Quarter::new(2025, 1));
    assert_eq!(p.value, Some(103.0), "lag crosses the year boundary");
}

#[test]
fn kpi_bindings_sum_under_ttm_and_literal_fallbacks_fill_gaps() {
    let o = run("per_unit", Quarter::new(2025, 4));
    assert_eq!(o.value, Some((110.0 + 111.0 + 112.0 + 113.0) / 40.0));
    let t = run("turnover", Quarter::new(2025, 4));
    // avg4q over five instants: 2024Q4 .. 2025Q4, with `Extra` falling back to 0.
    let avg = (1000.0 + 1010.0 + 1020.0 + 1030.0 + 1040.0) / 5.0;
    assert!((t.value.unwrap() - 446.0 / avg).abs() < 1e-9);
    assert!(t
        .inputs
        .iter()
        .any(|u| u.ident == "Extra" && u.source == "0"));
}

#[test]
fn missing_operand_is_a_gap_never_zero() {
    // TTM at 2025Q3 needs 2024Q4, which only the old tag carries and `per_unit` does not
    // bind: the gap names the first operand that failed, left to right.
    let o = run("per_unit", Quarter::new(2025, 3));
    assert_eq!(o.value, None);
    assert!(
        matches!(&o.gap, Some(Gap::Missing { ident, q, view: View::Standalone }) if ident == "Revenue" && *q == Quarter::new(2024, 4)),
        "{:?}",
        o.gap
    );
    let o = run("revenue", Quarter::new(2023, 1));
    assert!(matches!(o.gap, Some(Gap::Missing { .. })));
    assert_eq!(
        o.gap.unwrap().describe(),
        "Revenue tidak ada untuk 2023Q1 (kuartal)"
    );
}

#[test]
fn division_by_zero_is_a_gap() {
    let o = run("ratio_zero", Quarter::new(2025, 4));
    assert_eq!(o.value, None);
    assert_eq!(o.gap, Some(Gap::DivisionByZero));
    // yoy against a zero base too.
    let mut s = source();
    s.flow("idx-cor:New", 2026, 1, 0.0, None);
    s.flow("idx-cor:New", 2027, 1, 5.0, None);
    let set = packs();
    let ctx = Ctx {
        source: &s,
        packs: &set,
    };
    let pack = set.get("demo").unwrap();
    let m = pack.metric("growth").unwrap();
    let o = evaluate(
        &ctx,
        pack,
        m.formula.as_ref().unwrap(),
        Period::Standalone,
        Quarter::new(2027, 1),
    );
    assert_eq!(o.gap, Some(Gap::DivisionByZero));
}

#[test]
fn mixed_currencies_are_a_gap_and_dimensioned_instants_resolve() {
    let o = run("mixed", Quarter::new(2025, 4));
    assert_eq!(o.value, None);
    assert_eq!(
        o.gap,
        Some(Gap::UnitMismatch {
            units: vec!["IDR".into(), "USD".into()]
        })
    );
    let v = run("vessels", Quarter::new(2025, 4));
    assert_eq!(v.value, Some(555.0));
    assert_eq!(v.inputs[0].view, View::Instant);
    // The same element without the dims is not the vessel figure.
    assert_eq!(run("vessels", Quarter::new(2025, 3)).value, None);
}

#[test]
fn instants_do_not_answer_ttm_and_core_metrics_resolve_from_other_packs() {
    let set = packs();
    let src = source();
    let ctx = Ctx {
        source: &src,
        packs: &set,
    };
    let core = set.core();
    let m = core.metric("assets").unwrap();
    let o = evaluate(
        &ctx,
        core,
        m.formula.as_ref().unwrap(),
        Period::Ttm,
        Quarter::new(2025, 4),
    );
    assert!(matches!(
        o.gap,
        Some(Gap::Missing {
            view: View::Ttm,
            ..
        })
    ));
    assert_eq!(
        set.resolve(set.get("demo").unwrap(), "core.assets")
            .unwrap()
            .1
            .id,
        "assets"
    );
    assert!(set.resolve(set.get("demo").unwrap(), "core.nope").is_none());
}
