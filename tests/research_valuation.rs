//! Valuation and scenarios at API level (research plan §4.6, R3.4; prd.md A3.2 to A3.4)
//! over the synthetic pack roster with ten generated years of daily prices, dividends and
//! driver rows, under a fixed clock. Golden payloads live in
//! `tests/fixtures/research/valuation/<TICKER>.json`; `UPDATE_GOLDEN=1` rewrites them.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::Router;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use http_body_util::BodyExt;
use investing_app::research::api::{self, ApiState, Options};
use investing_app::research::engine::Engine;
use investing_app::research::packs::Packs;
use investing_app::research::store::{CorpEventRow, EventKind, PriceBar, Store};
use investing_app::research::universe::{add, sync, AddRequest, HeldPosition, Role};
use investing_app::research::valuation::{EMBEDDED_RULES, RULES_FILE};
use investing_app::research::xbrl;
use serde_json::{json, Value};
use tower::ServiceExt;

const NOW: &str = "2026-10-24T09:00:00+00:00";
const HELD: [&str; 6] = ["BANK", "COAL", "HEVY", "SHIP", "DIST", "NONE"];
const ALL: [&str; 7] = ["BANK", "COAL", "HEVY", "SHIP", "TANK", "DIST", "NONE"];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/research/valuation")
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// Weekdays from 2016-10-24 to 2026-10-23 with a deterministic, mildly cyclical close.
fn prices(seed: usize) -> Vec<PriceBar> {
    let base = 500.0 * (seed as f64 + 1.0);
    let mut day = d("2016-10-24");
    let mut out = Vec::new();
    let mut n = 0usize;
    while day <= d("2026-10-23") {
        if day.weekday().num_days_from_monday() < 5 {
            let t = n as f64;
            let close = base * (1.0 + 0.00025 * t + 0.08 * (t / 97.0).sin());
            out.push(PriceBar {
                date: day.format("%Y-%m-%d").to_string(),
                open: None,
                high: Some(close * 1.01),
                low: Some(close * 0.99),
                close: (close * 100.0).round() / 100.0,
                volume: Some(1_000 + (n % 50) as i64),
            });
            n += 1;
        }
        day = day.succ_opt().unwrap();
    }
    out
}

fn dividends(ticker: &str, seed: usize) -> Vec<CorpEventRow> {
    (2017..=2026)
        .map(|y| CorpEventRow {
            ticker: ticker.into(),
            date: format!("{y}-05-15"),
            kind: EventKind::Dividend,
            amount: Some(10.0 * (seed as f64 + 1.0) + (y - 2017) as f64),
            ratio_num: None,
            ratio_den: None,
        })
        .collect()
}

fn months(from: (i32, u32), to: (i32, u32)) -> Vec<String> {
    let mut out = Vec::new();
    let (mut y, mut m) = from;
    loop {
        out.push(format!("{y}-{m:02}-01"));
        if (y, m) == to {
            break;
        }
        m += 1;
        if m == 13 {
            m = 1;
            y += 1;
        }
    }
    out
}

struct Env {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    app: Router,
}

