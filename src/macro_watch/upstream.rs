//! Upstream fetchers ported from MC `backend/services/macro_ingestion.py`
//! (`fetch_fred_series_upstream`, `fetch_market_series_upstream`,
//! `_parse_official_observation`, `_extract_yahoo_history`).
//!
//! Failures surface in observation `quality_note`s verbatim, so errors are
//! worded like the Python exceptions MC reports (httpx `HTTPStatusError`,
//! `ConnectError`, `KeyError`, ...). Each call returns MC's result dict.

use chrono::{DateTime, Datelike, Utc};
use serde_json::{json, Map, Value};

use crate::http::PyResult;
pub use crate::http::{Http, PyErr, Response};
use crate::py::{isoformat, py_float, py_round, py_str, splitlines, truthy};

/// Upstream URLs (overridable for tests). Error messages always quote the
/// production URL shape, like MC.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub fred: String,
    pub yahoo: String,
    pub er_api: String,
    pub bi_inflation: String,
    pub nbs_pmi: String,
    /// Where an isolated FRED request goes (MC: an unreachable local port).
    pub fred_isolated: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            fred: "https://fred.stlouisfed.org/graph/fredgraph.csv".into(),
            yahoo: "https://query1.finance.yahoo.com/v8/finance/chart".into(),
            er_api: "https://open.er-api.com/v6/latest/USD".into(),
            bi_inflation: "https://www.bi.go.id/biweb/api/Inflasi/GetInflasi".into(),
            nbs_pmi: "https://www.stats.gov.cn/sj/zxfb/pmi".into(),
            fred_isolated: "http://127.0.0.1:54321/isolated_fred_unreachable".into(),
        }
    }
}

// ---- Python-like access to decoded JSON (errors worded like CPython) ----

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// `v[key]`.
pub fn key<'a>(v: &'a Value, k: &str) -> PyResult<&'a Value> {
    match v {
        Value::Object(o) => o
            .get(k)
            .ok_or_else(|| PyErr::new("KeyError", format!("'{k}'"))),
        Value::Array(_) => Err(PyErr::new(
            "TypeError",
            "list indices must be integers or slices, not str",
        )),
        Value::String(_) => Err(PyErr::new(
            "TypeError",
            "string indices must be integers, not 'str'",
        )),
        other => Err(PyErr::new(
            "TypeError",
            format!("'{}' object is not subscriptable", type_name(other)),
        )),
    }
}

/// `v[i]`.
pub fn index(v: &Value, i: usize) -> PyResult<&Value> {
    match v {
        Value::Array(a) => a
            .get(i)
            .ok_or_else(|| PyErr::new("IndexError", "list index out of range")),
        Value::Object(_) => Err(PyErr::new("KeyError", i.to_string())),
        other => Err(PyErr::new(
            "TypeError",
            format!("'{}' object is not subscriptable", type_name(other)),
        )),
    }
}

/// `float(v)`.
pub fn to_float(v: &Value) -> PyResult<f64> {
    match v {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "nan" | "+nan" | "-nan" => Ok(f64::NAN),
            "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
            "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
            _ => py_float(s).ok_or_else(|| {
                PyErr::new(
                    "ValueError",
                    format!("could not convert string to float: '{s}'"),
                )
            }),
        },
        other => Err(PyErr::new(
            "TypeError",
            format!(
                "float() argument must be a string or a real number, not '{}'",
                type_name(other)
            ),
        )),
    }
}

fn finite(v: f64) -> bool {
    v.is_finite()
}

