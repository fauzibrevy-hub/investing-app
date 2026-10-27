//! The pack section payload (design.md §4.2 Paket; research plan §5 `GET /{ticker}/pack`):
//! core plus the ticker's industry pack, one card per dimension, each metric with twelve
//! standalone quarters, the latest value, YoY, the own-history percentile where ten years
//! allow, peer values, KPI cells with provenance, drivers without a source listed rather
//! than hidden, and the coverage the Desk shows.
//!
//! `StoreSource` adapts the store to the evaluator's `Source`; everything else is assembly.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};
use serde_json::{json, Value};

use super::def::{Binding, Cadence, Formula, Layer, Metric, Pack, PackSet, Period};
use super::eval::{self, percentile_rank, Ctx, Outcome, Quarter, Source, View};
use crate::research::derive::Flag;
use crate::research::events::{self, KpiStatus};
use crate::research::kpi::KpiValue;
use crate::research::prices::drivers as catalogue;
use crate::research::store::{DriverSummary, Store, StoreError};
use crate::research::universe::{self, UniverseEntry};
use crate::research::xbrl::Cite;

/// Quarters shown per metric (design.md §4.2).
pub const SHOWN_QUARTERS: u32 = 12;
/// Own-history window for the percentile: ten years.
pub const HISTORY_QUARTERS: u32 = 40;
/// Points a percentile needs: three years of quarters, or three annual values.
pub const MIN_HISTORY_QUARTERLY: usize = 12;
pub const MIN_HISTORY_ANNUAL: usize = 3;
pub const HISTORY_CAPTION: &str = "riwayat < 3 th";

pub fn quarter_end_date(q: Quarter) -> Option<NaiveDate> {
    match q.q {
        1 => NaiveDate::from_ymd_opt(q.fy, 3, 31),
        2 => NaiveDate::from_ymd_opt(q.fy, 6, 30),
        3 => NaiveDate::from_ymd_opt(q.fy, 9, 30),
        4 => NaiveDate::from_ymd_opt(q.fy, 12, 31),
        _ => None,
    }
}

/// The latest completed calendar quarter before `today`.
pub fn last_completed_quarter(today: NaiveDate) -> Quarter {
    let q = (today.month0() / 3) + 1;
    Quarter::new(today.year(), q).back(1)
}

/// `(standalone, ttm, is instant)` of one derived cell.
type DerivedCell = (Option<f64>, Option<f64>, bool);

/// A fact of a PDF filing that fed a cell: the filing and the page it was read from.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfSource {
    pub filing_id: String,
    pub element: String,
    pub cite: Cite,
}

/// Which inputs of a derived cell a view rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum InputsOf {
    Standalone,
    Ttm,
}

/// One ticker's facts, derived cells and KPIs, indexed for the evaluator.
pub struct StoreSource {
    derived: HashMap<(String, i32, u32), DerivedCell>,
    /// Facts read from a PDF filing behind each derived cell's standalone and TTM views.
    pdf: HashMap<(String, i32, u32, InputsOf), Vec<PdfSource>>,
    dims: HashMap<(String, String), BTreeMap<NaiveDate, f64>>,
    units: HashMap<String, String>,
    kpis: HashMap<(String, i32, u32), KpiValue>,
    cadence: HashMap<String, Cadence>,
}

impl StoreSource {
    /// Loads what the applied packs can reference for `ticker`. Dimensioned instants are
    /// fetched only for the bindings that name dims.
    pub fn load(store: &Store, ticker: &str, applied: &[&Pack]) -> Result<Self, StoreError> {
        let mut derived = HashMap::new();
        let mut pdf: HashMap<(String, i32, u32, InputsOf), Vec<PdfSource>> = HashMap::new();
        let cites = store.pdf_cites(ticker)?;
        for r in store.derived(ticker, None)? {
            derived.insert(
                (r.element.clone(), r.fy, r.q),
                (r.standalone, r.ttm, r.flags.contains(&Flag::Instant)),
            );
            if cites.is_empty() {
                continue;
            }
            for (which, inputs) in [
                (InputsOf::Standalone, &r.inputs.standalone),
                (InputsOf::Ttm, &r.inputs.ttm),
            ] {
                for input in inputs {
                    let Some(cite) = cites.get(&input.fact) else {
                        continue;
                    };
                    let list = pdf
                        .entry((r.element.clone(), r.fy, r.q, which))
                        .or_default();
                    let source = PdfSource {
                        filing_id: input.fact.filing_id.clone(),
                        element: input.fact.element.clone(),
                        cite: cite.clone(),
                    };
                    if !list.contains(&source) {
                        list.push(source);
                    }
                }
            }
        }
        let mut dims: HashMap<(String, String), BTreeMap<NaiveDate, f64>> = HashMap::new();
        let mut cadence = HashMap::new();
        for pack in applied {
            for m in pack.metrics() {
                if let Some(c) = m.cadence {
                    cadence.insert(m.id.clone(), c);
                }
                let Some(f) = &m.formula else { continue };
                for alts in f.bindings.values() {
                    for b in alts {
                        if let Binding::Element {
                            element, dims: d, ..
                        } = b
                        {
                            if d.is_empty() || dims.contains_key(&(element.clone(), d.clone())) {
                                continue;
                            }
                            let rows = store.dimensioned_instants(ticker, element, d)?;
                            let mut by_date = BTreeMap::new();
                            for (end, value) in rows {
                                by_date.entry(end).or_insert(value);
                            }
                            dims.insert((element.clone(), d.clone()), by_date);
                        }
                    }
                }
            }
        }
        let kpis = store
            .kpis(ticker)?
            .into_iter()
            .map(|k| ((k.metric.clone(), k.fy, k.q), k))
            .collect();
        Ok(Self {
            derived,
            pdf,
            dims,
            units: store.element_units(ticker)?,
            kpis,
            cadence,
        })
    }

