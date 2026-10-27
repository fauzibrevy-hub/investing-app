//! The store-facing half of claims: which metrics a claim can bind to, the observations of
//! a metric for one ticker (with the date each became known), and the re-evaluation pass
//! that runs after ingest (A4.1: a claim is re-evaluated within one hour of data arriving;
//! the scheduler ticks every ten minutes and the KPI and refresh handlers call it directly).

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Value};

use super::{evaluate, Comparator, Evaluation, Observation, Spec, Status, Window};
use crate::research::kpi::KpiValue;
use crate::research::packs::eval::{self, Ctx, Quarter};
use crate::research::packs::payload::{quarter_end_date, StoreSource};
use crate::research::packs::{Layer, Pack, PackSet, Period};
use crate::research::prices::payload::split_ratios;
use crate::research::store::{ClaimEvalRow, ClaimRow, Store, StoreError};
use crate::research::universe::{self, UniverseEntry};

/// Price-derived metrics a claim can bind to (design.md §3.4 "core, pack, or price").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceMetric {
    Close,
    Ma200,
    /// `(close ÷ MA200 − 1) × 100`.
    VsMa200Pct,
}

impl PriceMetric {
    pub const ALL: [PriceMetric; 3] = [Self::Close, Self::Ma200, Self::VsMa200Pct];

    pub fn id(self) -> &'static str {
        match self {
            Self::Close => "price.close",
            Self::Ma200 => "price.ma200",
            Self::VsMa200Pct => "price.vs_ma200_pct",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Close => "Harga penutupan",
            Self::Ma200 => "MA200",
            Self::VsMa200Pct => "Harga terhadap MA200",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::VsMa200Pct => "%",
            _ => "IDR",
        }
    }

    fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricKind {
    Pack { pack_id: String, metric_id: String },
    Price(PriceMetric),
}

/// A claim's bound metric, resolved against the packs in effect for the ticker.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    /// The string stored in `claims.metric` (`<pack>.<metric>` or `price.<name>`).
    pub id: String,
    pub label: String,
    pub unit: String,
    pub kind: MetricKind,
}

