//! Registry, persistent store and observation building, ported from MC
//! `backend/services/macro_ingestion.py` (`CANONICAL_REGISTRY`,
//! `load/save_persistent_store`, `calculate_freshness`,
//! `build_observation_from_store`).

use std::path::Path;
use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// The canonical 15-metric registry, exactly as MC's `/api/macro/registry` serves it.
pub static REGISTRY_JSON: LazyLock<Vec<Value>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("registry.json")).expect("valid registry.json")
});

#[derive(Debug, Clone, Deserialize)]
pub struct Reg {
    pub metric_id: String,
    pub name: String,
    pub pillar: String,
    pub primary_series: String,
    pub unit: String,
    pub tier: String,
    pub source_id: String,
    pub basis: String,
    pub frequency: String,
}

pub static REGISTRY: LazyLock<Vec<Reg>> = LazyLock::new(|| {
    REGISTRY_JSON
        .iter()
        .map(|v| serde_json::from_value(v.clone()).expect("registry item"))
        .collect()
});

pub type Store = Map<String, Value>;

/// `load_persistent_store`: only a JSON object with exactly one entry per
/// registry metric is accepted; anything else reads as empty.
pub fn load_store(path: &Path) -> Store {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Store::new();
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(m)) if m.len() == REGISTRY.len() => m,
        Ok(_) => Store::new(),
        Err(e) => {
            tracing::warn!("failed to load persistent macro store: {e}");
            Store::new()
        }
    }
}

/// `save_persistent_store`: atomic temp + rename.
pub fn save_store(path: &Path, store: &Store) {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(store).expect("serializable"),
        )?;
        std::fs::rename(&tmp, path)
    };
    if let Err(e) = write() {
        tracing::error!("failed to save persistent macro store: {e}");
    }
}

/// `count_business_days`: weekdays in (start, end].
pub fn business_days(start: NaiveDate, end: NaiveDate) -> i64 {
    if start >= end {
        return 0;
    }
    let mut n = 0;
    let mut cur = start.succ_opt().expect("date");
    while cur <= end {
        if !matches!(cur.weekday(), Weekday::Sat | Weekday::Sun) {
            n += 1;
        }
        cur = cur.succ_opt().expect("date");
    }
    n
}

/// `date.fromisoformat` for the forms an observation date can take.
fn iso_date(s: &str) -> Option<NaiveDate> {
    match s.len() {
        10 => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
        8 if s.bytes().all(|b| b.is_ascii_digit()) => NaiveDate::parse_from_str(s, "%Y%m%d").ok(),
        _ => None,
    }
}

/// `calculate_freshness`: (status, note) against the server-side date.
pub fn freshness(
    metric_id: &str,
    observed: &str,
    as_of: NaiveDate,
    frequency: &str,
    upstream_ok: bool,
) -> (&'static str, String) {
    if !upstream_ok {
        return (
            "delayed",
            "Upstream provider offline or unreachable; using last verified observation".into(),
        );
    }
    let Some(obs) = iso_date(observed) else {
        return (
            "unavailable",
            format!("Invalid observation date: {observed}"),
        );
    };
    match frequency {
        "daily" => {
            let elapsed = business_days(obs, as_of);
            if metric_id == "us_hy_oas" {
                if elapsed <= 1 {
                    (
                        "delayed",
                        "1-trading-day publication lag per ICE/FRED release schedule".into(),
                    )
                } else {
                    (
                        "delayed",
                        format!(
                            "{elapsed} trading sessions elapsed; exceeds T+1 publication window"
                        ),
                    )
                }
            } else {
                match elapsed {
                    0 => ("fresh", "Current trading session verified".into()),
                    1 => (
                        "delayed",
                        "1 trading session elapsed since last close".into(),
                    ),
                    n => (
                        "delayed",
                        format!("{n} trading sessions elapsed without refresh"),
                    ),
                }
            }
        }
        "monthly" => {
            let months =
                (as_of.year() - obs.year()) * 12 + (as_of.month() as i32 - obs.month() as i32);
            match months {
                m if m <= 1 => ("fresh", "Current reporting period vintage verified".into()),
                2 => (
                    "delayed",
                    "Prior reporting vintage; new release pending".into(),
                ),
                m => (
                    "stale",
                    format!("Overdue monthly vintage ({m} months prior)"),
                ),
            }
        }
        "meeting_step" => {
            if (as_of - obs).num_days() <= 60 {
                ("fresh", "Active FOMC policy stance".into())
            } else {
                ("delayed", "Policy stance awaiting review".into())
            }
        }
        _ => ("fresh", "Standard verification".into()),
    }
}