    /// The PDF-filing facts one operand rests on: the inputs of the derived cell it reads at
    /// `q` in `view` (the mean of five instants rests on all five). Empty for an XBRL cell.
    pub fn pdf_sources(&self, element: &str, q: Quarter, view: View) -> Vec<&PdfSource> {
        let at = |q: Quarter, which: InputsOf| {
            self.pdf
                .get(&(element.to_string(), q.fy, q.q, which))
                .into_iter()
                .flatten()
        };
        match view {
            View::Instant | View::Standalone => at(q, InputsOf::Standalone).collect(),
            View::Ttm => at(q, InputsOf::Ttm).collect(),
            View::Avg4q => (0..5)
                .flat_map(|i| at(q.back(i), InputsOf::Standalone))
                .collect(),
        }
    }

    /// The stored KPI row for a metric at a quarter (annual KPIs sit at Q4).
    pub fn kpi_row(&self, metric: &str, q: Quarter) -> Option<&KpiValue> {
        self.kpis.get(&(metric.to_string(), q.fy, q.q))
    }

    /// Where an annual KPI's value for quarter `q` lives: Q4 of the same year at Q4, else
    /// Q4 of the year before (ASUMSI R2.3: annual values carry forward within the year).
    fn kpi_key(&self, metric: &str, q: Quarter) -> Quarter {
        match self.cadence.get(metric) {
            Some(Cadence::Annual) if q.q != 4 => Quarter::new(q.fy - 1, 4),
            _ => q,
        }
    }
}

