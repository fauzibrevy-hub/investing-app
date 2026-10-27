//! Source adapters and scheduler against local mocks: Yahoo (200 / 500 / bad JSON / redirect /
//! 429), a temporary scraper SQLite, emitens.csv, portfolio failures, macro context, router
//! mount and health (plan §6.3; M1 #3, #6). No live network.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{TimeZone, Utc};
use common::test_app;
use investing_app::research::prices::yahoo::{self, parse_chart, YahooSource};
use investing_app::research::prices::PriceSource;
use investing_app::research::scheduler::{due_for_prices, last_slot, Clock, Scheduler};
use investing_app::research::sources::emitens;
use investing_app::research::sources::portfolio::HeldProvider;
use investing_app::research::sources::SourceError;
use investing_app::research::store::Store;
use investing_app::research::universe::HeldPosition;
use serde_json::{json, Value};

// ---- Yahoo mock ----

#[derive(Clone)]
enum Mode {
    Ok,
    /// Like `Ok` with a second, later split the store has not seen.
    OkNewSplit,
    Status(u16),
    BadJson,
    Redirect,
}

/// First bar of the fixed mock: 2016-10-24T03:00:00Z (09:00 WIB).
const FIXED_START: i64 = 1_477_278_000;

#[derive(Clone)]
struct Mock {
    hits: Arc<AtomicUsize>,
    ranges: Arc<Mutex<Vec<String>>>,
    script: Arc<Mutex<Vec<Mode>>>,
    bars: usize,
    /// Unix seconds of the first bar.
    start_ts: i64,
}

impl Mock {
    fn new(script: Vec<Mode>, bars: usize) -> Self {
        Self {
            hits: Arc::default(),
            ranges: Arc::default(),
            script: Arc::new(Mutex::new(script)),
            bars,
            start_ts: FIXED_START,
        }
    }

    /// Bars that end today, for tests of the gap-based top-up range (R3 review): a store
    /// whose last bar is years old rightly asks for a longer window.
    fn recent(script: Vec<Mode>, bars: usize) -> Self {
        let today_0300 = Utc::now().timestamp() / 86_400 * 86_400 + 3 * 3_600;
        Self {
            start_ts: today_0300 - (bars as i64 - 1) * 86_400,
            ..Self::new(script, bars)
        }
    }
}

/// `n` daily bars from `start` (09:00 WIB), one dividend and one 2:1 split.
fn chart_body_from(start: i64, n: usize, range: &str) -> Value {
    let n = if range == "10y" { n } else { n.min(60) };
    let ts: Vec<i64> = (0..n as i64).map(|i| start + i * 86_400).collect();
    let close: Vec<Value> = (0..n)
        .map(|i| {
            if i == 3 {
                Value::Null
            } else {
                json!(1000.0 + (i % 50) as f64)
            }
        })
        .collect();
    json!({"chart": {"result": [{
        "meta": {"gmtoffset": 25200, "currency": "IDR"},
        "timestamp": ts,
        "events": {
            "dividends": {"1": {"amount": 25.0, "date": ts[10]}},
            "splits": {"1": {"date": ts[20], "numerator": 2, "denominator": 1, "splitRatio": "2:1"}}
        },
        "indicators": {
            "quote": [{"open": close, "high": close, "low": close, "close": close, "volume": vec![100; n]}],
            "adjclose": [{"adjclose": close}]
        }
    }], "error": null}})
}

fn chart_body(n: usize, range: &str) -> Value {
    chart_body_from(FIXED_START, n, range)
}

