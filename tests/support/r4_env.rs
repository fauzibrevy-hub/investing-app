//! Shared setup for the R4 API tests (claims, journal, events, calendar, documents,
//! calibration): synthetic filings for three tickers on two packs plus one without a pack,
//! a year of generated daily prices, and an app under a clock the test can move.
//! Everything is invented; no value here comes from the owner's sheets or filings.

#![allow(dead_code)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::Router;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use finance_app::research::api::{self, ApiState, Options};
use finance_app::research::engine::Engine;
use finance_app::research::packs::{PackSet, Packs};
use finance_app::research::store::{PriceBar, Store};
use finance_app::research::universe::{sync, HeldPosition};
use finance_app::research::xbrl;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::pack_fixture;

pub const NOW: &str = "2026-10-24T09:00:00+00:00";
pub const HELD: [&str; 3] = ["COAL", "BANK", "NONE"];

pub struct Env {
    pub dir: tempfile::TempDir,
    pub store: Arc<Store>,
    pub app: Router,
    pub clock: Arc<Mutex<DateTime<Utc>>>,
    pub packs: Arc<Packs>,
}

impl Env {
    pub fn set_clock(&self, at: &str) {
        *self.clock.lock().unwrap() = DateTime::parse_from_rfc3339(at)
            .unwrap()
            .with_timezone(&Utc);
    }

    pub fn now(&self) -> DateTime<Utc> {
        *self.clock.lock().unwrap()
    }

    pub fn pack_set(&self) -> Arc<PackSet> {
        self.packs.set()
    }

    /// Today in WIB under the test clock.
    pub fn today(&self) -> NaiveDate {
        (self.now() + chrono::Duration::hours(7)).date_naive()
    }

    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let (s, v, _) = call(&self.app, Method::GET, uri, None, None).await;
        (s, v)
    }

    pub async fn send(
        &self,
        method: Method,
        uri: &str,
        key: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let (s, v, _) = call(&self.app, method, uri, Some(key), Some(body)).await;
        (s, v)
    }

    pub async fn post(&self, uri: &str, key: &str, body: Value) -> (StatusCode, Value) {
        self.send(Method::POST, uri, key, body).await
    }
}

pub fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// Weekday closes from 2025-10-01 up to and including `last`, rising 1 a day from `seed`.
pub fn closes(last: &str, seed: f64) -> Vec<PriceBar> {
    let mut day = d("2025-10-01");
    let end = d(last);
    let mut out = Vec::new();
    let mut n = 0.0;
    while day <= end {
        if day.weekday().num_days_from_monday() < 5 {
            out.push(PriceBar {
                date: day.format("%Y-%m-%d").to_string(),
                open: None,
                high: None,
                low: None,
                close: seed + n,
                volume: Some(1_000),
            });
            n += 1.0;
        }
        day = day.succ_opt().unwrap();
    }
    out
}

pub fn env() -> Env {
    env_at(NOW)
}

pub fn env_at(now: &str) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    let mut files = 0;
    for spec in [
        pack_fixture::coal(),
        pack_fixture::bank(),
        pack_fixture::none(),
    ] {
        files += pack_fixture::write_ticker(&xbrl_dir, &spec);
    }
    let store = Arc::new(Store::open(&dir.path().join("research.sqlite")).unwrap());
    let at = DateTime::parse_from_rfc3339(now)
        .unwrap()
        .with_timezone(&Utc);
    let report = xbrl::ingest_new(&store, &xbrl_dir, at).unwrap();
    assert_eq!(report.ingested.len(), files);
    let held: Vec<HeldPosition> = HELD
        .iter()
        .map(|t| HeldPosition {
            ticker: (*t).into(),
            avg_price: 1_000.0,
        })
        .collect();
    sync(&store, &held, now).unwrap();
    for t in HELD {
        store
            .upsert_prices(t, &closes("2026-10-23", 1_000.0))
            .unwrap();
    }
    let clock = Arc::new(Mutex::new(at));
    let c2 = clock.clone();
    let engine = Arc::new(Engine::new(
        store.clone(),
        Path::new("emitens.csv").to_path_buf(),
        Arc::new(move || *c2.lock().unwrap()),
    ));
    let packs = Arc::new(Packs::embedded());
    let app = api::router(ApiState::new(
        Some(engine),
        Options {
            idem_path: dir.path().join("research-idempotency.jsonl"),
            watchlist: Some(dir.path().join("research/watchlist.csv")),
            refresh_tx: None,
            packs: packs.clone(),
            rules_dir: Some(dir.path().join("rules")),
            documents_dir: dir.path().join("research/documents"),
        },
    ));
    Env {
        dir,
        store,
        app,
        clock,
        packs,
    }
}

pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, HeaderMap) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(k) = key {
        req = req.header("idempotency-key", k);
    }
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, headers)
}