fn utc_date(ts: f64) -> String {
    DateTime::from_timestamp(ts.floor() as i64, 0)
        .map(|d| d.date_naive().format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn timestamp_of(meta: &Value, now: DateTime<Utc>) -> PyResult<f64> {
    match meta.get("regularMarketTime") {
        None => Ok(now.timestamp() as f64),
        Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(0.0)),
        Some(other) => Err(PyErr::new(
            "TypeError",
            format!(
                "'{}' object cannot be interpreted as an integer",
                type_name(other)
            ),
        )),
    }
}

fn now_iso(now: DateTime<Utc>) -> String {
    isoformat(&now.fixed_offset())
}

fn result_err(e: PyErr) -> Value {
    json!({"status": "error", "error": e.msg, "error_type": e.kind})
}

// ---- FRED ----

pub async fn fetch_fred(
    http: &Http,
    ep: &Endpoints,
    series_id: &str,
    isolate: bool,
    now: DateTime<Utc>,
) -> Value {
    let url = if isolate {
        ep.fred_isolated.clone()
    } else {
        format!("{}?id={series_id}", ep.fred)
    };
    let res = match http.get(&url).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "FRED fetch failed for {series_id} (isolate={isolate}): {}",
                e.msg
            );
            return result_err(e);
        }
    };
    match parse_fred(&res.text, series_id, now) {
        Ok(v) => v,
        Err(e) => result_err(e),
    }
}

fn csv_rows(text: &str) -> Vec<Vec<&str>> {
    splitlines(text.trim())
        .into_iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().split(',').collect())
        .collect()
}

pub fn parse_fred(text: &str, series_id: &str, now: DateTime<Utc>) -> PyResult<Value> {
    let rows = csv_rows(text);
    if series_id == "CPILFESL" {
        let mut valid: Vec<(String, f64)> = Vec::new();
        for r in &rows {
            if r.len() == 2 && !matches!(r[1], "." | "" | "CPILFESL" | "VALUE") {
                if let Some(v) = py_float(r[1]).filter(|v| finite(*v)) {
                    match valid.iter_mut().find(|(d, _)| d == r[0]) {
                        Some(e) => e.1 = v,
                        None => valid.push((r[0].to_string(), v)),
                    }
                }
            }
        }
        if valid.is_empty() {
            return Err(PyErr::new("ValueError", "No valid CPILFESL observations"));
        }
        valid.sort_by(|a, b| a.0.cmp(&b.0));
        let get = |d: &str| valid.iter().find(|(k, _)| k == d).map(|(_, v)| *v);
        let prior = |d: &str| -> PyResult<String> {
            let y: i64 = d.get(..4).and_then(|y| y.parse().ok()).ok_or_else(|| {
                PyErr::new(
                    "ValueError",
                    format!(
                        "invalid literal for int() with base 10: '{}'",
                        d.get(..4).unwrap_or(d)
                    ),
                )
            })?;
            Ok(format!("{}{}", y - 1, d.get(4..).unwrap_or("")))
        };
        let (latest_d, latest_idx) = valid.last().cloned().expect("non-empty");
        let prev_d = prior(&latest_d)?;
        let Some(prev_idx) = get(&prev_d).filter(|p| *p != 0.0) else {
            let note = format!(
                "CPILFESL YoY unavailable: missing valid prior-year observation for {prev_d}; numerator {latest_idx:.3} ({latest_d}) is present"
            );
            return Ok(json!({
                "status": "unavailable", "observed_at": latest_d, "value": null, "fetched_at": now_iso(now),
                "quality_note": note, "error": note, "error_type": "MissingPriorYearObservation", "history": [], "unit": "percent_yoy",
            }));
        };
        let yoy = py_round((latest_idx - prev_idx) / prev_idx * 100.0, 2);
        let mut hist = Vec::new();
        for (d, cur) in valid.iter().skip(valid.len().saturating_sub(12)) {
            if let Some(p) = get(&prior(d)?).filter(|p| *p != 0.0) {
                hist.push(json!({"date": d, "value": py_round((cur - p) / p * 100.0, 2)}));
            }
        }
        return Ok(json!({
            "status": "ok", "observed_at": latest_d, "value": yoy, "release_at": "2026-09-11T08:30:00Z",
            "fetched_at": now_iso(now),
            "quality_note": format!("YoY {yoy:.2}% derived from CPILFESL index: {latest_idx:.3} ({latest_d}) ÷ {prev_idx:.3} ({prev_d})"),
            "history": hist, "unit": "percent_yoy",
        }));
    }

    let mut valid = Vec::new();
    for r in rows.iter().skip(1) {
        if r.len() == 2 && !matches!(r[1], "." | "" | "VALUE") {
            let Some(mut v) = py_float(r[1]).filter(|v| finite(*v)) else {
                continue;
            };
            if series_id == "T10Y2Y" {
                // FRED publishes this series in percentage points; MC converts to bps once here.
                v = py_round(v * 100.0, 1);
            }
            valid.push((r[0].to_string(), v));
        }
    }
    let Some((latest_date, final_val)) = valid.last().cloned() else {
        return Err(PyErr::new(
            "ValueError",
            format!("No valid data points in FRED response for {series_id}"),
        ));
    };
    let (note, unit) = if series_id == "T10Y2Y" {
        let sign = if final_val >= 0.0 { "+" } else { "" };
        (
            format!("Verified from FRED (T10Y2Y ×100: {sign}{final_val:.1} bps)"),
            "bps",
        )
    } else {
        (format!("Verified from FRED ({series_id})"), "percent")
    };
    let history: Vec<Value> = valid
        .iter()
        .skip(valid.len().saturating_sub(15))
        .map(|(d, v)| json!({"date": d, "value": v}))
        .collect();
    Ok(json!({
        "status": "ok", "observed_at": latest_date, "value": final_val, "fetched_at": now_iso(now),
        "quality_note": note, "history": history, "unit": unit,
    }))
}