impl Source for StoreSource {
    fn element(&self, element: &str, dims: &str, q: Quarter, view: View) -> Option<f64> {
        if !dims.is_empty() {
            if !matches!(view, View::Instant | View::Standalone) {
                return None;
            }
            let end = quarter_end_date(q)?;
            return self
                .dims
                .get(&(element.to_string(), dims.to_string()))?
                .get(&end)
                .copied();
        }
        let (standalone, ttm, instant) = *self.derived.get(&(element.to_string(), q.fy, q.q))?;
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

    fn kpi(&self, _pack: &str, metric: &str, q: Quarter) -> Option<f64> {
        let key = self.kpi_key(metric, q);
        let row = self.kpis.get(&(metric.to_string(), key.fy, key.q))?;
        if row.unknown {
            None
        } else {
            row.value
        }
    }

    fn unit(&self, element: &str) -> Option<String> {
        self.units.get(element).cloned()
    }
}

fn gap_json(o: &Outcome) -> Value {
    match &o.gap {
        Some(g) => {
            let mut v = serde_json::to_value(g).unwrap_or(Value::Null);
            v["text"] = json!(g.describe());
            v
        }
        None => Value::Null,
    }
}

fn inputs_json(o: &Outcome) -> Value {
    json!(o
        .inputs
        .iter()
        .map(|u| json!({
            "ident": u.ident, "source": u.source, "q": u.q.label(), "view": u.view, "value": u.value,
        }))
        .collect::<Vec<_>>())
}

fn out_of_range(m: &Metric, v: f64) -> bool {
    m.range.is_some_and(|(lo, hi)| v < lo || v > hi)
}

/// The PDF page of every operand of `o` that was read from a PDF filing, once each.
fn pdf_sources(src: &StoreSource, o: &Outcome) -> Vec<Value> {
    let mut seen: Vec<&PdfSource> = Vec::new();
    for u in &o.inputs {
        for p in src.pdf_sources(&u.source, u.q, u.view) {
            if !seen.contains(&p) {
                seen.push(p);
            }
        }
    }
    seen.iter()
        .map(|p| {
            json!({
                "kind": "pdf", "filing_id": p.filing_id, "element": p.element,
                "page": p.cite.page, "label": p.cite.label,
            })
        })
        .collect()
}

fn xbrl_cell(m: &Metric, q: Quarter, o: &Outcome, src: &StoreSource) -> Value {
    let mut flags = Vec::new();
    if o.value.is_some_and(|v| out_of_range(m, v)) {
        flags.push("out_of_range");
    }
    let mut cell = json!({
        "fy": q.fy, "q": q.q, "label": q.label(),
        "value": o.value,
        "gap": gap_json(o),
        "flags": flags,
        "inputs": inputs_json(o),
    });
    // Only a cell that rests on a PDF filing carries the key, so XBRL cells are unchanged.
    let sources = pdf_sources(src, o);
    if !sources.is_empty() {
        cell["sources"] = Value::Array(sources);
    }
    cell
}

/// Why a ratio has no YoY when its denominator was negative in either period.
const YOY_NEGATIVE_DENOMINATOR: &str = "tidak bermakna: penyebut negatif";

/// YoY of `now` against `then`, one rule for every metric:
/// - `%` metrics: percentage points, `now - then`;
/// - ratios (`x`): the difference in times, `now - then`, since a percent change of a multiple
///   reads as noise (net debt / EBITDA 2,1 to 5,2 is +3,1x, not +148%);
/// - levels: `(now - then) / |then| * 100`, so the sign follows the direction of the move even
///   on a negative base;
/// - the sign crossing zero has no meaningful percentage: `sign_change` with both values;
/// - a zero base, or a ratio (`x`) with a negative denominator in either period: `null`.
fn yoy_json(unit: &str, now: Option<f64>, then: Option<f64>, denominator_negative: bool) -> Value {
    match (now, then) {
        (Some(a), Some(b)) if unit == "%" || unit == "pp" => {
            json!({"kind": "pp", "value": a - b})
        }
        (Some(_), Some(_)) if unit == "x" && denominator_negative => Value::Null,
        (Some(a), Some(b)) if unit == "x" => json!({"kind": "x", "value": a - b}),
        (Some(a), Some(b)) if b != 0.0 && (a == 0.0 || a.signum() == b.signum()) => {
            json!({"kind": "pct", "value": (a - b) / b.abs() * 100.0})
        }
        (Some(a), Some(b)) if b != 0.0 => json!({"kind": "sign_change", "from": b, "to": a}),
        _ => Value::Null,
    }
}

/// The reason a ratio's YoY is absent although both values exist; `null` otherwise.
fn yoy_reason(
    unit: &str,
    now: Option<f64>,
    then: Option<f64>,
    denominator_negative: bool,
) -> Value {
    if unit == "x" && denominator_negative && now.is_some() && then.is_some() {
        json!(YOY_NEGATIVE_DENOMINATOR)
    } else {
        Value::Null
    }
}

/// True when the formula's denominator is negative at `q` or a year earlier.
fn denominator_negative(
    ctx: &Ctx<'_>,
    pack: &Pack,
    formula: &Formula,
    period: Period,
    q: Quarter,
) -> bool {
    let Some(den) = eval::denominator(&formula.expr) else {
        return false;
    };
    let den_formula = Formula {
        text: String::new(),
        expr: den.clone(),
        bindings: formula.bindings.clone(),
    };
    [q, q.back(4)].into_iter().any(|at| {
        eval::evaluate(ctx, pack, &den_formula, period, at)
            .value
            .is_some_and(|v| v < 0.0)
    })
}

fn percentile_json(history: &[f64], current: Option<f64>, min_points: usize, years: f64) -> Value {
    let n = history.len();
    match current {
        Some(c) if n >= min_points => json!({
            "value": percentile_rank(history, c),
            "n": n,
            "years": years,
            "caption": format!("p{} · {} th", percentile_rank(history, c).map_or(0, |p| p.round() as i64), years.round() as i64),
        }),
        _ => json!({"value": null, "n": n, "years": years, "caption": HISTORY_CAPTION}),
    }
}

/// A peer's data, loaded once per request.
struct Peer {
    ticker: String,
    latest: Quarter,
    source: StoreSource,
}

fn metric_json_xbrl(
    packs: &PackSet,
    pack: &Pack,
    m: &Metric,
    shown: &[Quarter],
    src: &StoreSource,
    peers: &[Peer],
) -> (Value, bool) {
    let formula = m.formula.as_ref().expect("xbrl metric has a formula");
    let period = m.period.unwrap_or(super::def::Period::Standalone);
    let ctx = Ctx { source: src, packs };
    let latest_q = *shown.last().expect("shown quarters");
    let history: Vec<(Quarter, Outcome)> = (0..HISTORY_QUARTERS)
        .rev()
        .map(|i| latest_q.back(i))
        .map(|q| (q, eval::evaluate(&ctx, pack, formula, period, q)))
        .collect();
    let value_at = |q: Quarter| -> Option<f64> {
        history
            .iter()
            .find(|(hq, _)| *hq == q)
            .and_then(|(_, o)| o.value)
    };
    let cells: Vec<Value> = shown
        .iter()
        .map(|q| {
            let o = history
                .iter()
                .find(|(hq, _)| hq == q)
                .map(|(_, o)| o)
                .expect("shown quarters are inside the history");
            xbrl_cell(m, *q, o, src)
        })
        .collect();
    let latest = shown
        .iter()
        .rev()
        .find_map(|q| value_at(*q).map(|v| (*q, v)));
    let populated_at_latest = value_at(latest_q).is_some();
    let den_negative = |q: Quarter| denominator_negative(&ctx, pack, formula, period, q);
    let hist_values: Vec<f64> = history.iter().filter_map(|(_, o)| o.value).collect();
    let years = hist_values.len() as f64 / 4.0;
    let peers_json: Vec<Value> = peers
        .iter()
        .map(|p| {
            let pctx = Ctx {
                source: &p.source,
                packs,
            };
            let o = eval::evaluate(&pctx, pack, formula, period, p.latest);
            json!({"ticker": p.ticker, "fy": p.latest.fy, "q": p.latest.q, "value": o.value, "gap": gap_json(&o)})
        })
        .collect();
    let bindings: BTreeMap<&str, Vec<String>> = formula
        .bindings
        .iter()
        .map(|(k, v)| (k.as_str(), v.iter().map(Binding::source).collect()))
        .collect();
    (
        json!({
            "id": m.id, "label": m.label, "layer": m.layer.as_str(), "unit": m.unit,
            "direction": m.direction.as_str(), "why": m.why, "note": m.note,
            "formula": formula.text, "period": period.as_str(), "bindings": bindings, "range": m.range,
            "cells": cells,
            "latest": latest.map(|(q, v)| json!({"fy": q.fy, "q": q.q, "label": q.label(), "value": v})),
            "yoy": latest.map_or(Value::Null, |(q, v)| yoy_json(&m.unit, Some(v), value_at(q.back(4)), den_negative(q))),
            "yoy_reason": latest.map_or(Value::Null, |(q, v)| yoy_reason(&m.unit, Some(v), value_at(q.back(4)), den_negative(q))),
            "percentile": percentile_json(&hist_values, latest.map(|(_, v)| v), MIN_HISTORY_QUARTERLY, years),
            "peers": peers_json,
        }),
        populated_at_latest,
    )
}

fn kpi_cell(m: &Metric, q: Quarter, row: Option<&KpiValue>, applicable: bool) -> Value {
    if !applicable {
        return json!({"fy": q.fy, "q": q.q, "label": q.label(), "state": "not_applicable"});
    }
    match row {
        Some(r) => {
            let mut v = r.to_json();
            v["label"] = json!(q.label());
            if r.value.is_some_and(|x| out_of_range(m, x)) {
                v["flags"] = json!(["out_of_range"]);
            } else {
                v["flags"] = json!([]);
            }
            v
        }
        None => {
            json!({"fy": q.fy, "q": q.q, "label": q.label(), "state": "not_entered", "action": "Isi"})
        }
    }
}

fn metric_json_kpi(
    m: &Metric,
    shown: &[Quarter],
    src: &StoreSource,
    peers: &[Peer],
) -> (Value, KpiStatus) {
    let cadence = m.cadence.unwrap_or(Cadence::Quarterly);
    let applicable = |q: &Quarter| cadence == Cadence::Quarterly || q.q == 4;
    let cells: Vec<Value> = shown
        .iter()
        .map(|q| kpi_cell(m, *q, src.kpi_row(&m.id, *q), applicable(q)))
        .collect();
    let latest_q = *shown.last().expect("shown quarters");
    let history: Vec<(Quarter, f64)> = (0..HISTORY_QUARTERS)
        .rev()
        .map(|i| latest_q.back(i))
        .filter(applicable)
        .filter_map(|q| {
            src.kpi_row(&m.id, q)
                .filter(|r| !r.unknown)
                .and_then(|r| r.value.map(|v| (q, v)))
        })
        .collect();
    let latest = history.last().copied();
    let value_at = |q: Quarter| history.iter().find(|(hq, _)| *hq == q).map(|(_, v)| *v);
    let hist_values: Vec<f64> = history.iter().map(|(_, v)| *v).collect();
    let (min_points, years) = match cadence {
        Cadence::Quarterly => (MIN_HISTORY_QUARTERLY, hist_values.len() as f64 / 4.0),
        Cadence::Annual => (MIN_HISTORY_ANNUAL, hist_values.len() as f64),
    };
    let peers_json: Vec<Value> = peers
        .iter()
        .map(|p| {
            let latest = (0..HISTORY_QUARTERS)
                .map(|i| p.latest.back(i))
                .filter(applicable)
                .find_map(|q| {
                    p.source
                        .kpi_row(&m.id, q)
                        .filter(|r| !r.unknown)
                        .and_then(|r| r.value.map(|v| (q, v)))
                });
            match latest {
                Some((q, v)) => json!({"ticker": p.ticker, "fy": q.fy, "q": q.q, "value": v, "gap": null}),
                None => json!({"ticker": p.ticker, "fy": null, "q": null, "value": null, "gap": {"kind": "not_entered", "text": "belum diisi"}}),
            }
        })
        .collect();
    let expected = events::expected_quarters(cadence, shown);
    let status = KpiStatus {
        metric: m.id.clone(),
        cadence,
        entered: expected
            .iter()
            .filter(|q| src.kpi_row(&m.id, **q).is_some_and(|r| !r.unknown))
            .copied()
            .collect(),
        unknown: expected
            .iter()
            .filter(|q| src.kpi_row(&m.id, **q).is_some_and(|r| r.unknown))
            .copied()
            .collect(),
        expected,
    };
    (
        json!({
            "id": m.id, "label": m.label, "layer": m.layer.as_str(), "unit": m.unit,
            "direction": m.direction.as_str(), "why": m.why, "note": m.note,
            "cadence": cadence.as_str(), "range": m.range,
            "cells": cells,
            "latest": latest.map(|(q, v)| json!({"fy": q.fy, "q": q.q, "label": q.label(), "value": v})),
            "yoy": latest.map_or(Value::Null, |(q, v)| yoy_json(&m.unit, Some(v), value_at(q.back(4)), false)),
            "yoy_reason": null,
            "percentile": percentile_json(&hist_values, latest.map(|(_, v)| v), min_points, years),
            "peers": peers_json,
        }),
        status,
    )
}

/// Stored driver series by id, for the status words below.
pub type DriverIndex = HashMap<String, DriverSummary>;

pub fn driver_index(store: &Store) -> Result<DriverIndex, StoreError> {
    Ok(store
        .driver_summaries()?
        .into_iter()
        .map(|s| (s.series.clone(), s))
        .collect())
}

/// `TBD` without a source, `belum diambil` with a source but nothing stored yet,
/// `tersedia` once the series exists in the store (A3.5; design.md §4.4).
pub fn driver_status(has_source: bool, stored: Option<&DriverSummary>) -> &'static str {
    match (has_source, stored) {
        (false, _) => "TBD",
        (true, None) => "belum diambil",
        (true, Some(_)) => "tersedia",
    }
}