/// Filings for the seven tickers, prices, dividends, drivers, and the app under a fixed
/// clock (2026-10-24 09:00 UTC) so goldens do not drift.
fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    assert_eq!(pack_fixture::write_all(&xbrl_dir), 7 * 18);
    let db = dir.path().join("research.sqlite");
    let store = Arc::new(Store::open(&db).unwrap());
    let now = Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap();
    let report = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
    assert_eq!(report.ingested.len(), 7 * 18);
    let held: Vec<HeldPosition> = HELD
        .iter()
        .map(|t| HeldPosition {
            ticker: (*t).into(),
            avg_price: 1_000.0,
        })
        .collect();
    sync(&store, &held, NOW).unwrap();
    let packs = Packs::embedded().set();
    add(
        &store,
        &packs,
        &AddRequest {
            ticker: "TANK".into(),
            role: Some(Role::Peer),
            peer_of: Some("SHIP".into()),
            ..Default::default()
        },
        NOW,
    )
    .unwrap();
    for (i, t) in ALL.iter().enumerate() {
        store.upsert_prices(t, &prices(i)).unwrap();
        store.upsert_events(&dividends(t, i)).unwrap();
        // The pack fixtures carry neither a share tag nor EPS, so the owner's manual count
        // is the point-in-time source (D13 step 2); the derived path is covered in
        // research_facts.rs.
        store
            .set_shares_manual(&investing_app::research::store::ManualShares {
                ticker: (*t).into(),
                shares: 1_000_000.0 * (i as f64 + 1.0),
                as_of: "2016-10-01".into(),
                source_note: "prospektus sintetis".into(),
                entered_at: NOW.into(),
            })
            .unwrap();
    }
    store.upsert_prices("^JKSE", &prices(9)).unwrap();
    // Drivers: CPI monthly (2015 = 100, slow rise), the 10-year yield as a monthly series
    // since 2024-01, USD/IDR monthly for the USD filer.
    let cpi: Vec<(String, f64)> = months((2016, 10), (2026, 9))
        .into_iter()
        .enumerate()
        .map(|(i, m)| (m, 100.0 + 0.3 * i as f64))
        .collect();
    store.upsert_drivers("cpi", "OECD", NOW, &cpi).unwrap();
    let bond: Vec<(String, f64)> = months((2024, 1), (2026, 10))
        .into_iter()
        .enumerate()
        .map(|(i, m)| (m, 7.0 - 0.02 * i as f64))
        .collect();
    store.upsert_drivers("sbn_10y", "PHEI", NOW, &bond).unwrap();
    let fx: Vec<(String, f64)> = months((2016, 10), (2026, 10))
        .into_iter()
        .enumerate()
        .map(|(i, m)| (m, 13_000.0 + 40.0 * i as f64))
        .collect();
    store
        .upsert_drivers("usd_idr", "Yahoo IDR=X", NOW, &fx)
        .unwrap();

    let engine = Arc::new(Engine::new(
        store.clone(),
        dir.path().join("emitens.csv"),
        Arc::new(move || now),
    ));
    let app = api::router(ApiState::new(
        Some(engine),
        Options {
            idem_path: dir.path().join("research-idempotency.jsonl"),
            watchlist: Some(dir.path().join("research/watchlist.csv")),
            refresh_tx: None,
            packs: Arc::new(Packs::embedded()),
            rules_dir: Some(dir.path().join("rules")),
            documents_dir: dir.path().join("research/documents"),
        },
    ));
    Env { dir, store, app }
}

async fn call(
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

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let (s, v, _) = call(app, Method::GET, uri, None, None).await;
    (s, v)
}

/// Golden form: `as_of` is clock-dependent, amounts of a billion or more are scaled down
/// by 1e6 so no fixture holds a ten-digit run, fractions rounded to six decimals.
fn golden_form(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    let y = if k == "as_of" {
                        json!("<dynamic>")
                    } else {
                        golden_form(x)
                    };
                    (k.clone(), y)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(golden_form).collect()),
        Value::Number(n) if n.as_f64().is_some_and(|x| x.abs() >= 1e9) => {
            json!((n.as_f64().unwrap() / 1e6 * 1e4).round() / 1e4)
        }
        Value::Number(n) => match n.as_f64() {
            Some(x) if x.fract() != 0.0 => json!((x * 1e6).round() / 1e6),
            _ => v.clone(),
        },
        other => other.clone(),
    }
}

fn golden(name: &str, actual: &Value) {
    let path = fixtures().join(format!("{name}.json"));
    let actual = golden_form(actual);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(fixtures()).unwrap();
        let mut text = serde_json::to_string_pretty(&actual).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing fixture {name}.json (run with UPDATE_GOLDEN=1)"));
    let expected: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        expected, actual,
        "{name}.json differs; re-run with UPDATE_GOLDEN=1 if the change is intended"
    );
}

