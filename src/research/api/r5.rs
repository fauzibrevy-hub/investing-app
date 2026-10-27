//! R5 handlers (research plan §5, R5-A1): the canon ticker page header + overview
//! `GET /{ticker}` and the compare table `GET /compare`. Both only assemble what the R2 to R4
//! modules already compute: the pack payload (coverage, metrics, percentiles), claim counts,
//! events, seen, prices and filings.

use std::collections::{BTreeSet, HashMap};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{canon_ticker, fact_ref_json, store_error, ApiError, ApiState};
use crate::research::claims::source as claim_source;
use crate::research::events;
use crate::research::packs::def::{Layer, Pack};
use crate::research::packs::{payload, CORE_ID};
use crate::research::store::{DerivedRow, Store};
use crate::research::universe::{self, UniverseEntry};

/// Changes listed on the ticker page header: events since the last visit, or this many days back
/// when the ticker was never seen.
const CHANGES_DAYS: i64 = 90;
const CHANGES_LIMIT: usize = 50;
/// Tickers one compare table holds.
const MAX_COMPARE: usize = 12;

fn role_rank(e: &UniverseEntry) -> u8 {
    match e.role.as_str() {
        "held" => 0,
        "manual" => 1,
        _ => 2,
    }
}

fn age_days(today: NaiveDate, day: &str) -> Option<i64> {
    NaiveDate::parse_from_str(day.get(..10)?, "%Y-%m-%d")
        .ok()
        .map(|d| (today - d).num_days().max(0))
}

/// `GET /api/research/{ticker}`: header and overview of the canon ticker page (design.md §4.2
/// blocks 1 and 2).
pub(super) async fn detail(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let engine = st.engine()?;
    let store = &engine.store;
    let (now, today) = (engine.now(), engine.today());
    let entry = store
        .universe_entry(&ticker)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
    let pack = payload::build(store, &st.pack_set(), &entry, &[], today, &now.to_rfc3339())
        .map_err(store_error)?;

    let filings = store.filings(Some(&ticker)).map_err(store_error)?;
    let latest_filing = filings
        .iter()
        .max_by_key(|f| (f.filing.fy, f.filing.period.quarter()))
        .map(|f| {
            let f = &f.filing;
            json!({
                "id": f.id, "fy": f.fy, "period": f.period.as_str(),
                "available_from": f.available_from.to_string(),
                "age_days": (today - f.available_from).num_days().max(0),
                "audited": f.audited, "taxonomy": f.taxonomy,
            })
        });
    let latest_close = store
        .last_closes(&ticker, 1)
        .map_err(store_error)?
        .pop()
        .map(|(date, close)| {
            json!({"date": date, "close": close, "age_days": age_days(today, &date)})
        });

    let seen_at = store.seen(&ticker).map_err(store_error)?.map(|s| s.seen_at);
    let [holding, broken, not_testable, expired] =
        claim_source::status_counts(&store.claims(Some(&ticker)).map_err(store_error)?);

    let since = match seen_at.as_deref().map(DateTime::parse_from_rfc3339) {
        Some(Ok(t)) => t.with_timezone(&Utc),
        Some(Err(e)) => {
            tracing::warn!("research api: seen time of {ticker} is not RFC 3339: {e}");
            now - Duration::days(CHANGES_DAYS)
        }
        None => now - Duration::days(CHANGES_DAYS),
    };
    let mut changes: Vec<_> = store
        .events_list(false)
        .map_err(store_error)?
        .into_iter()
        .filter(|e| e.ticker.as_deref() == Some(ticker.as_str()))
        .collect();
    // Newest first by instant, not by the stored text (offsets may differ); an unparseable
    // time is listed last rather than hidden.
    let parsed = |at: &str| {
        let t = DateTime::parse_from_rfc3339(at)
            .ok()
            .map(|t| t.with_timezone(&Utc));
        if t.is_none() {
            tracing::warn!("research api: event time {at:?} is not RFC 3339");
        }
        t
    };
    changes.sort_by_key(|a| std::cmp::Reverse(parsed(&a.at)));
    let changes: Vec<Value> = changes
        .into_iter()
        .filter(|e| parsed(&e.at).is_none_or(|t| t >= since))
        .take(CHANGES_LIMIT)
        .map(|e| {
            let item = events::item_json(&e, today);
            json!({
                "id": e.id, "kind": e.kind, "at": e.at,
                "text": item["text"], "section": item["section"],
                "dismissed": e.dismissed_at.is_some(),
            })
        })
        .collect();

    Ok(Json(json!({
        "ticker": ticker,
        "name": engine.entity_name(&ticker),
        "as_of": now.to_rfc3339(),
        "role": entry.role.as_str(),
        "peer_of": entry.peer_of,
        "avg_price": store.avg_price(&ticker).map_err(store_error)?,
        "pack": {
            "id": pack["pack"]["id"], "label": pack["pack"]["label"],
            "version": pack["pack"]["version"], "source": pack["pack"]["source"],
            "action": pack["pack"]["action"],
        },
        "business_note": entry.business_note,
        "latest_filing": latest_filing,
        "latest_close": latest_close,
        "seen_at": seen_at,
        "claims": {
            "holding": holding, "broken": broken, "not_testable": not_testable,
            "expired": expired,
        },
        "coverage": pack["coverage"],
        "changes": changes,
    })))
}