async fn chart(
    State(m): State<Mock>,
    UrlPath(_sym): UrlPath<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    m.hits.fetch_add(1, Ordering::SeqCst);
    let range = q.get("range").cloned().unwrap_or_default();
    m.ranges.lock().unwrap().push(range.clone());
    let mode = {
        let mut s = m.script.lock().unwrap();
        if s.len() > 1 {
            s.remove(0)
        } else {
            s[0].clone()
        }
    };
    match mode {
        Mode::Ok => axum::Json(chart_body_from(m.start_ts, m.bars, &range)).into_response(),
        Mode::OkNewSplit => {
            let mut body = chart_body_from(m.start_ts, m.bars, &range);
            let ts = body["chart"]["result"][0]["timestamp"][30].clone();
            body["chart"]["result"][0]["events"]["splits"]["2"] =
                json!({"date": ts, "numerator": 5, "denominator": 1, "splitRatio": "5:1"});
            axum::Json(body).into_response()
        }
        Mode::Status(c) => StatusCode::from_u16(c).unwrap().into_response(),
        Mode::BadJson => (StatusCode::OK, "<html>nope</html>").into_response(),
        Mode::Redirect => (
            StatusCode::FOUND,
            [(header::LOCATION, "http://127.0.0.1:9/x")],
        )
            .into_response(),
    }
}

async fn serve(mock: Mock) -> String {
    let app = Router::new()
        .route("/v8/finance/chart/{sym}", get(chart))
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v8/finance/chart")
}

fn fast(base: &str) -> YahooSource {
    YahooSource::new(Some(base)).with_timing(Duration::from_millis(1), Duration::from_millis(1))
}

// ---- helpers ----

struct FakeHeld(Mutex<Result<Vec<HeldPosition>, SourceError>>);

impl FakeHeld {
    fn ok(tickers: &[&str]) -> Self {
        Self(Mutex::new(Ok(tickers
            .iter()
            .map(|t| HeldPosition {
                ticker: (*t).into(),
                avg_price: 1000.0,
            })
            .collect())))
    }
}

impl HeldProvider for FakeHeld {
    async fn held(&self) -> Result<Vec<HeldPosition>, SourceError> {
        self.0.lock().unwrap().clone()
    }
}

fn real_clock() -> Clock {
    Arc::new(Utc::now)
}

fn scheduler(dir: &Path, base: &str, held: FakeHeld) -> Scheduler<YahooSource, FakeHeld> {
    Scheduler::new(
        Arc::new(Store::open(&dir.join("research.sqlite")).unwrap()),
        fast(base),
        held,
        dir.join("XBRL"),
        dir.join("research/watchlist.csv"),
        real_clock(),
    )
}

// ---- parse ----

#[test]
fn parse_chart_reads_bars_events_and_local_dates() {
    let d = parse_chart(&chart_body(30, "10y")).unwrap();
    assert_eq!(d.bars.len(), 29, "the null close is dropped");
    assert_eq!(d.bars[0].date, "2016-10-24");
    assert_eq!(d.bars[0].volume, Some(100));
    assert_eq!(d.dividends, vec![("2016-11-03".to_string(), 25.0)]);
    assert_eq!(d.splits, vec![("2016-11-13".to_string(), 2.0, 1.0)]);
    let ev = d.events("AAAA");
    assert_eq!(ev.len(), 2);
}

#[test]
fn parse_chart_rejects_errors_and_empty_series() {
    assert!(parse_chart(
        &json!({"chart": {"result": null, "error": {"description": "Not Found"}}})
    )
    .is_err());
    assert!(parse_chart(&json!({"chart": {"result": []}})).is_err());
    let empty = json!({"chart": {"result": [{"meta": {}, "timestamp": [1], "indicators": {"quote": [{"close": [null]}]}}]}});
    assert!(parse_chart(&empty).is_err());
    assert!(parse_chart(&json!({})).is_err());
    assert_eq!(yahoo::symbol_for("AAAA"), "AAAA.JK");
    assert_eq!(yahoo::symbol_for("^JKSE"), "^JKSE");
}

// ---- M1 #3: new ticker, 10 years + events, ≤ 60 s ----