// ---- Yahoo / open.er-api / official feeds ----

/// `_extract_yahoo_history`: last 15 daily closes; any parse problem → [].
pub fn yahoo_history(j: &Value) -> Vec<Value> {
    let res = j
        .get("chart")
        .and_then(|c| c.get("result"))
        .and_then(|r| r.get(0))
        .cloned()
        .unwrap_or(json!({}));
    let ts = res
        .get("timestamp")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let closes = res
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(|q| q.get(0))
        .and_then(|q| q.get("close"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut hist = Vec::new();
    for (t, c) in ts.iter().zip(closes.iter()) {
        if c.is_null() {
            continue;
        }
        let (Some(t), Ok(c)) = (t.as_f64(), to_float(c)) else {
            return vec![];
        };
        hist.push(json!({"date": utc_date(t), "value": py_round(c, 2)}));
    }
    let skip = hist.len().saturating_sub(15);
    hist.split_off(skip)
}

/// `_get_m6_brent_symbol`: Brent contract six months out.
pub fn m6_brent_symbol(now: DateTime<Utc>) -> String {
    const CODES: [&str; 12] = ["F", "G", "H", "J", "K", "M", "N", "Q", "U", "V", "X", "Z"];
    let m = now.month() as i32 - 1 + 6;
    format!(
        "BZ{}{}.NYM",
        CODES[(m % 12) as usize],
        now.year() % 100 + m / 12
    )
}

struct Quote {
    price: f64,
    obs_date: String,
    json: Value,
}

async fn yahoo_quote(
    http: &Http,
    ep: &Endpoints,
    symbol: &str,
    now: DateTime<Utc>,
) -> PyResult<Quote> {
    let j = http
        .get(&format!("{}/{symbol}?range=1mo&interval=1d", ep.yahoo))
        .await?
        .json()?;
    let meta = key(index(key(key(&j, "chart")?, "result")?, 0)?, "meta")?;
    let price = py_round(to_float(key(meta, "regularMarketPrice")?)?, 2);
    let obs_date = utc_date(timestamp_of(meta, now)?);
    Ok(Quote {
        price,
        obs_date,
        json: j,
    })
}

fn closes_by_date(res: &Value) -> Vec<(String, f64)> {
    let ts = res
        .get("timestamp")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let cs = res
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(|q| q.get(0))
        .and_then(|q| q.get("close"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<(String, f64)> = Vec::new();
    for (t, c) in ts.iter().zip(cs.iter()) {
        let (Some(t), Some(c)) = (t.as_f64(), c.as_f64()) else {
            continue;
        };
        let d = utc_date(t);
        match out.iter_mut().find(|(k, _)| *k == d) {
            Some(e) => e.1 = c,
            None => out.push((d, c)),
        }
    }
    out
}

async fn brent_curve(http: &Http, ep: &Endpoints, now: DateTime<Utc>) -> Value {
    let sym = m6_brent_symbol(now);
    let attempt = async {
        let r1 = http
            .get(&format!("{}/BZ=F?range=1mo&interval=1d", ep.yahoo))
            .await?;
        let r6 = http
            .get(&format!("{}/{sym}?range=1mo&interval=1d", ep.yahoo))
            .await?;
        let (j1v, j6v) = (r1.json()?, r6.json()?);
        let j1 = index(key(key(&j1v, "chart")?, "result")?, 0)?;
        let j6 = index(key(key(&j6v, "chart")?, "result")?, 0)?;
        let p1 = to_float(key(key(j1, "meta")?, "regularMarketPrice")?)?;
        let p6 = to_float(key(key(j6, "meta")?, "regularMarketPrice")?)?;
        let spread = py_round(p1 - p6, 2);
        let (d1, d6) = (closes_by_date(j1), closes_by_date(j6));
        let mut common: Vec<&(String, f64)> = d1
            .iter()
            .filter(|(d, _)| d6.iter().any(|(k, _)| k == d))
            .collect();
        common.sort_by(|a, b| a.0.cmp(&b.0));
        let hist: Vec<Value> = common
            .iter()
            .map(|(d, c1)| {
                let c6 = d6
                    .iter()
                    .find(|(k, _)| k == d)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                json!({"date": d, "value": py_round(c1 - c6, 2)})
            })
            .collect();
        let hist = hist[hist.len().saturating_sub(15)..].to_vec();
        let obs_date = utc_date(timestamp_of(key(j1, "meta")?, now)?);
        Ok::<Value, PyErr>(json!({
            "status": "ok", "observed_at": obs_date, "value": spread, "release_at": format!("{obs_date}T19:30:00Z"),
            "fetched_at": now_iso(now),
            "quality_note": format!("Verified from Yahoo Finance NYMEX (BZ=F ${p1:.2} − {sym} ${p6:.2}: +{spread:.2} USD/barel)"),
            "upstream_ok": true, "history": hist,
        }))
    };
    match attempt.await {
        Ok(v) => v,
        Err(e) => json!({
            "status": "unavailable",
            "observed_at": now.date_naive().format("%Y-%m-%d").to_string(),
            "value": null,
            "fetched_at": now_iso(now),
            "quality_note": format!("Brent M6 ({sym}) feed unavailable ({}); spread cannot be derived", e.msg),
            "upstream_ok": false,
            "history": [],
        }),
    }
}

/// `_parse_official_observation`: latest finite date/value record.
pub fn parse_official(payload: &Value, metric_id: &str, now: DateTime<Utc>) -> Option<Value> {
    let obj = payload.as_object()?;
    let mut records = obj.get("data").cloned();
    if let Some(Value::Object(inner)) = &records {
        let pick = |k: &str| inner.get(k).filter(|v| truthy(v)).cloned();
        records = pick("data")
            .or_else(|| pick("items"))
            .or_else(|| inner.get("results").cloned());
    }
    if !matches!(records, Some(Value::Array(_))) {
        records = obj
            .iter()
            .find(|(k, v)| {
                ["data", "items", "results", "records"].contains(&k.to_lowercase().as_str())
                    && v.is_array()
            })
            .map(|(_, v)| v.clone());
    }
    let records = records?.as_array()?.clone();

    let field = |rec: &Map<String, Value>, names: &[&str]| {
        rec.iter()
            .find(|(k, _)| names.contains(&k.to_lowercase().as_str()))
            .map(|(_, v)| v.clone())
    };
    let mut parsed: Vec<(String, f64)> = Vec::new();
    for rec in records.iter().filter_map(Value::as_object) {
        let raw_value = field(
            rec,
            &["value", "nilai", "index", "pmi", "cpi", "core_cpi", "angka"],
        )
        .unwrap_or(Value::Null);
        let raw_date = field(
            rec,
            &[
                "date",
                "tanggal",
                "period",
                "periode",
                "observed_at",
                "time",
            ],
        )
        .unwrap_or(Value::Null);
        let Ok(value) = to_float(&raw_value) else {
            continue;
        };
        if !finite(value) || raw_date.is_null() || raw_date == json!("") {
            continue;
        }
        let s = py_str(&raw_date);
        let s = s.trim();
        let observed: String = if s.chars().count() >= 10 {
            s.chars().take(10).collect()
        } else {
            s.to_string()
        };
        let iso_ok = chrono::NaiveDate::parse_from_str(&observed, "%Y-%m-%d").is_ok()
            && observed.len() == 10
            || observed.len() == 8
                && chrono::NaiveDate::parse_from_str(&observed, "%Y%m%d").is_ok();
        if !iso_ok {
            continue;
        }
        parsed.push((observed, py_round(value, 2)));
    }
    // `max(..., key=date)` keeps the first of equal dates.
    let latest = parsed
        .iter()
        .fold(None::<&(String, f64)>, |acc, x| match acc {
            Some(a) if a.0 >= x.0 => Some(a),
            _ => Some(x),
        })?;
    let cpi = metric_id == "id_core_cpi";
    let history: Vec<Value> = parsed[parsed.len().saturating_sub(15)..]
        .iter()
        .map(|(d, v)| json!({"date": d, "value": v}))
        .collect();
    Some(json!({
        "status": "ok",
        "observed_at": latest.0,
        "value": latest.1,
        "release_at": if cpi { format!("{}T11:00:00+07:00", latest.0) } else { format!("{}T09:30:00+08:00", latest.0) },
        "fetched_at": now_iso(now),
        "quality_note": format!("Verified from official feed ({})", if cpi { "BPS" } else { "NBS_CHINA" }),
        "history": history,
        "unit": if cpi { "percent_yoy" } else { "index_points" },
    }))
}

pub async fn fetch_market(
    http: &Http,
    ep: &Endpoints,
    metric_id: &str,
    now: DateTime<Utc>,
) -> Value {
    let simple = |symbol: &'static str, release: &'static str| async move {
        let q = yahoo_quote(http, ep, symbol, now).await?;
        Ok::<Value, PyErr>(json!({
            "status": "ok", "observed_at": q.obs_date, "value": q.price,
            "release_at": format!("{}{release}", q.obs_date), "fetched_at": now_iso(now),
            "quality_note": "Verified from upstream feed (YAHOO_FINANCE)", "history": yahoo_history(&q.json),
        }))
    };
    let result = match metric_id {
        "brent_front_month" => simple("BZ=F", "T19:30:00Z").await,
        "brent_curve" => return brent_curve(http, ep, now).await,
        "wti_spot" => simple("CL=F", "T14:30:00-05:00").await,
        "vix" => simple("^VIX", "T16:15:00-05:00").await,
        "dxy" => simple("DX-Y.NYB", "T17:00:00Z").await,
        "usd_idr" => async {
            let j = http.get(&ep.er_api).await?.json()?;
            let rate = py_round(to_float(key(key(&j, "rates")?, "IDR")?)?, 2);
            let today = now.date_naive().format("%Y-%m-%d").to_string();
            Ok(json!({
                "status": "ok", "observed_at": today, "value": rate, "release_at": format!("{today}T16:15:00+07:00"),
                "fetched_at": now_iso(now), "quality_note": "Verified from upstream feed (OPEN_EXCHANGE_RATES)",
            }))
        }
        .await,
        "id_core_cpi" | "china_pmi_primary" => async {
            let url = if metric_id == "id_core_cpi" { &ep.bi_inflation } else { &ep.nbs_pmi };
            let j = http.get(url).await?.json()?;
            Ok(parse_official(&j, metric_id, now).unwrap_or_else(|| {
                json!({
                    "status": "unavailable",
                    "error_type": "UnparsedOfficialResponse",
                    "quality_note": format!("Official {metric_id} response contained no finite date/value record"),
                })
            }))
        }
        .await,
        _ => Ok(json!({"status": "unsupported"})),
    };
    result.unwrap_or_else(result_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 20, 8, 0, 0).unwrap()
    }

    #[test]
    fn fred_csv_and_spread_scaling() {
        let v = parse_fred(
            "observation_date,T10Y2Y\n2026-10-15,0.42\n2026-10-16,.\n2026-10-17,-0.051\n",
            "T10Y2Y",
            now(),
        )
        .unwrap();
        assert_eq!(v["value"], -5.1);
        assert_eq!(
            v["quality_note"],
            "Verified from FRED (T10Y2Y ×100: -5.1 bps)"
        );
        assert_eq!(
            v["history"],
            json!([{"date": "2026-10-15", "value": 42.0}, {"date": "2026-10-17", "value": -5.1}])
        );
        assert_eq!(
            parse_fred("DATE,VALUE\n2026-10-01,.\n", "DGS2", now())
                .unwrap_err()
                .msg,
            "No valid data points in FRED response for DGS2"
        );
    }

    #[test]
    fn cpi_yoy() {
        let csv =
            "observation_date,CPILFESL\n2025-08-01,329.7\n2025-09-01,330.0\n2026-08-01,337.765\n";
        let v = parse_fred(csv, "CPILFESL", now()).unwrap();
        assert_eq!(v["value"], 2.45);
        assert_eq!(
            v["quality_note"],
            "YoY 2.45% derived from CPILFESL index: 337.765 (2026-08-01) ÷ 329.700 (2025-08-01)"
        );
        assert_eq!(v["history"], json!([{"date": "2026-08-01", "value": 2.45}]));
        let missing = parse_fred(
            "observation_date,CPILFESL\n2026-08-01,337.765\n",
            "CPILFESL",
            now(),
        )
        .unwrap();
        assert_eq!(missing["status"], "unavailable");
    }

    #[test]
    fn m6_symbol_rolls_year() {
        assert_eq!(m6_brent_symbol(now()), "BZJ27.NYM");
        assert_eq!(
            m6_brent_symbol(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()),
            "BZU26.NYM"
        );
    }

    #[test]
    fn official_payload_shapes() {
        let p = json!({"data": {"items": [{"Tanggal": "2026-08-31T00:00:00", "Nilai": "2.12"}, {"tanggal": "2026-07-31", "nilai": 2.3}, {"date": "x", "value": 1}]}});
        let v = parse_official(&p, "id_core_cpi", now()).unwrap();
        assert_eq!(
            (v["observed_at"].as_str(), v["value"].as_f64()),
            (Some("2026-08-31"), Some(2.12))
        );
        assert_eq!(v["release_at"], "2026-08-31T11:00:00+07:00");
        assert!(parse_official(&json!({"Records": []}), "china_pmi_primary", now()).is_none());
        assert!(parse_official(&json!([1]), "china_pmi_primary", now()).is_none());
    }

    #[test]
    fn python_errors() {
        let v = json!({"chart": {"result": []}});
        assert_eq!(
            index(key(key(&v, "chart").unwrap(), "result").unwrap(), 0)
                .unwrap_err()
                .msg,
            "list index out of range"
        );
        assert_eq!(
            key(&json!(null), "x").unwrap_err().msg,
            "'NoneType' object is not subscriptable"
        );
        assert_eq!(
            key(&json!({}), "meta").unwrap_err(),
            PyErr::new("KeyError", "'meta'")
        );
        assert_eq!(
            Response {
                text: "<html>".into()
            }
            .json()
            .unwrap_err()
            .msg,
            "Expecting value: line 1 column 1 (char 0)"
        );
    }
}