#[derive(Deserialize)]
pub(super) struct CompareQuery {
    pack: Option<String>,
    ticker: Option<String>,
    tickers: Option<String>,
}

fn bad_tickers(raw: &str, why: &str) -> ApiError {
    ApiError::invalid(&["query", "tickers"], "value_error", why, json!(raw))
}

/// `tickers=A,B,C`: at most twelve, each four uppercase letters, duplicates dropped.
fn parse_tickers(raw: &str) -> Result<Vec<String>, ApiError> {
    let mut list: Vec<String> = Vec::new();
    for t in raw.split(',').map(str::trim) {
        if t.len() != 4 || !t.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(bad_tickers(raw, "Each ticker is four uppercase letters"));
        }
        if !list.iter().any(|x| x == t) {
            list.push(t.to_string());
        }
    }
    if list.len() > MAX_COMPARE {
        return Err(bad_tickers(raw, "At most 12 tickers"));
    }
    Ok(list)
}

/// What a KPI metric shows at the ticker's latest quarter: `(state, value, period)`.
fn kpi_cell(metric: &Value) -> (&'static str, Option<f64>, Option<String>) {
    let cells = metric["cells"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    // The latest quarter the metric applies to (an annual KPI is not applicable at Q1 to Q3).
    let last = cells.iter().rev().find(|c| c["state"] != "not_applicable");
    if let Some(c) = last.filter(|c| c["unknown"] == true) {
        return ("unknown", None, c["label"].as_str().map(str::to_string));
    }
    match metric["latest"]["value"].as_f64() {
        Some(v) => (
            "value",
            Some(v),
            metric["latest"]["label"].as_str().map(str::to_string),
        ),
        None => ("gap", None, None),
    }
}

/// The fact behind a value that is one plain element of one filing: the single input of the
/// evaluation, looked up in the derived rows. Ratios and sums of several inputs, and
/// dimensioned or referenced inputs, and values derived from several facts (year-to-date
/// differences, TTM sums), have no single fact, so they get none.
fn fact_target(metric: &Value, derived: &HashMap<(String, i32, u32), DerivedRow>) -> Value {
    let cell = metric["cells"].as_array().and_then(|c| {
        let latest = metric["latest"]["label"].as_str()?;
        c.iter().find(|c| c["label"] == latest)
    });
    let Some(inputs) = cell.and_then(|c| c["inputs"].as_array()) else {
        return Value::Null;
    };
    let [one] = inputs.as_slice() else {
        return Value::Null;
    };
    let source = one["source"].as_str().unwrap_or("");
    if source.contains('[') || source.starts_with("kpi:") || source.starts_with("metric:") {
        return Value::Null;
    }
    let (Some(label), Some(view)) = (one["q"].as_str(), one["view"].as_str()) else {
        return Value::Null;
    };
    let Some((fy, q)) = label
        .split_once('Q')
        .and_then(|(fy, q)| Some((fy.parse::<i32>().ok()?, q.parse::<u32>().ok()?)))
    else {
        return Value::Null;
    };
    let Some(row) = derived.get(&(source.to_string(), fy, q)) else {
        return Value::Null;
    };
    let inputs = match view {
        "ttm" => &row.inputs.ttm,
        "instant" | "standalone" => &row.inputs.standalone,
        _ => return Value::Null,
    };
    // A standalone Q2 to Q4 value or a TTM sum rests on several facts: none is "the" source.
    match inputs.as_slice() {
        [one] => fact_ref_json(&one.fact),
        _ => Value::Null,
    }
}

/// `(group, group label, pack, dimension index, metric index)` of every table row: the core
/// pack's metrics under `core`, then the chosen pack's metrics grouped by dimension. Driver
/// metrics hold no values and are left out.
type RowDef<'a> = (String, String, &'a Pack, usize, usize);