#[tokio::test]
async fn new_ticker_backfills_ten_years_with_events_within_a_minute() {
    let mock = Mock::recent(vec![Mode::Ok], 2_500);
    let base = serve(mock.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base, FakeHeld::ok(&["AAAA"]));

    let t = Instant::now();
    let report = s.tick().await;
    assert!(t.elapsed() < Duration::from_secs(60));
    assert_eq!(report.universe, 1);
    assert_eq!(report.prices_ok, ["AAAA", "^JKSE"]);
    assert!(report.prices_failed.is_empty());

    assert_eq!(s.store.price_count("AAAA").unwrap(), 2_499);
    assert_eq!(s.store.price_count("^JKSE").unwrap(), 2_499);
    let ranges = mock.ranges.lock().unwrap().clone();
    assert_eq!(ranges, ["10y", "10y"]);
    let (div, split): (i64, i64) = s
        .store
        .with_conn(|c| {
            Ok((
                c.query_row("SELECT COUNT(*) FROM corp_events WHERE kind='dividend' AND ticker='AAAA'", [], |r| r.get(0))?,
                c.query_row("SELECT COUNT(*) FROM corp_events WHERE kind='split' AND ratio_num=2 AND ratio_den=1", [], |r| r.get(0))?,
            ))
        })
        .unwrap();
    assert_eq!((div, split), (1, 2));

    // Already fetched after the latest slot: the next tick does nothing, a forced run tops up.
    let again = s.tick().await;
    assert!(again.prices_ok.is_empty());
    let (ok, _) = s.refresh_prices(&["AAAA".to_string()], true).await;
    assert_eq!(ok.len(), 2);
    assert_eq!(mock.ranges.lock().unwrap().last().unwrap(), "3mo");
}

// ---- M1 #6: failures keep old data ----

#[tokio::test]
async fn http_500_keeps_stored_data_and_records_a_failed_run() {
    let mock = Mock::new(vec![Mode::Ok], 200);
    let base = serve(mock.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base, FakeHeld::ok(&["AAAA"]));
    s.tick().await;
    let before = s.store.price_count("AAAA").unwrap();
    assert!(before > 100);

    *mock.script.lock().unwrap() = vec![Mode::Status(500)];
    mock.hits.store(0, Ordering::SeqCst);
    let (ok, failed) = s.refresh_prices(&["AAAA".to_string()], true).await;
    assert!(ok.is_empty());
    assert_eq!(failed, ["AAAA", "^JKSE"]);
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        8,
        "1 try + 3 retries, per symbol"
    );
    assert_eq!(s.store.price_count("AAAA").unwrap(), before);

    let runs = s.store.source_runs("yahoo").unwrap();
    let last = runs
        .iter()
        .rev()
        .find(|r| r.ticker.as_deref() == Some("AAAA"))
        .unwrap();
    assert!(!last.ok);
    assert!(last.error.as_deref().unwrap().contains("500"));
    // The last good run is still the freshness reference.
    assert!(s
        .store
        .last_ok_run("yahoo", Some("AAAA"))
        .unwrap()
        .is_some());
}