/// Long series cut to three points so the golden stays small and reviewable.
fn trimmed(v: &Value) -> Value {
    match v {
        Value::Array(a) if a.len() > 3 && a.iter().all(Value::is_array) => {
            Value::Array(a.iter().take(3).map(trimmed).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(trimmed).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), trimmed(x))).collect()),
        other => other.clone(),
    }
}

const VERDICTS: [&str; 9] = [
    "fair", "wajar", "murah", "mahal", "layak", "beli", "jual", "keluar", "coret",
];

#[tokio::test]
async fn golden_valuation_payloads_and_no_fair_value_anywhere() {
    let e = env();
    let long = regex::Regex::new("[0-9]{10,}").unwrap();
    for t in ALL {
        let (s, v) = get(&e.app, &format!("/api/research/{t}/valuation")).await;
        assert_eq!(s, StatusCode::OK, "{t}: {v}");
        assert_eq!(v["ticker"], t);
        assert_eq!(v["rules_version"], "2026.10-r3");
        assert_eq!(v["price"]["date"], "2026-10-23");
        assert_eq!(v["reverse_dcf"]["rows"].as_array().unwrap().len(), 3);
        for (k, want) in [("ttm", true), ("cycle5y", true), ("cycle10y", true)] {
            assert_eq!(v["multiples"]["pe"].get(k).is_some(), want, "{t} {k}");
        }
        let text = golden_form(&v).to_string();
        let lower = text.to_lowercase();
        for w in VERDICTS {
            assert!(!lower.contains(w), "{t}: `{w}` in the valuation payload");
        }
        assert!(!long.is_match(&text), "{t}: ten-digit run");
        golden(t, &v);
    }
    let (s, _) = get(&e.app, "/api/research/ZZZZ/valuation").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = get(&e.app, "/api/research/BANK/valuation?fcf=weekly").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a3_2_reverse_dcf_shows_three_rates_and_follows_the_rules_file() {
    let e = env();
    let (_, v) = get(&e.app, "/api/research/HEVY/valuation").await;
    let dcf = &v["reverse_dcf"];
    assert_eq!(dcf["applicable"], true, "{dcf}");
    assert_eq!(dcf["fcf_base"], "ttm");
    let rates: Vec<f64> = dcf["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["discount_rate"].as_f64().unwrap())
        .collect();
    assert_eq!(rates, [9.0, 11.0, 13.0]);
    let growth: Vec<f64> = dcf["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["implied_growth"].as_f64().unwrap())
        .collect();
    assert!(growth[0] < growth[1] && growth[1] < growth[2], "{growth:?}");
    // The FCF base toggle changes the base and the implied growth.
    let (_, avg) = get(&e.app, "/api/research/HEVY/valuation?fcf=avg5y").await;
    assert_eq!(avg["reverse_dcf"]["fcf_base"], "avg5y");
    assert_eq!(avg["reverse_dcf"]["fcf"], avg["reverse_dcf"]["fcf_avg5y"]);
    assert_ne!(avg["reverse_dcf"]["fcf"], dcf["fcf"]);
    // A USD filer is converted with the USD/IDR driver and says so.
    let (_, coal) = get(&e.app, "/api/research/COAL/valuation").await;
    assert_eq!(coal["currency"], "USD");
    assert!(coal["fx_usd_idr"].as_f64().unwrap() > 13_000.0);
    // The synthetic COAL market cap is far below its converted FCF: the implied growth
    // falls outside the search bracket and the payload says so instead of showing rows.
    assert_eq!(
        coal["reverse_dcf"]["applicable"], false,
        "{}",
        coal["reverse_dcf"]
    );
    assert_eq!(
        coal["reverse_dcf"]["reason"],
        "pertumbuhan tersirat di luar rentang pencarian"
    );
    assert!(
        coal["reverse_dcf"]["fcf"].as_f64().unwrap() > 1e11,
        "FCF converted to IDR"
    );

    // Editing the rules file and reloading changes the rates and bumps the version.
    let rules_dir = e.dir.path().join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(RULES_FILE),
        EMBEDDED_RULES
            .replace("[9.0, 11.0, 13.0]", "[8.0, 10.0, 12.0]")
            .replace("version = \"2026.10-r3\"", "version = \"2026.11-r4\""),
    )
    .unwrap();
    let (s, r, _) = call(
        &e.app,
        Method::POST,
        "/api/research/packs/reload",
        Some("reload-1"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["rules"]["version"], "2026.11-r4");
    let (_, rules) = get(&e.app, "/api/research/rules").await;
    assert_eq!(rules["rules_version"], "2026.11-r4");
    assert_eq!(rules["discount_rates"], json!([8.0, 10.0, 12.0]));
    assert_eq!(rules["cycle_windows"], json!([5, 10]));
    assert_eq!(rules["percentile_window_years"], 10);
    assert_eq!(rules["pojk_offsets"]["fy"], 90);
    assert!(rules["changelog"].as_array().unwrap().len() == 1);
    assert!(rules["source"].as_str().unwrap().ends_with("rules.toml"));
    let (_, v2) = get(&e.app, "/api/research/HEVY/valuation").await;
    assert_eq!(v2["rules_version"], "2026.11-r4");
    let rates2: Vec<f64> = v2["reverse_dcf"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["discount_rate"].as_f64().unwrap())
        .collect();
    assert_eq!(rates2, [8.0, 10.0, 12.0]);
    assert!(
        v2["reverse_dcf"]["rows"][0]["implied_growth"]
            .as_f64()
            .unwrap()
            < growth[0]
    );
    // A broken file falls back to the embedded rules and reports the error.
    std::fs::write(rules_dir.join(RULES_FILE), "discount_rates = 'x'\n").unwrap();
    let (_, r, _) = call(
        &e.app,
        Method::POST,
        "/api/research/packs/reload",
        Some("reload-2"),
        None,
    )
    .await;
    assert_eq!(r["rules"]["version"], "2026.10-r3");
    assert!(r["rules"]["error"].as_str().unwrap().contains("rules.toml"));
}

#[tokio::test]
async fn a3_3_multiples_ttm_and_cycle_adjusted_side_by_side_with_percentiles_and_share_source() {
    let e = env();
    let (_, v) = get(&e.app, "/api/research/HEVY/valuation").await;
    let m = &v["multiples"];
    for key in ["pe", "pb", "ev_ebitda", "dividend_yield"] {
        for variant in ["ttm", "cycle5y", "cycle10y"] {
            let b = &m[key][variant];
            assert!(b.get("value").is_some(), "{key}.{variant}: {b}");
            if let Some(p) = b["percentile"].as_f64() {
                assert!((0.0..=100.0).contains(&p), "{key}.{variant} percentile {p}");
            }
        }
        let ttm = &m[key]["ttm"];
        assert!(ttm["value"].is_number(), "{key} ttm: {ttm}");
        assert!(ttm["percentile"].is_number(), "{key} ttm percentile: {ttm}");
        assert!(
            ttm["n"].as_u64().unwrap() > 750,
            "{key}: {} (three years minimum)",
            ttm["n"]
        );
        assert!(ttm["median"].is_number() && ttm["min"].is_number() && ttm["max"].is_number());
    }
    // Five fiscal years on disk (2022 to 2025 complete, 2026 to Q2): the 5-year window
    // has four years (>= 3), the 10-year window the same four years, CPI-adjusted.
    let c5 = &m["pe"]["cycle5y"];
    assert!(c5["value"].is_number(), "{c5}");
    assert_eq!(c5["cpi_adjusted"], true);
    assert!(c5["n"].as_u64().unwrap() > 0);
    assert_ne!(c5["value"], m["pe"]["ttm"]["value"]);
    // The share count is point-in-time with its source shown.
    assert_eq!(v["shares"]["source"]["kind"], "manual");
    assert_eq!(v["shares"]["source"]["source_note"], "prospektus sintetis");
    assert_eq!(v["shares"]["source_label"], "manual");
    assert_eq!(v["shares"]["count"], 3_000_000.0);
    // A later manual count (a rights issue) takes over from its date on.
    e.store
        .set_shares_manual(&investing_app::research::store::ManualShares {
            ticker: "HEVY".into(),
            shares: 2_000_000.0,
            as_of: "2026-01-15".into(),
            source_note: "prospektus sintetis".into(),
            entered_at: NOW.into(),
        })
        .unwrap();
    let (_, v2) = get(&e.app, "/api/research/HEVY/valuation").await;
    assert_eq!(v2["shares"]["source"]["as_of"], "2026-01-15");
    assert_eq!(v2["shares"]["count"], 2_000_000.0);
    assert_ne!(v2["market_cap"], v["market_cap"]);
    // Earnings yield vs the 10-year bond with the spread's percentile.
    let ey = &v["earnings_yield"];
    assert_eq!(ey["bond_status"], "tersedia");
    assert!(ey["bond_yield"].as_f64().unwrap() > 6.0);
    assert!(ey["spread"].is_number());
    assert!(ey["spread_band"]["n"].as_u64().unwrap() > 500);
    // NONE (no pack, a general filer) still gets core-based multiples.
    let (_, none) = get(&e.app, "/api/research/NONE/valuation").await;
    assert!(none["multiples"]["pe"]["ttm"]["value"].is_number());
}

#[tokio::test]
async fn a3_4_scenarios_accept_owner_inputs_and_show_value_per_share_with_gap() {
    let e = env();
    let uri = "/api/research/HEVY/scenarios";
    let (s, v, _) = call(&e.app, Method::PUT, uri, None, Some(json!({}))).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "writes need an Idempotency-Key: {v}"
    );
    let body = json!({
        "bear": {"growth": 0.0, "margin": 8.0, "exit_multiple": 8.0, "years": 5},
        "base": {"growth": 8.0, "margin": 10.0, "exit_multiple": 12.0, "years": 5},
        "bull": {"growth": 15.0, "margin": 12.0, "exit_multiple": 15.0, "years": 5}
    });
    let (s, v, _) = call(&e.app, Method::PUT, uri, Some("sc-1"), Some(body.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let rows = v["scenarios"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let price = v["price"]["close"].as_f64().unwrap();
    let mut values = Vec::new();
    for (row, name) in rows.iter().zip(["bear", "base", "bull"]) {
        assert_eq!(row["name"], name);
        let value = row["value_per_share"].as_f64().unwrap();
        let gap = row["gap_pct"].as_f64().unwrap();
        assert!(
            (gap - 100.0 * (value / price - 1.0)).abs() < 1e-2,
            "gap {gap} vs {value} / {price} (replies are rounded to 4 decimals)"
        );
        assert!(row["action"].is_null());
        values.push(value);
    }
    assert!(values[0] < values[1] && values[1] < values[2], "{values:?}");
    assert_eq!(v["scenarios"]["discount_rate"], 11.0);
    // Replay with the same key, then the GET shows the same table.
    let (s, v2, h) = call(&e.app, Method::PUT, uri, Some("sc-1"), Some(body)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        golden_form(&v2),
        golden_form(&v),
        "the journaled replay rounds floats"
    );
    assert_eq!(h["idempotent-replay"], "true");
    let (_, full) = get(&e.app, "/api/research/HEVY/valuation").await;
    assert_eq!(full["scenarios"]["rows"], v["scenarios"]["rows"]);
    // Partial inputs are kept without a value; null clears a row; validation is per field.
    let (s, v3, _) = call(
        &e.app,
        Method::PUT,
        uri,
        Some("sc-2"),
        Some(json!({"bull": null, "bear": {"growth": 2.0, "margin": null, "exit_multiple": 9.0, "years": 4}})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v3}");
    let rows = v3["scenarios"]["rows"].as_array().unwrap();
    assert!(rows[0]["value_per_share"].is_null() && rows[0]["growth"] == 2.0);
    assert!(rows[1]["value_per_share"].is_number(), "base untouched");
    assert_eq!(rows[2]["action"], "Isi skenario");
    for bad in [
        json!({"base": {"growth": 500}}),
        json!({"base": {"years": 0}}),
        json!({"base": {"exit_multiple": 0}}),
        json!({"base": {"margin": "x"}}),
        json!({"worst": {"growth": 1}}),
        json!({"base": {"growht": 1}}),
        json!({"bear": {"growth": 7.0}, "base": {"years": 99}}),
        json!([1, 2]),
    ] {
        let (s, _, _) = call(&e.app, Method::PUT, uri, Some("sc-bad"), Some(bad.clone())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    // A rejected body writes nothing: bear still carries the earlier partial input.
    let (_, after) = get(&e.app, "/api/research/HEVY/valuation").await;
    assert_eq!(after["scenarios"]["rows"][0]["growth"], 2.0);
    assert!(after["scenarios"]["rows"][1]["value_per_share"].is_number());
    let (s, _, _) = call(
        &e.app,
        Method::PUT,
        "/api/research/ZZZZ/scenarios",
        Some("sc-404"),
        Some(json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // The rows are logged: a rebuild restores them.
    let before = e.store.scenarios("HEVY").unwrap();
    assert_eq!(before.len(), 2);
    e.store.rebuild().unwrap();
    assert_eq!(e.store.scenarios("HEVY").unwrap(), before);
    // Nothing else in the tab calls anything a fair value: the only value per share is in
    // the scenario rows.
    let text = full.to_string().to_lowercase();
    for w in VERDICTS {
        assert!(!text.contains(w), "`{w}` in the valuation payload");
    }
    let count = text.matches("value_per_share").count();
    assert_eq!(
        count, 3,
        "one value per share per scenario row and nowhere else"
    );
}

// ---- R3.5: GET /{ticker}/prices ----

#[tokio::test]
async fn a3_5_prices_payload_carries_price_ma200_drivers_index_relative_events_and_stats() {
    let e = env();
    let long = regex::Regex::new("[0-9]{10,}").unwrap();
    for t in ALL {
        let (s, v) = get(&e.app, &format!("/api/research/{t}/prices?range=1y")).await;
        assert_eq!(s, StatusCode::OK, "{t}: {v}");
        assert_eq!(v["range"], "1y");
        assert_eq!(v["empty"], false);
        let n = v["price"].as_array().unwrap().len();
        assert!((240..=270).contains(&n), "{t}: {n} closes");
        assert_eq!(
            v["ma200"].as_array().unwrap().len(),
            n,
            "MA200 spans the whole range"
        );
        assert_eq!(v["index_relative"].as_array().unwrap().len(), n);
        assert_eq!(
            v["index_relative"][0][1], 100.0,
            "rebased at the range start"
        );
        assert_eq!(v["levels"], json!([]), "no journal levels written");
        let events = v["events"].as_array().unwrap();
        assert_eq!(events.len(), 1, "{t}: one dividend a year");
        assert_eq!(events[0]["kind"], "dividend");
        let st = &v["stats"];
        assert_eq!(st["close"]["date"], "2026-10-23");
        assert!(st["high_52w"]["drawdown_pct"].as_f64().unwrap() <= 0.0);
        assert!(st["volatility_1y_pct"].as_f64().unwrap() > 0.0);
        assert!(st["vs_index_6m_pct"].is_number());
        assert!(st["dividend_yield_ttm_pct"].as_f64().unwrap() > 0.0);
        if HELD.contains(&t) {
            assert_eq!(st["avg_cost"]["value"], 1_000.0, "{t}");
        } else {
            assert!(st["avg_cost"].is_null(), "{t} is a peer");
        }
        let text = golden_form(&v).to_string().to_lowercase();
        for w in VERDICTS {
            assert!(!text.contains(w), "{t}: `{w}` in the prices payload");
        }
        assert!(!long.is_match(&text));
        golden(&format!("{t}-prices-1y"), &trimmed(&v));
    }
    // Ranges: 1y, 3y, 5y, 10y of generated history; anything else is 422.
    for (range, min, max) in [
        ("3y", 740, 790),
        ("5y", 1_240, 1_320),
        ("10y", 2_480, 2_620),
    ] {
        let (s, v) = get(&e.app, &format!("/api/research/HEVY/prices?range={range}")).await;
        assert_eq!(s, StatusCode::OK);
        let n = v["price"].as_array().unwrap().len();
        assert!((min..=max).contains(&n), "{range}: {n}");
    }
    let (s, _) = get(&e.app, "/api/research/HEVY/prices?range=2y").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = get(&e.app, "/api/research/ZZZZ/prices").await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // A3.5: the pack's drivers are listed with their series; the bank's BI rate has none
    // stored here (status says so), the coal pack's USD/IDR is plotted, the shipping pack
    // plots through labelled proxies only, and TBD drivers are listed, never omitted.
    let (_, bank) = get(&e.app, "/api/research/BANK/prices?range=3y").await;
    let d = bank["drivers"].as_array().unwrap();
    assert_eq!(d.len(), 2);
    assert_eq!(d[0]["id"], "bi_rate");
    assert_eq!(d[0]["status"], "belum diambil");
    assert_eq!(d[1]["status"], "TBD");
    let (_, coal) = get(&e.app, "/api/research/COAL/prices?range=3y").await;
    let d = coal["drivers"].as_array().unwrap();
    let fx = d.iter().find(|x| x["id"] == "usd_idr").unwrap();
    assert_eq!(fx["status"], "tersedia");
    assert!(
        fx["series"].as_array().unwrap().len() >= 35,
        "{}",
        fx["series"]
    );
    assert_eq!(fx["unit"], "IDR");
    assert!(coal["drivers_plotted"].as_u64().unwrap() >= 1);
    let hba = d.iter().find(|x| x["id"] == "hba").unwrap();
    assert_eq!(hba["status"], "TBD");
    assert_eq!(hba["proxy"]["id"], "newcastle");
    e.store
        .upsert_drivers(
            "bwet",
            "Yahoo BWET",
            NOW,
            &[("2026-10-01".into(), 850.0), ("2026-10-22".into(), 895.0)],
        )
        .unwrap();
    let (_, ship) = get(&e.app, "/api/research/SHIP/prices?range=1y").await;
    let d = ship["drivers"].as_array().unwrap();
    assert_eq!(d[0]["id"], "charter_index");
    assert_eq!(d[0]["status"], "TBD");
    assert_eq!(d[0]["series"], json!([]));
    assert_eq!(d[0]["proxy"]["id"], "bwet");
    assert_eq!(d[0]["proxy"]["proxy"], true);
    assert_eq!(d[0]["proxy"]["series"].as_array().unwrap().len(), 2);
    assert_eq!(d[1]["id"], "bunker");
    assert_eq!(d[1]["proxy"]["id"], "brent");
    assert_eq!(ship["drivers_plotted"], 1);
    let (_, none) = get(&e.app, "/api/research/NONE/prices?range=1y").await;
    assert_eq!(none["drivers"], json!([]), "no pack, no drivers");
    assert!(none["pack"].is_null());
}