fn series_json(s: Option<&DriverSummary>) -> Value {
    s.map_or(Value::Null, |s| {
        json!({"points": s.points, "first": s.first, "last": s.last, "fetched_at": s.fetched_at})
    })
}

/// One pack driver with its status, catalogue details and labelled proxy.
pub fn driver_json(d: &super::def::Driver, index: &DriverIndex) -> Value {
    let spec = catalogue::spec(&d.id);
    let stored = index.get(&d.id);
    let proxy = d.proxy.as_deref().and_then(catalogue::spec).map(|p| {
        let ps = index.get(p.id);
        json!({
            "id": p.id, "label": p.label, "source": p.source, "unit": p.unit,
            "cadence": p.cadence.as_str(), "note": p.note, "proxy": true,
            "status": driver_status(p.has_source(), ps), "series": series_json(ps),
        })
    });
    json!({
        "id": d.id, "label": d.label, "source": d.source,
        "compare_metric": d.compare_metric,
        "status": driver_status(d.has_source(), stored),
        "unit": spec.map(|s| s.unit), "cadence": spec.map(|s| s.cadence.as_str()),
        "note": spec.map(|s| s.note),
        "series": series_json(stored),
        "proxy": proxy,
    })
}

fn metric_json_driver(pack: &Pack, m: &Metric, index: &DriverIndex) -> Value {
    let driver = m
        .driver
        .as_deref()
        .and_then(|id| pack.drivers.iter().find(|d| d.id == id));
    json!({
        "id": m.id, "label": m.label, "layer": m.layer.as_str(), "unit": m.unit,
        "direction": m.direction.as_str(), "why": m.why, "note": m.note,
        "driver": driver.map(|d| driver_json(d, index)),
        "cells": [], "latest": null, "yoy": null, "yoy_reason": null, "percentile": null, "peers": [],
    })
}