fn row_defs<'a>(core: &'a Pack, pack: Option<&'a Pack>) -> Vec<RowDef<'a>> {
    let mut rows = Vec::new();
    for (p, is_core) in std::iter::once((core, true)).chain(pack.map(|p| (p, false))) {
        for (di, d) in p.dimensions.iter().enumerate() {
            for (mi, m) in d.metrics.iter().enumerate() {
                if m.layer == Layer::Driver {
                    continue;
                }
                let (group, label) = if is_core {
                    ("core".to_string(), p.label.clone())
                } else {
                    (d.id.clone(), d.label.clone())
                };
                rows.push((group, label, p, di, mi));
            }
        }
    }
    rows
}

/// One ticker's payload metrics by `(pack, metric)` and its derived rows by `(element, fy, q)`.
struct TickerData {
    ticker: String,
    metrics: HashMap<(String, String), Value>,
    derived: HashMap<(String, i32, u32), DerivedRow>,
}

fn gap_cell(ticker: &str, reason: Option<&str>) -> Value {
    json!({
        "ticker": ticker, "value": null, "period": null, "percentile": null,
        "state": "gap", "fact": null, "reason": reason, "flags": [],
    })
}

fn cell_json(layer: Layer, d: &TickerData, pack_id: &str, metric_id: &str) -> Value {
    // The ticker's pack in effect does not define this metric.
    let Some(pm) = d.metrics.get(&(pack_id.to_string(), metric_id.to_string())) else {
        return gap_cell(&d.ticker, Some("not_defined"));
    };
    let (state, value, period) = if layer == Layer::Kpi {
        kpi_cell(pm)
    } else {
        match pm["latest"]["value"].as_f64() {
            Some(v) => (
                "value",
                Some(v),
                pm["latest"]["label"].as_str().map(str::to_string),
            ),
            None => ("gap", None, None),
        }
    };
    let fact = if layer == Layer::Xbrl && value.is_some() {
        fact_target(pm, &d.derived)
    } else {
        Value::Null
    };
    let last = pm["cells"].as_array().and_then(|c| c.last());
    let latest_cell = last.filter(|c| c["label"] == pm["latest"]["label"]);
    // Why there is no value (the evaluator's words), and what the pack section flags.
    let reason = if state == "gap" {
        last.and_then(|c| c["gap"]["text"].as_str())
            .or(Some("not_entered"))
    } else {
        None
    };
    json!({
        "ticker": d.ticker, "value": value, "period": period,
        "percentile": if value.is_some() { pm["percentile"]["value"].clone() } else { Value::Null },
        "state": state, "fact": fact, "reason": reason,
        "flags": latest_cell.map_or(json!([]), |c| c["flags"].clone()),
    })
}

