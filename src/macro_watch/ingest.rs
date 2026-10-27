//! One ingestion cycle, ported from MC `execute_upstream_ingestion` +
//! `fetch_canonical_observations_async(run_ingest=True)`. Every upstream is
//! fetched concurrently and isolated: one failing feed only marks its own
//! metric (plan.md R6).

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use super::observe::{Reg, Store, REGISTRY};
use super::upstream::{fetch_fred, fetch_market, Endpoints, Http};
use crate::py::truthy;

fn fred_series(metric_id: &str) -> Option<&'static str> {
    Some(match metric_id {
        "ust_2y" => "DGS2",
        "ust_10y" => "DGS10",
        "ust_10y_2y_spread" => "T10Y2Y",
        "ust_10y_real_yield" => "DFII10",
        "us_hy_oas" => "BAMLH0A0HYM2",
        "us_core_cpi" => "CPILFESL",
        "fed_funds_path" => "DFEDTARU",
        _ => return None,
    })
}

pub struct Clients {
    pub fred: Http,
    pub market: Http,
    pub endpoints: Endpoints,
}

impl Clients {
    pub fn new(endpoints: Endpoints) -> Self {
        Self {
            // FRED drops requests without a User-Agent; send httpx's default like MC does.
            fred: Http::new(Some("python-httpx/0.28.1")),
            market: Http::new(Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )),
            endpoints,
        }
    }
}

async fn fetch(clients: &Clients, reg: &Reg, disable_fred: bool, now: DateTime<Utc>) -> Value {
    match fred_series(&reg.metric_id) {
        Some(sid) => fetch_fred(&clients.fred, &clients.endpoints, sid, disable_fred, now).await,
        None => fetch_market(&clients.market, &clients.endpoints, &reg.metric_id, now).await,
    }
}

/// Python `==` between two decoded JSON values (numbers compare by value).
fn py_eq(a: Option<&Value>, b: &Value) -> bool {
    match (a, b) {
        (Some(Value::Number(x)), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Some(x), y) => x == y,
        (None, Value::Null) => true,
        (None, _) => false,
    }
}

fn s(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty())
}

/// Apply one upstream result to the cached entry; returns whether the
/// observation changed (`is_updated`).
pub fn apply(reg: &Reg, res: &Value, store: &mut Store) -> bool {
    let mut cached: Map<String, Value> = store
        .get(&reg.metric_id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let status = res.get("status").and_then(Value::as_str).unwrap_or("");
    let mut updated = false;
    let unit = s(&res["unit"])
        .map(str::to_string)
        .unwrap_or_else(|| reg.unit.clone());

    if let Some(sid) = fred_series(&reg.metric_id) {
        if status == "ok" {
            let (obs_at, val) = (res["observed_at"].clone(), res["value"].clone());
            updated =
                !py_eq(cached.get("observed_at"), &obs_at) || !py_eq(cached.get("value"), &val);
            let history = [res.get("history"), cached.get("history")]
                .into_iter()
                .flatten()
                .find(|h| truthy(h))
                .cloned()
                .unwrap_or(json!([]));
            cached = json!({
                "observed_at": obs_at,
                "value": val,
                "release_at": format!("{}T16:15:00Z", crate::py::py_str(&obs_at)),
                "fetched_at": res["fetched_at"],
                "upstream_ok": true,
                "quality_note": s(&res["quality_note"]).map(str::to_string).unwrap_or_else(|| format!("Verified from FRED ({sid})")),
                "history": history,
                "value_unit": unit,
            })
            .as_object()
            .cloned()
            .expect("object");
        } else {
            // Keep the old fetched_at; surface the failure verbatim.
            let err_type = res
                .get("error_type")
                .and_then(Value::as_str)
                .unwrap_or("Error");
            let note = s(&res["quality_note"]).or_else(|| s(&res["error"])).map(str::to_string).unwrap_or_else(|| {
                format!("Upstream FRED connection failed ({err_type}); using last verified observation, not refreshed")
            });
            cached.insert("upstream_ok".into(), json!(false));
            cached.insert("quality_note".into(), json!(note));
        }
    } else if status == "ok" && res.get("value").is_some() {
        let (obs_at, val) = (res["observed_at"].clone(), res["value"].clone());
        updated = !py_eq(cached.get("observed_at"), &obs_at) || !py_eq(cached.get("value"), &val);
        let obs_str = crate::py::py_str(&obs_at);
        let release_at = s(&res["release_at"])
            .map(|r| json!(r))
            .or_else(|| cached.get("release_at").cloned())
            .unwrap_or_else(|| json!(format!("{obs_str}T17:00:00Z")));
        let mut existing: Vec<Value> = cached
            .get("history")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let history = if res.get("history").is_some_and(truthy) {
            res["history"].clone()
        } else if !existing.is_empty() {
            if !existing.iter().any(|p| py_eq(p.get("date"), &obs_at)) {
                existing.push(json!({"date": obs_at, "value": val}));
            }
            json!(existing[existing.len().saturating_sub(15)..])
        } else {
            json!([{"date": obs_at, "value": val}])
        };
        cached = json!({
            "observed_at": obs_at,
            "value": val,
            "release_at": release_at,
            "fetched_at": res["fetched_at"],
            "upstream_ok": true,
            "quality_note": s(&res["quality_note"]).map(str::to_string).unwrap_or_else(|| format!("Verified from upstream feed ({})", reg.source_id)),
            "history": history,
            "value_unit": unit,
        })
        .as_object()
        .cloned()
        .expect("object");
    } else if status == "unavailable" {
        // A successful response without a finite value is not an observation.
        let note = s(&res["quality_note"])
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "Official {} response unavailable; no finite observation parsed",
                    reg.source_id
                )
            });
        cached.insert("value".into(), Value::Null);
        cached.insert("upstream_ok".into(), json!(false));
        cached.insert("quality_note".into(), json!(note));
        cached.insert("history".into(), json!([]));
    } else {
        let err_type = res
            .get("error_type")
            .and_then(Value::as_str)
            .unwrap_or("Error");
        let note = s(&res["quality_note"]).or_else(|| s(&res["error"])).map(str::to_string).unwrap_or_else(|| {
            format!("Upstream {} feed unreachable ({err_type}); using last verified observation, not refreshed", reg.source_id)
        });
        cached.insert("upstream_ok".into(), json!(false));
        cached.insert("quality_note".into(), json!(note));
    }
    store.insert(reg.metric_id.clone(), Value::Object(cached));
    updated
}

/// Fetch every metric concurrently (no store lock held during network I/O).
pub async fn fetch_all(clients: &Clients, disable_fred: bool, now: DateTime<Utc>) -> Vec<Value> {
    futures::future::join_all(
        REGISTRY
            .iter()
            .map(|r| fetch(clients, r, disable_fred, now)),
    )
    .await
}

/// Apply results in registry order; returns the ids of observations that changed.
pub fn apply_all(store: &mut Store, results: &[Value]) -> Vec<String> {
    let mut changed = Vec::new();
    for (reg, res) in REGISTRY.iter().zip(results) {
        if apply(reg, res, store) {
            changed.push(reg.metric_id.clone());
        }
    }
    changed
}