/// Latest quarter a ticker has a filing for, else the latest completed calendar quarter.
fn latest_quarter(
    store: &Store,
    ticker: &str,
    today: NaiveDate,
) -> Result<(Quarter, bool), StoreError> {
    Ok(match store.filings(Some(ticker))?.first() {
        Some(f) => (Quarter::new(f.filing.fy, f.filing.period.quarter()), true),
        None => (last_completed_quarter(today), false),
    })
}

/// Builds `GET /api/research/{ticker}/pack` for a universe row.
pub fn build(
    store: &Store,
    packs: &PackSet,
    entry: &UniverseEntry,
    peer_tickers: &[String],
    today: NaiveDate,
    now: &str,
) -> Result<Value, StoreError> {
    let ticker = entry.ticker.as_str();
    let codes = store.latest_industry_codes()?;
    let choice = universe::pack_choice(packs, entry, codes.get(ticker).map(String::as_str));
    let industry = choice.id.as_deref().and_then(|id| packs.get(id));
    let mut applied: Vec<&Pack> = vec![packs.core()];
    applied.extend(industry);

    let (latest_q, has_filings) = latest_quarter(store, ticker, today)?;
    let shown: Vec<Quarter> = (0..SHOWN_QUARTERS)
        .rev()
        .map(|i| latest_q.back(i))
        .collect();
    let src = StoreSource::load(store, ticker, &applied)?;
    let index = driver_index(store)?;
    let mut peers = Vec::new();
    for p in peer_tickers.iter().take(universe::MAX_PEERS) {
        let (latest, _) = latest_quarter(store, p, today)?;
        peers.push(Peer {
            ticker: p.clone(),
            latest,
            source: StoreSource::load(store, p, &applied)?,
        });
    }

    let mut dimensions = Vec::new();
    let mut xbrl_populated = Vec::new();
    let mut kpi_status = Vec::new();
    for pack in &applied {
        for d in &pack.dimensions {
            let metrics: Vec<Value> = d
                .metrics
                .iter()
                .map(|m| match m.layer {
                    Layer::Xbrl => {
                        let (v, populated) = metric_json_xbrl(packs, pack, m, &shown, &src, &peers);
                        xbrl_populated.push(populated);
                        v
                    }
                    Layer::Kpi => {
                        let (v, status) = metric_json_kpi(m, &shown, &src, &peers);
                        kpi_status.push(status);
                        v
                    }
                    Layer::Driver => metric_json_driver(pack, m, &index),
                })
                .collect();
            dimensions.push(json!({
                "pack": pack.id, "id": d.id, "label": d.label, "kind": d.kind.as_str(), "metrics": metrics,
            }));
        }
    }
    let drivers: Vec<Value> = industry
        .map(|p| p.drivers.iter().map(|d| driver_json(d, &index)).collect())
        .unwrap_or_default();
    let missing_drivers: Vec<Value> = industry
        .map(|p| {
            p.missing_drivers()
                .map(|d| json!({"id": d.id, "label": d.label, "proxy": d.proxy}))
                .collect()
        })
        .unwrap_or_default();
    let coverage = events::coverage(industry.is_some(), latest_q, &xbrl_populated, &kpi_status);
    Ok(json!({
        "ticker": ticker,
        "as_of": now,
        "role": entry.role.as_str(),
        "business_note": entry.business_note,
        "pack": {
            "id": choice.id, "source": choice.source, "filing_code": choice.filing_code,
            "filing_default": choice.filing_default, "override": choice.override_,
            "version": industry.map(|p| p.version.clone()), "label": industry.map(|p| p.label.clone()),
            "action": if industry.is_none() { Some("Tentukan paket") } else { None },
        },
        "core": {"id": packs.core().id, "version": packs.core().version},
        "latest": {"fy": latest_q.fy, "q": latest_q.q, "label": latest_q.label(), "from_filing": has_filings},
        "quarters": shown.iter().map(|q| json!({"fy": q.fy, "q": q.q, "label": q.label()})).collect::<Vec<_>>(),
        "dimensions": dimensions,
        "drivers": drivers,
        "missing_drivers": missing_drivers,
        "peers": peer_tickers.iter().take(universe::MAX_PEERS).collect::<Vec<_>>(),
        "coverage": coverage,
    }))
}