impl Metric {
    /// Quarterly metrics have one value per fiscal quarter; price metrics one per day.
    pub fn quarterly(&self) -> bool {
        matches!(self.kind, MetricKind::Pack { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricError {
    /// Not `price.*`, and no such pack or metric.
    Unknown(String),
    /// The pack is neither core nor the ticker's pack in effect.
    NotApplicable(String),
    /// A driver metric: it has a series, not a quarterly value.
    NoValues(String),
}

impl MetricError {
    pub fn message(&self) -> String {
        match self {
            Self::Unknown(m) => format!("{m} is not a metric a claim can bind to"),
            Self::NotApplicable(m) => format!("{m} is not in the packs in effect for this ticker"),
            Self::NoValues(m) => format!("{m} is a driver series and has no quarterly value"),
        }
    }
}

/// Core plus the pack in effect for `entry` (override, else the filing's industry code).
pub fn applied_packs<'a>(
    store: &Store,
    packs: &'a PackSet,
    entry: &UniverseEntry,
) -> Result<Vec<&'a Pack>, StoreError> {
    let codes = store.latest_industry_codes()?;
    let choice = universe::pack_choice(packs, entry, codes.get(&entry.ticker).map(String::as_str));
    let mut applied = vec![packs.core()];
    applied.extend(choice.id.as_deref().and_then(|id| packs.get(id)));
    Ok(applied)
}

pub fn resolve(applied: &[&Pack], metric: &str) -> Result<Metric, MetricError> {
    if metric.starts_with("price.") {
        let p = PriceMetric::parse(metric).ok_or_else(|| MetricError::Unknown(metric.into()))?;
        return Ok(Metric {
            id: metric.into(),
            label: p.label().into(),
            unit: p.unit().into(),
            kind: MetricKind::Price(p),
        });
    }
    let (pack_id, metric_id) = metric
        .split_once('.')
        .ok_or_else(|| MetricError::Unknown(metric.into()))?;
    let pack = applied
        .iter()
        .find(|p| p.id == pack_id)
        .ok_or_else(|| MetricError::NotApplicable(metric.into()))?;
    let m = pack
        .metric(metric_id)
        .ok_or_else(|| MetricError::Unknown(metric.into()))?;
    if m.layer == Layer::Driver {
        return Err(MetricError::NoValues(metric.into()));
    }
    Ok(Metric {
        id: metric.into(),
        label: m.label.clone(),
        unit: m.unit.clone(),
        kind: MetricKind::Pack {
            pack_id: pack_id.into(),
            metric_id: metric_id.into(),
        },
    })
}

/// Every metric the ticker's packs offer to a claim, for the editor's picker.
pub fn catalogue(applied: &[&Pack]) -> Vec<Value> {
    let mut out: Vec<Value> = applied
        .iter()
        .flat_map(|p| {
            p.metrics()
                .filter(|m| m.layer != Layer::Driver)
                .map(|m| {
                    json!({
                        "id": format!("{}.{}", p.id, m.id), "label": m.label, "unit": m.unit,
                        "layer": m.layer.as_str(), "pack": p.id, "quarterly": true,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    out.extend(PriceMetric::ALL.iter().map(|p| {
        json!({
            "id": p.id(), "label": p.label(), "unit": p.unit(), "layer": "price",
            "pack": null, "quarterly": false,
        })
    }));
    out
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// Everything one ticker's claims read, loaded once per pass.
pub struct TickerData<'a> {
    pub ticker: String,
    pub applied: Vec<&'a Pack>,
    source: StoreSource,
    /// Quarter of each filing and the date it became available.
    filings: BTreeMap<Quarter, NaiveDate>,
    kpis: Vec<KpiValue>,
    /// Ascending daily closes (split-adjusted by the price source).
    closes: Vec<(NaiveDate, f64)>,
    /// `(date, ratio)` of the ticker's splits.
    splits: Vec<(NaiveDate, f64)>,
}

impl<'a> TickerData<'a> {
    pub fn load(
        store: &Store,
        packs: &'a PackSet,
        entry: &UniverseEntry,
    ) -> Result<Self, StoreError> {
        let applied = applied_packs(store, packs, entry)?;
        let source = StoreSource::load(store, &entry.ticker, &applied)?;
        let mut filings = BTreeMap::new();
        for f in store.filings(Some(&entry.ticker))? {
            let q = Quarter::new(f.filing.fy, f.filing.period.quarter());
            let slot = filings.entry(q).or_insert(f.filing.available_from);
            *slot = (*slot).min(f.filing.available_from);
        }
        let mut closes: Vec<(NaiveDate, f64)> = store
            .prices(&entry.ticker)?
            .into_iter()
            .filter_map(|b| parse_date(&b.date).map(|d| (d, b.close)))
            .collect();
        closes.sort_by_key(|(d, _)| *d);
        Ok(Self {
            ticker: entry.ticker.clone(),
            applied,
            source,
            filings,
            kpis: store.kpis(&entry.ticker)?,
            closes,
            splits: split_ratios(store, &entry.ticker)?,
        })
    }

    /// Observations of `metric`. Quarterly values come from the filing's own availability
    /// date (KPIs: the document date), so an evaluation never sees a quarter early. Prices
    /// give the newest close known on `as_of` and the one at `due`.
    pub fn observations(
        &self,
        packs: &PackSet,
        metric: &Metric,
        as_of: NaiveDate,
        due: NaiveDate,
    ) -> Vec<Observation> {
        match &metric.kind {
            MetricKind::Price(p) => self.price_observations(*p, as_of, due),
            MetricKind::Pack { pack_id, metric_id } => {
                let Some((pack, m)) = packs
                    .get(pack_id)
                    .and_then(|p| p.metric(metric_id).map(|m| (p, m)))
                else {
                    return Vec::new();
                };
                match m.layer {
                    Layer::Xbrl => {
                        let Some(formula) = &m.formula else {
                            return Vec::new();
                        };
                        let period = m.period.unwrap_or(Period::Standalone);
                        let ctx = Ctx {
                            source: &self.source,
                            packs,
                        };
                        self.filings
                            .iter()
                            .filter(|(_, avail)| **avail <= as_of)
                            .filter_map(|(q, avail)| {
                                let o = eval::evaluate(&ctx, pack, formula, period, *q);
                                let value = o.value?;
                                Some(Observation {
                                    quarter: Some(*q),
                                    date: quarter_end_date(*q)?,
                                    available_from: *avail,
                                    value,
                                    inputs: json!({"kind": "xbrl", "operands": o.inputs}),
                                })
                            })
                            .collect()
                    }
                    Layer::Kpi => self
                        .kpis
                        .iter()
                        .filter(|k| k.metric == *metric_id && !k.unknown)
                        .filter_map(|k| {
                            let q = Quarter::new(k.fy, k.q);
                            Some(Observation {
                                quarter: Some(q),
                                date: quarter_end_date(q)?,
                                available_from: parse_date(&k.document_date)
                                    .or_else(|| parse_date(&k.entered_at))?,
                                value: k.value?,
                                inputs: json!({
                                    "kind": "kpi", "document": k.document,
                                    "document_date": k.document_date,
                                }),
                            })
                        })
                        .collect(),
                    Layer::Driver => Vec::new(),
                }
            }
        }
    }

    fn price_observations(
        &self,
        p: PriceMetric,
        as_of: NaiveDate,
        due: NaiveDate,
    ) -> Vec<Observation> {
        let mut dates = vec![as_of];
        if due < as_of {
            dates.push(due);
        }
        let mut out = Vec::new();
        for on in dates {
            let i = self.closes.partition_point(|(d, _)| *d <= on);
            if i == 0 {
                continue;
            }
            let (date, close) = self.closes[i - 1];
            let ma200 = (i >= 200)
                .then(|| self.closes[i - 200..i].iter().map(|(_, c)| c).sum::<f64>() / 200.0);
            let value = match p {
                PriceMetric::Close => Some(close),
                PriceMetric::Ma200 => ma200,
                PriceMetric::VsMa200Pct => ma200
                    .filter(|m| *m > 0.0)
                    .map(|m| (close / m - 1.0) * 100.0),
            };
            if let Some(value) = value {
                out.push(Observation {
                    quarter: None,
                    date,
                    available_from: date,
                    value,
                    inputs: json!({"kind": "price", "close": close, "date": date.to_string()}),
                });
            }
        }
        out
    }
}

/// The claim row as a spec; `None` when the stored text no longer parses.
pub fn spec_of(c: &ClaimRow) -> Option<Spec> {
    Some(Spec {
        comparator: Comparator::parse(&c.comparator)?,
        threshold: c.threshold,
        threshold_hi: c.threshold_hi,
        window: Window::parse(&c.window)?,
        since: crate::research::journal::wib_day(&c.created_at)?,
        due: parse_date(&c.due)?,
    })
}

/// Moves a price threshold written before a split onto the stored closes' basis. Ratio
/// metrics (`price.vs_ma200_pct`) do not depend on the basis.
fn scaled_for_splits(data: &TickerData<'_>, metric: &Metric, claim: &ClaimRow, spec: &mut Spec) {
    let MetricKind::Price(PriceMetric::Close | PriceMetric::Ma200) = metric.kind else {
        return;
    };
    let Some(written) = crate::research::journal::wib_day(&claim.created_at) else {
        return;
    };
    let basis = crate::research::valuation::split_factor(&data.splits, written);
    if basis > 0.0 && basis != 1.0 {
        spec.threshold /= basis;
        spec.threshold_hi = spec.threshold_hi.map(|h| h / basis);
    }
}

/// Evaluates one stored claim over `data` as of `as_of`.
pub fn evaluate_claim(
    data: &TickerData<'_>,
    packs: &PackSet,
    claim: &ClaimRow,
    as_of: NaiveDate,
) -> Result<(Evaluation, Spec), MetricError> {
    let mut spec = spec_of(claim).ok_or_else(|| MetricError::Unknown(claim.metric.clone()))?;
    let metric = resolve(&data.applied, &claim.metric)?;
    scaled_for_splits(data, &metric, claim, &mut spec);
    let obs = data.observations(packs, &metric, as_of, spec.due);
    Ok((evaluate(&spec, &obs, as_of), spec))
}

/// A claim whose status moved in a re-evaluation pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusChange {
    pub claim_id: String,
    pub ticker: String,
    pub statement: String,
    pub from: String,
    pub to: String,
    pub value: Option<f64>,
}

/// The observations of a stored evaluation, for comparison.
fn stored_observations(inputs: &str) -> Value {
    serde_json::from_str::<Value>(inputs)
        .ok()
        .and_then(|v| v.get("observations").cloned())
        .unwrap_or(Value::Null)
}

/// Re-evaluates every open claim (of `tickers`, or all) as of `today`. A new evaluation row
/// is stored when the status, the value or the observations differ from the last one, so
/// the history shows what the data did and not the passing of days; the returned changes
/// are the claims whose status moved.
pub fn reevaluate(
    store: &Store,
    packs: &PackSet,
    now: DateTime<Utc>,
    today: NaiveDate,
    tickers: Option<&[String]>,
) -> Result<Vec<StatusChange>, StoreError> {
    let mut by_ticker: BTreeMap<String, Vec<ClaimRow>> = BTreeMap::new();
    for c in store.claims(None)? {
        if c.closed_at.is_none() && tickers.is_none_or(|t| t.contains(&c.ticker)) {
            by_ticker.entry(c.ticker.clone()).or_default().push(c);
        }
    }
    let mut changes = Vec::new();
    let mut entries: HashMap<String, UniverseEntry> = HashMap::new();
    for (ticker, claims) in by_ticker {
        // A claim on a ticker that left the universe keeps its last status.
        let Some(entry) = store.universe_entry(&ticker)? else {
            continue;
        };
        entries.insert(ticker.clone(), entry.clone());
        let data = TickerData::load(store, packs, &entry)?;
        for claim in claims {
            let Some(stored_spec) = spec_of(&claim) else {
                tracing::warn!(
                    "research claim {}: stored condition no longer parses",
                    claim.id
                );
                continue;
            };
            // A metric the packs no longer offer (override changed, pack reloaded without
            // it) makes the claim not testable, with the reason on the evaluation; it must
            // not keep a status it can no longer back.
            let (eval, spec) = match evaluate_claim(&data, packs, &claim, today) {
                Ok(ok) => ok,
                Err(e) => {
                    tracing::warn!("research claim {}: {}", claim.id, e.message());
                    (
                        Evaluation {
                            status: Status::NotTestable,
                            value: None,
                            used: Vec::new(),
                            reason: Some("metrik tidak tersedia di paket saat ini"),
                        },
                        stored_spec,
                    )
                }
            };
            let inputs = eval.inputs_json(spec.window, today);
            let last = store.last_eval(&claim.id)?;
            let unchanged = last.as_ref().is_some_and(|l| {
                l.status == eval.status.as_str()
                    && l.value == eval.value
                    && stored_observations(&l.inputs) == inputs["observations"]
            });
            if unchanged {
                continue;
            }
            store.record_eval(&ClaimEvalRow {
                claim_id: claim.id.clone(),
                evaluated_at: now.to_rfc3339(),
                status: eval.status.as_str().into(),
                value: eval.value,
                inputs: inputs.to_string(),
            })?;
            if claim.status != eval.status.as_str() {
                changes.push(StatusChange {
                    claim_id: claim.id,
                    ticker: claim.ticker,
                    statement: claim.statement,
                    from: claim.status,
                    to: eval.status.as_str().into(),
                    value: eval.value,
                });
            }
        }
    }
    Ok(changes)
}

/// Counts per status over `claims` (closed claims excluded), for the coverage board.
pub fn status_counts(claims: &[ClaimRow]) -> [usize; 4] {
    let mut n = [0; 4];
    for c in claims.iter().filter(|c| c.closed_at.is_none()) {
        match Status::parse(&c.status) {
            Some(Status::Holding) => n[0] += 1,
            Some(Status::Broken) => n[1] += 1,
            Some(Status::NotTestable) => n[2] += 1,
            Some(Status::Expired) => n[3] += 1,
            None => {}
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_metrics_have_stable_ids_and_units() {
        for p in PriceMetric::ALL {
            assert_eq!(PriceMetric::parse(p.id()), Some(p));
        }
        assert_eq!(PriceMetric::Close.unit(), "IDR");
        assert_eq!(PriceMetric::VsMa200Pct.unit(), "%");
        assert_eq!(PriceMetric::parse("price.volume"), None);
    }

    #[test]
    fn unknown_metric_text_is_refused() {
        assert_eq!(
            resolve(&[], "price.volume"),
            Err(MetricError::Unknown("price.volume".into()))
        );
        assert_eq!(
            resolve(&[], "nothing"),
            Err(MetricError::Unknown("nothing".into()))
        );
        assert!(resolve(&[], "price.close").is_ok());
        assert_eq!(
            resolve(&[], "core.net_income"),
            Err(MetricError::NotApplicable("core.net_income".into()))
        );
    }
}