/// R3.1: a malformed body, a redirect and an exhausted 429 run each leave the stored bars
/// and events untouched and are recorded as failed runs (plan §7.2 integration row).
#[tokio::test]
async fn malformed_redirect_and_exhausted_rate_limit_keep_stored_data() {
    let mock = Mock::new(vec![Mode::Ok], 120);
    let base = serve(mock.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base, FakeHeld::ok(&["AAAA"]));
    s.tick().await;
    let bars = s.store.prices("AAAA").unwrap();
    let events = s.store.events("AAAA").unwrap();
    assert!(bars.len() > 100 && events.len() == 2);

    for (script, expected_hits) in [
        (vec![Mode::BadJson], 2),
        (vec![Mode::Redirect], 2),
        (vec![Mode::Status(429)], 8),
    ] {
        *mock.script.lock().unwrap() = script;
        mock.hits.store(0, Ordering::SeqCst);
        let (ok, failed) = s.refresh_prices(&["AAAA".to_string()], true).await;
        assert!(ok.is_empty());
        assert_eq!(failed, ["AAAA", "^JKSE"]);
        assert_eq!(mock.hits.load(Ordering::SeqCst), expected_hits);
        assert_eq!(s.store.prices("AAAA").unwrap(), bars);
        assert_eq!(s.store.events("AAAA").unwrap(), events);
        let last = s.store.latest_run("yahoo").unwrap().unwrap();
        assert!(!last.ok && last.error.is_some());
    }
    // The schema no longer carries an adjusted close: bars are open/high/low/close/volume.
    let cols: Vec<String> = s
        .store
        .with_conn(|c| {
            let mut st = c.prepare("PRAGMA table_info(prices_daily)")?;
            let v = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap();
    assert_eq!(
        cols,
        ["ticker", "date", "open", "high", "low", "close", "volume"]
    );
}

/// Yahoo closes are split-adjusted: a split the store has not seen makes the top-up a full
/// refetch, so every stored bar lands on the new basis (review R3).
#[tokio::test]
async fn a_new_split_triggers_a_full_refetch() {
    let mock = Mock::recent(vec![Mode::Ok], 120);
    let base = serve(mock.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base, FakeHeld::ok(&["AAAA"]));
    s.tick().await;
    mock.ranges.lock().unwrap().clear();
    let (ok, _) = s.refresh_prices(&["AAAA".to_string()], true).await;
    assert_eq!(ok.len(), 2);
    assert_eq!(
        mock.ranges.lock().unwrap().clone(),
        ["3mo", "3mo"],
        "known split: top-up only"
    );
    *mock.script.lock().unwrap() = vec![Mode::OkNewSplit];
    mock.ranges.lock().unwrap().clear();
    let (ok, _) = s.refresh_prices(&["AAAA".to_string()], true).await;
    assert_eq!(ok.len(), 2);
    assert_eq!(
        mock.ranges.lock().unwrap().clone(),
        ["3mo", "10y", "3mo", "10y"],
        "new split: the top-up is followed by the full history"
    );
    let splits: i64 = s
        .store
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM corp_events WHERE kind='split' AND ticker='AAAA'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(splits, 2);
    assert_eq!(yahoo::range_for_gap(None), "10y");
    assert_eq!(yahoo::range_for_gap(Some(3)), "3mo");
    assert_eq!(yahoo::range_for_gap(Some(200)), "1y");
    assert_eq!(yahoo::range_for_gap(Some(1_000)), "5y");
    assert_eq!(yahoo::range_for_gap(Some(4_000)), "10y");
}

#[tokio::test]
async fn bad_json_and_redirects_fail_without_retrying() {
    for mode in [Mode::BadJson, Mode::Redirect, Mode::Status(404)] {
        let mock = Mock::new(vec![mode], 10);
        let base = serve(mock.clone()).await;
        let err = fast(&base).fetch("AAAA.JK", "3mo").await.unwrap_err();
        assert!(!err.0.is_empty());
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1, "{}", err.0);
    }
}

#[tokio::test]
async fn rate_limit_is_retried_and_requests_are_paced() {
    let mock = Mock::new(vec![Mode::Status(429), Mode::Status(503), Mode::Ok], 40);
    let base = serve(mock.clone()).await;
    let src = YahooSource::new(Some(&base))
        .with_timing(Duration::from_millis(40), Duration::from_millis(1));
    let t = Instant::now();
    let data = src.fetch("AAAA.JK", "3mo").await.unwrap();
    assert!(!data.bars.is_empty());
    assert_eq!(mock.hits.load(Ordering::SeqCst), 3);
    assert!(
        t.elapsed() >= Duration::from_millis(80),
        "3 requests, 40 ms apart"
    );
}

// ---- xbrl ----

// ---- emitens ----

#[test]
fn emitens_names_with_quotes_and_gaps() {
    let csv = "ticker,name,sector,industry,taxonomy_type,is_active,created_at\n\
               AAAA,\"PT Alfa, Tbk\",Financial Services,Banks,banking,true,x\n\
               BBBB,PT Beta,Energy,Coal,general,true,x\n\
               CCCC,,Financial Services,Insurance,insurance,true,x\n";
    let map = emitens::parse(csv);
    assert_eq!(map["AAAA"], "PT Alfa, Tbk");
    assert_eq!(map["BBBB"], "PT Beta");
    assert!(!map.contains_key("CCCC"), "an empty name is no name");
    assert!(emitens::parse("").is_empty());
    assert!(emitens::parse("a,b\n1,2\n").is_empty(), "no ticker column");
    assert!(emitens::load(Path::new("/nonexistent/emitens.csv")).is_err());
}

// ---- scheduler ----

#[test]
fn prices_are_due_once_per_trading_day_after_1630_wib() {
    // 2026-10-22 is a Thursday. 16:30 WIB = 09:30 UTC.
    let at = |d: u32, h: u32, m: u32| Utc.with_ymd_and_hms(2026, 10, d, h, m, 0).unwrap();
    assert_eq!(last_slot(at(22, 10, 0)), at(22, 9, 30));
    assert_eq!(
        last_slot(at(22, 9, 0)),
        at(21, 9, 30),
        "before close → yesterday"
    );
    assert_eq!(last_slot(at(24, 12, 0)), at(23, 9, 30), "Saturday → Friday");
    assert_eq!(
        last_slot(at(26, 1, 0)),
        at(23, 9, 30),
        "Monday morning → Friday"
    );

    assert!(due_for_prices(None, at(22, 10, 0)));
    assert!(due_for_prices(Some(at(21, 12, 0)), at(22, 10, 0)));
    assert!(!due_for_prices(Some(at(22, 9, 45)), at(22, 12, 0)));
    assert!(
        !due_for_prices(Some(at(23, 10, 0)), at(25, 12, 0)),
        "weekend: Friday fetch is enough"
    );
}

#[tokio::test]
async fn universe_sync_writes_the_watchlist_and_survives_a_portfolio_failure() {
    let base = serve(Mock::new(vec![Mode::Ok], 10)).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base, FakeHeld::ok(&["CCCC", "AAAA"]));
    investing_app::research::universe::add(
        &s.store,
        &investing_app::research::packs::Packs::embedded().set(),
        &investing_app::research::universe::AddRequest {
            ticker: "BBBB".into(),
            ..Default::default()
        },
        "2026-10-22T00:00:00Z",
    )
    .unwrap();

    let entries = s.sync_universe().await.unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("research/watchlist.csv")).unwrap(),
        "ticker\nAAAA\nBBBB\nCCCC\n"
    );

    *s.held.0.lock().unwrap() = Err(SourceError("portfolio sheet is empty".into()));
    assert!(s.sync_universe().await.is_err());
    assert_eq!(
        s.store.universe().unwrap().len(),
        3,
        "stored universe untouched"
    );
    assert!(
        std::fs::read_to_string(dir.path().join("research/watchlist.csv"))
            .unwrap()
            .contains("CCCC")
    );
    let run = s.store.latest_run("portfolio").unwrap().unwrap();
    assert!(!run.ok);
}

// ---- macro, router, health ----

#[tokio::test]
async fn router_is_mounted_behind_auth_and_health_lists_research_sources() {
    let t = test_app();
    let res = t
        .send(
            axum::http::Request::get("/api/research/universe")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let seed = Store::open(&t.cfg.research_db_path).unwrap();
    investing_app::research::universe::sync(
        &seed,
        &[HeldPosition {
            ticker: "AAAA".into(),
            avg_price: 1.0,
        }],
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    let (status, body) = t.get("/api/research/universe").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["ticker"], "AAAA");
    assert_eq!(body["items"][0]["role"], "held");

    let (_, health) = t.get("/api/health?deep=1").await;
    for k in [
        "research_prices",
        "research_xbrl",
        "research_emitens",
        "research_store",
        "research_scraper_job",
    ] {
        assert!(health["sources"][k]["ok"].is_boolean(), "{k}");
    }
    assert_eq!(health["sources"]["research_store"]["ok"], true);
    assert_eq!(
        health["sources"]["research_prices"]["ok"], false,
        "never fetched"
    );
}