/// `GET /api/research/packs`: every pack definition rendered read-only (design.md §4.4).
pub fn definitions_json(
    packs: &PackSet,
    members: &BTreeMap<String, Vec<String>>,
    index: &DriverIndex,
) -> Vec<Value> {
    packs
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "version": p.version, "label": p.label,
                "industry_codes": p.industry_codes, "taxonomies": p.taxonomies,
                "members": members.get(&p.id).cloned().unwrap_or_default(),
                "dimensions": p.dimensions.iter().map(|d| json!({
                    "id": d.id, "label": d.label, "kind": d.kind.as_str(),
                    "metrics": d.metrics.iter().map(|m| json!({
                        "id": m.id, "label": m.label, "layer": m.layer.as_str(), "unit": m.unit,
                        "direction": m.direction.as_str(), "why": m.why, "note": m.note,
                        "formula": m.formula.as_ref().map(|f| f.text.clone()),
                        "bindings": m.formula.as_ref().map(|f| f.bindings.iter()
                            .map(|(k, v)| (k.clone(), json!(v.iter().map(Binding::source).collect::<Vec<_>>())))
                            .collect::<serde_json::Map<String, Value>>()),
                        "period": m.period.map(|p| p.as_str()),
                        "cadence": m.cadence.map(|c| c.as_str()),
                        "driver": m.driver,
                        "range": m.range,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "drivers": p.drivers.iter().map(|d| driver_json(d, index)).collect::<Vec<_>>(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_helpers() {
        assert_eq!(
            quarter_end_date(Quarter::new(2026, 2)),
            NaiveDate::from_ymd_opt(2026, 6, 30)
        );
        assert_eq!(
            last_completed_quarter(NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()),
            Quarter::new(2026, 3)
        );
        assert_eq!(
            last_completed_quarter(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()),
            Quarter::new(2025, 4)
        );
    }

    #[test]
    fn yoy_is_points_for_percent_metrics_and_relative_otherwise() {
        assert_eq!(yoy_json("%", Some(12.0), Some(10.0), false)["value"], 2.0);
        let rel = yoy_json("ccy", Some(120.0), Some(100.0), false)["value"]
            .as_f64()
            .unwrap();
        assert!((rel - 20.0).abs() < 1e-9);
        assert!(yoy_json("ccy", Some(1.0), Some(0.0), false).is_null());
        assert!(yoy_json("ccy", None, Some(1.0), false).is_null());
    }

    fn pct(v: Value) -> f64 {
        assert_eq!(v["kind"], "pct");
        v["value"].as_f64().unwrap()
    }

    #[test]
    fn yoy_on_a_negative_base_follows_the_direction_of_the_move() {
        // both negative, worse: -27.163 -> -124.789 is a fall, not a rise of 359 %
        let worse = pct(yoy_json("ccy", Some(-124.789), Some(-27.163), false));
        assert!(worse < 0.0 && (worse + 359.4).abs() < 0.1, "{worse}");
        // both negative, better: -100 -> -60 is +40 %
        assert!((pct(yoy_json("ccy", Some(-60.0), Some(-100.0), false)) - 40.0).abs() < 1e-9);
        // both positive, unchanged rule
        assert!((pct(yoy_json("ccy", Some(150.0), Some(100.0), false)) - 50.0).abs() < 1e-9);
        // to zero from negative / positive
        assert!((pct(yoy_json("ccy", Some(0.0), Some(-50.0), false)) - 100.0).abs() < 1e-9);
        assert!((pct(yoy_json("ccy", Some(0.0), Some(50.0), false)) + 100.0).abs() < 1e-9);
    }

    #[test]
    fn yoy_crossing_zero_has_no_percentage() {
        let up = yoy_json("ccy", Some(5.0), Some(-3.0), false);
        assert_eq!(up, json!({"kind": "sign_change", "from": -3.0, "to": 5.0}));
        let down = yoy_json("ccy", Some(-1.0), Some(8.0), false);
        assert_eq!(
            down,
            json!({"kind": "sign_change", "from": 8.0, "to": -1.0})
        );
    }

    #[test]
    fn yoy_of_a_multiple_is_the_difference_in_times() {
        // BUMI net debt / EBITDA and LEAD capex / depreciation moved by multiples of a few
        // times; a percent change of those read as +313,9 and +162,5.
        let up = yoy_json("x", Some(5.2), Some(2.1), false);
        assert_eq!(up["kind"], "x");
        assert!((up["value"].as_f64().unwrap() - 3.1).abs() < 1e-9);
        let down = yoy_json("x", Some(0.33), Some(0.40), false);
        assert!((down["value"].as_f64().unwrap() + 0.07).abs() < 1e-9);
        // a ratio can cross zero without a sign-change label: the difference is still a number
        assert!(
            (yoy_json("x", Some(-0.5), Some(1.5), false)["value"]
                .as_f64()
                .unwrap()
                + 2.0)
                .abs()
                < 1e-9
        );
        // levels keep the relative change
        assert_eq!(yoy_json("ccy", Some(5.2), Some(2.1), false)["kind"], "pct");
    }

    #[test]
    fn yoy_of_a_ratio_with_a_negative_denominator_is_not_meaningful() {
        assert!(yoy_json("x", Some(2.0), Some(4.0), true).is_null());
        assert_eq!(
            yoy_reason("x", Some(2.0), Some(4.0), true),
            json!("tidak bermakna: penyebut negatif")
        );
        // a positive denominator gives the difference in times, not a percent change
        assert_eq!(
            yoy_json("x", Some(2.0), Some(4.0), false),
            json!({"kind": "x", "value": -2.0})
        );
        assert!(yoy_reason("x", Some(2.0), Some(4.0), false).is_null());
        // no reason when one side is missing, and none for levels
        assert!(yoy_reason("x", None, Some(4.0), true).is_null());
        assert!(yoy_reason("ccy", Some(2.0), Some(4.0), true).is_null());
        // pp metrics are unchanged by the flag
        assert_eq!(yoy_json("%", Some(12.0), Some(10.0), true)["value"], 2.0);
    }
}

#[cfg(test)]
mod pdf_source_tests {
    use super::*;

    fn cite(page: u32) -> Cite {
        Cite {
            page,
            label: format!("p{page}"),
        }
    }

    type Entry<'a> = ((&'a str, i32, u32, InputsOf), PdfSource);

    fn source(pdf: Vec<Entry<'_>>) -> StoreSource {
        let mut map: HashMap<(String, i32, u32, InputsOf), Vec<PdfSource>> = HashMap::new();
        for ((e, fy, q, which), s) in pdf {
            map.entry((e.to_string(), fy, q, which))
                .or_default()
                .push(s);
        }
        StoreSource {
            derived: HashMap::new(),
            pdf: map,
            dims: HashMap::new(),
            units: HashMap::new(),
            kpis: HashMap::new(),
            cadence: HashMap::new(),
        }
    }

    fn pdf(element: &str, page: u32) -> PdfSource {
        PdfSource {
            filing_id: "BUMI_2024_Q1".into(),
            element: element.into(),
            cite: cite(page),
        }
    }

    #[test]
    fn each_view_reads_the_inputs_of_its_own_derived_view() {
        let e = "idx-cor:SalesAndRevenue";
        let src = source(vec![
            ((e, 2024, 2, InputsOf::Standalone), pdf(e, 7)),
            ((e, 2024, 3, InputsOf::Ttm), pdf(e, 8)),
        ]);
        let q2 = Quarter::new(2024, 2);
        let q3 = Quarter::new(2024, 3);
        let pages = |v: Vec<&PdfSource>| v.iter().map(|s| s.cite.page).collect::<Vec<_>>();
        assert_eq!(pages(src.pdf_sources(e, q2, View::Standalone)), [7]);
        assert_eq!(pages(src.pdf_sources(e, q2, View::Instant)), [7]);
        assert!(src.pdf_sources(e, q2, View::Ttm).is_empty());
        assert_eq!(pages(src.pdf_sources(e, q3, View::Ttm)), [8]);
        assert!(src.pdf_sources(e, q3, View::Standalone).is_empty());
    }

    #[test]
    fn the_mean_of_five_instants_cites_every_quarter_it_reads() {
        let e = "idx-cor:Equity";
        let src = source(vec![
            ((e, 2024, 1, InputsOf::Standalone), pdf(e, 6)),
            ((e, 2024, 3, InputsOf::Standalone), pdf(e, 9)),
        ]);
        // Window of 2025Q1: 2025Q1, 2024Q4, Q3, Q2, Q1 (five instants).
        let got = src.pdf_sources(e, Quarter::new(2025, 1), View::Avg4q);
        assert_eq!(got.iter().map(|s| s.cite.page).collect::<Vec<_>>(), [9, 6]);
        // One quarter later the 2024Q1 instant has left the window.
        let later = src.pdf_sources(e, Quarter::new(2025, 2), View::Avg4q);
        assert_eq!(later.iter().map(|s| s.cite.page).collect::<Vec<_>>(), [9]);
    }
}