/// `GET /api/research/compare?pack=&ticker=&tickers=` (design.md §4.3). Values and
/// percentiles are the pack payload's latest cell of each ticker, so the table and the
/// ticker page's pack section never disagree.
pub(super) async fn compare(
    State(st): State<ApiState>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<Value>, ApiError> {
    let engine = st.engine()?;
    let store: &Store = &engine.store;
    let packs = st.pack_set();
    let entries = store.universe().map_err(store_error)?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let choice_of = |e: &UniverseEntry| {
        universe::pack_choice(&packs, e, codes.get(&e.ticker).map(String::as_str)).id
    };

    let given = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let subject = given(&q.ticker)
        .map(|t| canon_ticker(&st, &t))
        .transpose()?;
    let pack_id = match (given(&q.pack), &subject) {
        (Some(p), _) => Some(p),
        (None, Some(t)) => entries.iter().find(|e| &e.ticker == t).and_then(choice_of),
        (None, None) => {
            return Err(ApiError::invalid(
                &["query", "pack"],
                "missing",
                "Give pack or ticker",
                Value::Null,
            ))
        }
    };
    let pack: Option<&Pack> =
        match &pack_id {
            Some(id) => Some(packs.get(id).ok_or_else(|| {
                ApiError::new(StatusCode::NOT_FOUND, format!("Unknown pack {id}"))
            })?),
            None => None,
        };
    // `core` is a pack id of its own (every ticker applies it): its rows are the core rows.
    let industry = pack.filter(|p| p.id != CORE_ID);

    let mut truncated = false;
    let list: Vec<String> = match given(&q.tickers) {
        Some(raw) => {
            let mut list = parse_tickers(&raw)?;
            // The page's own ticker leads the table even when the list omits it.
            if let Some(t) = subject.as_ref().filter(|t| !list.contains(t)) {
                list.insert(0, t.clone());
            }
            if list.len() > MAX_COMPARE {
                return Err(bad_tickers(&raw, "At most 12 tickers"));
            }
            list
        }
        None => {
            let mut members: Vec<&UniverseEntry> = entries
                .iter()
                .filter(|e| match pack {
                    Some(p) if p.id == CORE_ID => true,
                    Some(p) => choice_of(e).as_deref() == Some(p.id.as_str()),
                    None => subject.as_deref() == Some(e.ticker.as_str()),
                })
                .collect();
            members.sort_by(|a, b| (role_rank(a), &a.ticker).cmp(&(role_rank(b), &b.ticker)));
            let mut list: Vec<String> = subject.iter().cloned().collect();
            for e in &members {
                if !list.contains(&e.ticker) {
                    list.push(e.ticker.clone());
                }
            }
            let peers: BTreeSet<&str> = entries
                .iter()
                .filter(|e| {
                    e.peer_of
                        .as_deref()
                        .is_some_and(|p| members.iter().any(|m| m.ticker == p))
                })
                .map(|e| e.ticker.as_str())
                .collect();
            for p in peers {
                if !list.iter().any(|t| t == p) {
                    list.push(p.to_string());
                }
            }
            truncated = list.len() > MAX_COMPARE;
            list.truncate(MAX_COMPARE);
            list
        }
    };

    let (now, today) = (engine.now().to_rfc3339(), engine.today());
    let mut tickers = Vec::new();
    let mut data: Vec<TickerData> = Vec::new();
    for t in &list {
        let entry = entries.iter().find(|e| &e.ticker == t).ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("{t} is not in the universe"))
        })?;
        let built = payload::build(store, &packs, entry, &[], today, &now).map_err(store_error)?;
        let mut metrics = HashMap::new();
        for d in built["dimensions"].as_array().into_iter().flatten() {
            for m in d["metrics"].as_array().into_iter().flatten() {
                if let (Some(p), Some(id)) = (d["pack"].as_str(), m["id"].as_str()) {
                    metrics.insert((p.to_string(), id.to_string()), m.clone());
                }
            }
        }
        let derived = store
            .derived(t, None)
            .map_err(store_error)?
            .into_iter()
            .map(|r| ((r.element.clone(), r.fy, r.q), r))
            .collect();
        // Reporting currency of the newest filing (as the financials route says it), so the
        // table can scale money per column; null until a filing is stored.
        let currency = store
            .filings(Some(t))
            .map_err(store_error)?
            .first()
            .map(|f| f.filing.currency.clone());
        tickers.push(json!({
            "ticker": t, "name": engine.entity_name(t), "role": entry.role.as_str(),
            "currency": currency,
        }));
        data.push(TickerData {
            ticker: t.clone(),
            metrics,
            derived,
        });
    }

    let rows: Vec<Value> = row_defs(packs.core(), industry)
        .into_iter()
        .map(|(group, group_label, p, di, mi)| {
            let m = &p.dimensions[di].metrics[mi];
            json!({
                "group": group, "group_label": group_label,
                "metric": {
                    "id": m.id, "label": m.label, "unit": m.unit, "layer": m.layer.as_str(),
                    "direction": m.direction.as_str(),
                    "period": m.period.map(|p| p.as_str()),
                },
                "cells": data.iter().map(|d| cell_json(m.layer, d, &p.id, &m.id)).collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(Json(json!({
        "as_of": now,
        "pack": pack.map(|p| json!({"id": p.id, "label": p.label, "version": p.version})),
        "tickers": tickers,
        // The default list holds at most 12 tickers; this says when members were left out.
        "truncated": truncated,
        "rows": rows,
    })))
}