/// `_is_finite_number` (bools are not numbers).
pub fn finite_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// `build_observation_from_store`. `value` is a float like pydantic's `Optional[float]`.
pub fn build_observation(reg: &Reg, as_of: NaiveDate, fred_isolated: bool, store: &Store) -> Value {
    let mut cached = store
        .get(&reg.metric_id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // CT1: project a FRED outage for this request only, never into the store.
    if fred_isolated && reg.source_id == "FRED" {
        cached.insert("upstream_ok".into(), json!(false));
        cached.insert(
            "quality_note".into(),
            json!("Upstream FRED connection failed (ConnectError); using last verified observation, not refreshed"),
        );
    }
    if cached.is_empty() {
        return json!({
            "series": reg.primary_series, "unit": reg.unit, "observed_at": "—", "release_at": "—", "fetched_at": "—",
            "source_id": reg.source_id, "tier": reg.tier, "basis": reg.basis, "freshness": "unavailable",
            "metric_id": reg.metric_id, "pillar": reg.pillar, "name": reg.name, "value": null,
            "quality_note": "No verified observation in persistent store", "upstream_ok": false, "history": null,
        });
    }

    let obs_at = cached
        .get("observed_at")
        .map(crate::py::py_str)
        .unwrap_or_else(|| "1970-01-01".into());
    let raw = cached.get("value").cloned().unwrap_or(Value::Null);
    if !raw.is_null() && finite_number(&raw).is_none() {
        cached.insert("value".into(), Value::Null);
        cached.insert("upstream_ok".into(), json!(false));
        cached.insert(
            "quality_note".into(),
            json!("Non-finite observation rejected; metric is unavailable for scoring"),
        );
    }
    let upstream_ok = cached.get("upstream_ok").is_some_and(crate::py::truthy);
    let (fresh, cal_note) = freshness(&reg.metric_id, &obs_at, as_of, &reg.frequency, upstream_ok);
    let quality_note = cached
        .get("quality_note")
        .filter(|v| crate::py::truthy(v))
        .cloned()
        .unwrap_or(json!(cal_note));

    // MC: the CPILFESL August 2026 vintage was released 2026-09-11.
    let release_at = if reg.metric_id == "us_core_cpi" && obs_at == "2026-08-01" {
        json!("2026-09-11T08:30:00Z")
    } else {
        cached
            .get("release_at")
            .cloned()
            .unwrap_or_else(|| json!(format!("{obs_at}T00:00:00Z")))
    };
    let history = match cached.get("history") {
        Some(Value::Array(points)) => {
            json!(points
                .iter()
                .filter(|p| p.is_object() && finite_number(&p["value"]).is_some())
                .cloned()
                .collect::<Vec<_>>())
        }
        Some(other) => other.clone(),
        None => Value::Null,
    };
    let value = cached
        .get("value")
        .and_then(finite_number)
        .map(|f| json!(f))
        .unwrap_or(Value::Null);

    json!({
        "series": reg.primary_series,
        "unit": reg.unit,
        "observed_at": obs_at,
        "release_at": release_at,
        "fetched_at": cached.get("fetched_at").cloned().unwrap_or_else(|| json!(format!("{obs_at}T00:00:00Z"))),
        "source_id": reg.source_id,
        "tier": reg.tier,
        "basis": reg.basis,
        "freshness": fresh,
        "metric_id": reg.metric_id,
        "pillar": reg.pillar,
        "name": reg.name,
        "value": value,
        "quality_note": quality_note,
        "upstream_ok": upstream_ok,
        "history": history,
    })
}

pub fn observations(as_of: NaiveDate, fred_isolated: bool, store: &Store) -> Vec<Value> {
    REGISTRY
        .iter()
        .map(|r| build_observation(r, as_of, fred_isolated, store))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    #[test]
    fn registry_has_15_metrics() {
        assert_eq!(REGISTRY.len(), 15);
        assert_eq!(REGISTRY[0].metric_id, "brent_front_month");
    }

    #[test]
    fn business_day_counting() {
        // Fri 2026-10-16 → Mon 2026-10-19 is one business day.
        assert_eq!(business_days(d(2026, 10, 16), d(2026, 10, 19)), 1);
        assert_eq!(business_days(d(2026, 10, 19), d(2026, 10, 19)), 0);
        assert_eq!(business_days(d(2026, 10, 12), d(2026, 10, 19)), 5);
    }

    #[test]
    fn freshness_rules() {
        let as_of = d(2026, 10, 20);
        assert_eq!(
            freshness("vix", "2026-10-20", as_of, "daily", true).0,
            "fresh"
        );
        assert_eq!(
            freshness("vix", "2026-10-16", as_of, "daily", true),
            (
                "delayed",
                "2 trading sessions elapsed without refresh".into()
            )
        );
        assert_eq!(
            freshness("us_hy_oas", "2026-10-19", as_of, "daily", true).1,
            "1-trading-day publication lag per ICE/FRED release schedule"
        );
        assert_eq!(
            freshness("x", "2026-09-01", as_of, "monthly", true).0,
            "fresh"
        );
        assert_eq!(
            freshness("x", "2026-08-31", as_of, "monthly", true).0,
            "delayed"
        );
        assert_eq!(
            freshness("x", "2026-06-01", as_of, "monthly", true),
            ("stale", "Overdue monthly vintage (4 months prior)".into())
        );
        assert_eq!(
            freshness("x", "2026-08-21", as_of, "meeting_step", true).0,
            "fresh"
        );
        assert_eq!(
            freshness("x", "—", as_of, "daily", true),
            ("unavailable", "Invalid observation date: —".into())
        );
        assert_eq!(
            freshness("x", "2026-10-20", as_of, "daily", false).0,
            "delayed"
        );
    }

    #[test]
    fn store_requires_all_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("macro_store.json");
        std::fs::write(&p, r#"{"vix": {}}"#).unwrap();
        assert!(load_store(&p).is_empty());
        let full: Store = REGISTRY
            .iter()
            .map(|r| (r.metric_id.clone(), json!({})))
            .collect();
        save_store(&p, &full);
        assert_eq!(load_store(&p).len(), 15);
    }

    #[test]
    fn observation_from_empty_and_isolated_store() {
        let reg = &REGISTRY[13]; // vix
        let empty = build_observation(reg, d(2026, 10, 20), false, &Store::new());
        assert_eq!(empty["freshness"], "unavailable");
        assert_eq!(empty["observed_at"], "—");

        let mut store = Store::new();
        store.insert("ust_2y".into(), json!({"observed_at": "2026-10-20", "value": 4, "upstream_ok": true, "history": [{"date": "x", "value": "bad"}, {"date": "y", "value": 1}]}));
        let ust = REGISTRY.iter().find(|r| r.metric_id == "ust_2y").unwrap();
        let o = build_observation(ust, d(2026, 10, 20), false, &store);
        assert_eq!(o["value"], json!(4.0));
        assert_eq!(o["freshness"], "fresh");
        assert_eq!(o["quality_note"], "Current trading session verified");
        assert_eq!(o["history"], json!([{"date": "y", "value": 1}]));
        let iso = build_observation(ust, d(2026, 10, 20), true, &store);
        assert_eq!(
            (iso["upstream_ok"].as_bool(), iso["freshness"].as_str()),
            (Some(false), Some("delayed"))
        );
        assert!(
            !store["ust_2y"]
                .as_object()
                .unwrap()
                .contains_key("quality_note"),
            "isolation never touches the store"
        );
    }
}
