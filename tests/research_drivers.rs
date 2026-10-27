//! Driver series adapters (research plan R3.3, spike-r3-drivers.md; prd.md A3.5) against a
//! local mock of the three source families (Yahoo chart v8, SDMX CSV, FRED CSV): every
//! sourced catalogue driver is fetched and stored, failures leave the stored series alone,
//! the scheduler refreshes once per trading day, the pack payloads list drivers with a
//! status word and labelled proxies, and health reports `research_drivers`. No live network.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{TimeZone, Utc};
use common::test_app_with;
use finance_app::research::packs::Pack;
use finance_app::research::prices::drivers::{self, DriverFetcher, Family, HttpDrivers};
use finance_app::research::prices::yahoo::YahooSource;
use finance_app::research::scheduler::{Clock, Scheduler};
use finance_app::research::sources::portfolio::HeldProvider;
use finance_app::research::sources::SourceError;
use finance_app::research::store::Store;
use finance_app::research::universe::{add, AddRequest, HeldPosition, Role};
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Ok,
    Status(u16),
    BadBody,
}

#[derive(Clone)]
struct Mock {
    hits: Arc<AtomicUsize>,
    mode: Arc<Mutex<Mode>>,
    urls: Arc<Mutex<Vec<String>>>,
    /// `(request path, User-Agent)` of every request.
    agents: Arc<Mutex<Vec<(String, String)>>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            hits: Arc::default(),
            mode: Arc::new(Mutex::new(Mode::Ok)),
            urls: Arc::default(),
            agents: Arc::default(),
        }
    }

    fn set(&self, m: Mode) {
        *self.mode.lock().unwrap() = m;
        self.hits.store(0, Ordering::SeqCst);
    }
}

/// 60 daily closes from 2016-10-24 09:00 WIB.
fn chart_body(sym: &str) -> Value {
    let start = 1_477_278_000i64;
    let ts: Vec<i64> = (0..60).map(|i| start + i * 86_400).collect();
    let base = if sym.starts_with("IDR") {
        13_000.0
    } else {
        50.0
    };
    let close: Vec<Value> = (0..60).map(|i| json!(base + i as f64)).collect();
    json!({"chart": {"result": [{
        "meta": {"gmtoffset": 25200, "currency": "USD"},
        "timestamp": ts,
        "indicators": {"quote": [{"open": close, "high": close, "low": close, "close": close, "volume": vec![1; 60]}]}
    }], "error": null}})
}

fn gate(m: &Mock, uri: String) -> Option<Response> {
    m.hits.fetch_add(1, Ordering::SeqCst);
    m.urls.lock().unwrap().push(uri);
    match *m.mode.lock().unwrap() {
        Mode::Ok => None,
        Mode::Status(c) => Some(StatusCode::from_u16(c).unwrap().into_response()),
        Mode::BadBody => Some((StatusCode::OK, "<html>maintenance</html>").into_response()),
    }
}

async fn chart(State(m): State<Mock>, UrlPath(sym): UrlPath<String>) -> Response {
    gate(&m, format!("chart/{sym}")).unwrap_or_else(|| axum::Json(chart_body(&sym)).into_response())
}

async fn bis(
    State(m): State<Mock>,
    UrlPath((flow, key)): UrlPath<(String, String)>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    gate(&m, format!("bis/{flow}/{key}?{:?}", q.get("startPeriod"))).unwrap_or_else(|| {
        "FREQ,REF_AREA,TIME_PERIOD,OBS_VALUE\nM,ID,2026-07,5.75\nM,ID,2026-08,NaN\nM,ID,2026-09,5.5\n"
            .into_response()
    })
}

async fn oecd(State(m): State<Mock>, UrlPath((flow, key)): UrlPath<(String, String)>) -> Response {
    gate(&m, format!("oecd/{flow}/{key}")).unwrap_or_else(|| {
        "STRUCTURE,STRUCTURE_ID,REF_AREA,\"Reference area\",FREQ,TIME_PERIOD,OBS_VALUE,BASE_PER\n\
         DATAFLOW,X,IDN,\"Indonesia, Republic of\",M,2026-08,137.7939,2015\n\
         DATAFLOW,X,IDN,\"Indonesia, Republic of\",M,2026-09,138.2123,2015\n"
            .into_response()
    })
}

async fn fred(
    State(m): State<Mock>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let id = q.get("id").cloned().unwrap_or_default();
    gate(&m, format!("fred?id={id}&cosd={:?}", q.get("cosd"))).unwrap_or_else(|| {
        format!("observation_date,{id}\n2026-07-01,.\n2026-08-01,140.51\n2026-09-01,153.84\n")
            .into_response()
    })
}

async fn record_agent(
    State(m): State<Mock>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    m.agents
        .lock()
        .unwrap()
        .push((req.uri().path().to_string(), ua));
    next.run(req).await
}

async fn serve(mock: Mock) -> String {
    let app = Router::new()
        .route("/v8/finance/chart/{sym}", get(chart))
        .route("/bis/{flow}/{key}/all", get(bis))
        .route("/oecd/{flow}/{key}", get(oecd))
        .route("/fred.csv", get(fred))
        .layer(axum::middleware::from_fn_with_state(
            mock.clone(),
            record_agent,
        ))
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn fetcher(base: &str) -> HttpDrivers {
    let yahoo = YahooSource::new(Some(&format!("{base}/v8/finance/chart")))
        .with_timing(Duration::from_millis(1), Duration::from_millis(1));
    HttpDrivers::with_bases(
        yahoo,
        &format!("{base}/bis"),
        &format!("{base}/oecd"),
        &format!("{base}/fred.csv"),
    )
    .with_retry_pause(Duration::from_millis(1))
}

struct NoHeld;

impl HeldProvider for NoHeld {
    async fn held(&self) -> Result<Vec<HeldPosition>, SourceError> {
        Ok(Vec::new())
    }
}

fn clock() -> Clock {
    Arc::new(|| Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap())
}

/// Prices go to the same mock (fast timing) so the `^JKSE` pass does not back off for
/// seconds against a dead address.
fn scheduler(dir: &Path, base: &str) -> Scheduler<YahooSource, NoHeld> {
    Scheduler::new(
        Arc::new(Store::open(&dir.join("research.sqlite")).unwrap()),
        YahooSource::new(Some(&format!("{base}/v8/finance/chart")))
            .with_timing(Duration::from_millis(1), Duration::from_millis(1)),
        NoHeld,
        dir.join("XBRL"),
        dir.join("research/watchlist.csv"),
        clock(),
    )
    .with_drivers(Arc::new(fetcher(base)))
}

#[tokio::test]
async fn every_sourced_driver_is_fetched_stored_and_recorded() {
    let mock = Mock::new();
    let base = serve(mock.clone()).await;
    let f = fetcher(&base);
    let store = Store::open_memory().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap();
    for spec in drivers::sourced() {
        let n = drivers::ingest(&store, &f, spec, now).await.unwrap();
        assert!(n > 0, "{}", spec.id);
    }
    let summaries = store.driver_summaries().unwrap();
    let ids: Vec<&str> = summaries.iter().map(|s| s.series.as_str()).collect();
    assert_eq!(
        ids,
        [
            "bi_rate",
            "brent",
            "bwet",
            "cpi",
            "gold",
            "newcastle",
            "usd_idr"
        ]
    );
    // Monthly observations are dated on the first of their month; NaN and `.` skipped.
    let bi = store.driver_series("bi_rate", None).unwrap();
    assert_eq!(
        bi,
        vec![
            ("2026-07-01".to_string(), 5.75),
            ("2026-09-01".to_string(), 5.5)
        ]
    );
    let coal = store.driver_series("newcastle", None).unwrap();
    assert_eq!(coal.len(), 2);
    assert_eq!(coal[1], ("2026-09-01".to_string(), 153.84));
    let cpi = store.driver_series("cpi", Some("2026-09-01")).unwrap();
    assert_eq!(cpi, vec![("2026-09-01".to_string(), 138.2123)]);
    assert_eq!(store.driver_series("usd_idr", None).unwrap().len(), 60);
    assert_eq!(summaries[0].source, "BIS SDMX WS_CBPOL M.ID");
    // A backfill asks for ten years; the next run asks from a month before the last row.
    let urls = mock.urls.lock().unwrap().clone();
    assert!(
        urls.iter()
            .any(|u| u.starts_with("bis/WS_CBPOL/M.ID?Some(\"2016-10\")")),
        "{urls:?}"
    );
    // The backfill start is ten years before the wall-clock date.
    let ten_years_ago = drivers::ten_years_before(Utc::now().date_naive());
    assert!(
        urls.iter()
            .any(|u| *u == format!("fred?id=PCOALAUUSDM&cosd=Some(\"{ten_years_ago}\")")),
        "{urls:?}"
    );
    drivers::ingest(&store, &f, drivers::spec("newcastle").unwrap(), now)
        .await
        .unwrap();
    let last = mock.urls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last, "fred?id=PCOALAUUSDM&cosd=Some(\"2026-08-01\")");
    // One source_runs row per driver, keyed by id.
    let runs = store.source_runs(drivers::SOURCE).unwrap();
    assert_eq!(runs.len(), 8);
    assert!(runs.iter().all(|r| r.ok));
    assert_eq!(runs[0].ticker.as_deref(), Some("bi_rate"));
}

#[tokio::test]
async fn only_fred_sends_the_curl_user_agent() {
    let mock = Mock::new();
    let base = serve(mock.clone()).await;
    let f = fetcher(&base);
    let store = Store::open_memory().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap();
    for id in ["newcastle", "bi_rate", "cpi"] {
        drivers::ingest(&store, &f, drivers::spec(id).unwrap(), now)
            .await
            .unwrap();
    }
    let agents = mock.agents.lock().unwrap().clone();
    let of = |prefix: &str| -> Vec<String> {
        agents
            .iter()
            .filter(|(p, _)| p.starts_with(prefix))
            .map(|(_, ua)| ua.clone())
            .collect()
    };
    assert_eq!(of("/fred"), [drivers::FRED_UA]);
    assert_eq!(drivers::FRED_UA, "curl/8.7.1");
    assert_eq!(of("/bis"), [drivers::HTTP_UA]);
    assert_eq!(of("/oecd"), [drivers::HTTP_UA]);
    assert_eq!(drivers::HTTP_UA, "Mozilla/5.0");
}

/// One real request through the production fetcher: `cargo test --test research_drivers
/// live_fred -- --ignored`. Proves the FRED client's user agent is accepted by FRED's edge.
#[tokio::test]
#[ignore = "network"]
async fn live_fred_serves_the_production_client() {
    let spec = drivers::spec("newcastle").unwrap();
    let since = chrono::NaiveDate::from_ymd_opt(2026, 1, 1);
    let obs = HttpDrivers::new().fetch(spec, since).await.unwrap();
    assert!(!obs.is_empty());
}

#[tokio::test]
async fn failures_keep_the_stored_series_and_tbd_drivers_are_never_fetched() {
    let mock = Mock::new();
    let base = serve(mock.clone()).await;
    let f = fetcher(&base);
    let store = Store::open_memory().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap();
    let coal = drivers::spec("newcastle").unwrap();
    let bi = drivers::spec("bi_rate").unwrap();
    drivers::ingest(&store, &f, coal, now).await.unwrap();
    drivers::ingest(&store, &f, bi, now).await.unwrap();
    let before = (
        store.driver_series("newcastle", None).unwrap(),
        store.driver_series("bi_rate", None).unwrap(),
    );
    for mode in [Mode::Status(500), Mode::Status(404), Mode::BadBody] {
        mock.set(mode);
        assert!(drivers::ingest(&store, &f, coal, now).await.is_err());
        assert!(drivers::ingest(&store, &f, bi, now).await.is_err());
        let after = (
            store.driver_series("newcastle", None).unwrap(),
            store.driver_series("bi_rate", None).unwrap(),
        );
        assert_eq!(after, before);
    }
    // A 5xx and a 429 are retried (three tries); a 404 and a bad body are not.
    mock.set(Mode::Status(500));
    drivers::ingest(&store, &f, coal, now).await.unwrap_err();
    assert_eq!(mock.hits.load(Ordering::SeqCst), 3);
    mock.set(Mode::Status(429));
    drivers::ingest(&store, &f, bi, now).await.unwrap_err();
    assert_eq!(mock.hits.load(Ordering::SeqCst), 3);
    mock.set(Mode::Status(404));
    drivers::ingest(&store, &f, coal, now).await.unwrap_err();
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
    mock.set(Mode::BadBody);
    drivers::ingest(&store, &f, coal, now).await.unwrap_err();
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
    let runs = store.source_runs(drivers::SOURCE).unwrap();
    let failed = runs.iter().filter(|r| !r.ok).count();
    assert_eq!(failed, 10);
    assert!(runs
        .last()
        .unwrap()
        .error
        .as_deref()
        .unwrap()
        .contains("fred"));

    // No source: an error without a request.
    mock.set(Mode::Ok);
    let hba = drivers::spec("hba").unwrap();
    assert_eq!(hba.family, Family::None);
    let err = f.fetch(hba, None).await.unwrap_err();
    assert!(err.0.contains("TBD"));
    assert_eq!(mock.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_scheduler_refreshes_drivers_once_per_trading_day_after_prices() {
    let mock = Mock::new();
    let base = serve(mock.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path(), &base);
    let report = s.tick().await;
    assert_eq!(report.drivers_ok.len(), 7, "{:?}", report.drivers_failed);
    assert!(report.drivers_failed.is_empty());
    assert_eq!(s.store.driver_summaries().unwrap().len(), 7);
    // Already fetched after the latest slot: nothing is due; a forced refresh re-fetches.
    let again = s.tick().await;
    assert!(again.drivers_ok.is_empty() && again.drivers_failed.is_empty());
    let (ok, failed) = s.refresh_drivers(true).await;
    assert_eq!((ok.len(), failed.len()), (7, 0));
    // A failing source is reported per driver and leaves the rest intact.
    mock.set(Mode::Status(503));
    let (ok, failed) = s.refresh_drivers(true).await;
    assert!(ok.is_empty());
    assert_eq!(failed.len(), 7);
    assert_eq!(s.store.driver_summaries().unwrap().len(), 7);
    // Without a fetcher the pass is skipped, not failed.
    let bare = Scheduler::new(
        s.store.clone(),
        YahooSource::new(Some(&format!("{base}/v8/finance/chart")))
            .with_timing(Duration::from_millis(1), Duration::from_millis(1)),
        NoHeld,
        dir.path().join("XBRL"),
        dir.path().join("research/watchlist.csv"),
        clock(),
    );
    let (ok, failed) = bare.refresh_drivers(true).await;
    assert!(ok.is_empty() && failed.is_empty());
}

#[tokio::test]
async fn pack_payloads_list_every_driver_with_a_status_word_and_labelled_proxies() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        let packs = finance_app::research::packs::Packs::embedded().set();
        for (ticker, pack) in [
            ("BANK", "bank_sharia"),
            ("SHIP", "shipping"),
            ("COAL", "coal_mining"),
        ] {
            add(
                &store,
                &packs,
                &AddRequest {
                    ticker: ticker.into(),
                    role: Some(Role::Manual),
                    pack_override: Some(pack.into()),
                    ..Default::default()
                },
                "2026-10-23T00:00:00+00:00",
            )
            .unwrap();
        }
        store
            .upsert_drivers(
                "bi_rate",
                "BIS SDMX WS_CBPOL M.ID",
                "2026-10-23T09:00:00+00:00",
                &[("2026-08-01".into(), 5.75), ("2026-09-01".into(), 5.5)],
            )
            .unwrap();
        store
            .upsert_drivers(
                "bwet",
                "Yahoo BWET",
                "2026-10-23T09:00:00+00:00",
                &[("2026-10-22".into(), 895.91)],
            )
            .unwrap();
    }
    let t = test_app_with(|cfg| cfg.research_db_path = db.clone());

    let (s, bank) = t.get("/api/research/BANK/pack").await;
    assert_eq!(s, StatusCode::OK, "{bank}");
    let d = bank["drivers"].as_array().unwrap();
    assert_eq!(d[0]["id"], "bi_rate");
    assert_eq!(d[0]["status"], "tersedia");
    assert_eq!(d[0]["series"]["points"], 2);
    assert_eq!(d[0]["series"]["last"], "2026-09-01");
    assert_eq!(d[0]["cadence"], "bulanan");
    assert_eq!(d[1]["id"], "sbn_10y");
    assert_eq!(d[1]["status"], "TBD");
    assert!(d[1]["series"].is_null());
    assert_eq!(bank["missing_drivers"][0]["id"], "sbn_10y");

    let (_, ship) = t.get("/api/research/SHIP/pack").await;
    let d = ship["drivers"].as_array().unwrap();
    assert_eq!(d[0]["id"], "charter_index");
    assert_eq!(d[0]["status"], "TBD");
    assert_eq!(d[0]["proxy"]["id"], "bwet");
    assert_eq!(d[0]["proxy"]["proxy"], true);
    assert_eq!(d[0]["proxy"]["status"], "tersedia");
    assert_eq!(d[1]["id"], "bunker");
    assert_eq!(d[1]["proxy"]["id"], "brent");
    assert_eq!(d[1]["proxy"]["status"], "belum diambil");
    let missing: Vec<&str> = ship["missing_drivers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        missing,
        ["charter_index", "bunker"],
        "listed, never omitted"
    );
    assert_eq!(ship["missing_drivers"][0]["proxy"], "bwet");

    let (_, coal) = t.get("/api/research/COAL/pack").await;
    let d = coal["drivers"].as_array().unwrap();
    assert_eq!(d[0]["id"], "newcastle");
    assert_eq!(d[0]["status"], "belum diambil");
    assert_eq!(d[1]["id"], "hba");
    assert_eq!(d[1]["status"], "TBD");
    assert_eq!(d[1]["proxy"]["id"], "newcastle");

    // The Packs screen (design.md §4.4): definitions carry the same status words, and the
    // whole catalogue is listed, TBD entries included.
    let (_, list) = t.get("/api/research/packs").await;
    let bank_def = list["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "bank_sharia")
        .unwrap();
    assert_eq!(bank_def["drivers"][0]["status"], "tersedia");
    assert_eq!(bank_def["drivers"][1]["status"], "TBD");
    let cat = list["driver_catalogue"].as_array().unwrap();
    assert_eq!(cat.len(), drivers::CATALOGUE.len());
    let tbd: Vec<&str> = cat
        .iter()
        .filter(|d| d["status"] == "TBD")
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(tbd, ["sbn_10y", "hba"]);
    assert_eq!(
        cat.iter().find(|d| d["id"] == "brent").unwrap()["proxy_for"],
        "bunker"
    );

    // Health: the entry exists, says what is missing, and lists the TBD drivers.
    let (_, health) = t.get("/api/health?deep=1").await;
    let h = &health["sources"]["research_drivers"];
    assert_eq!(h["ok"], false, "{h}");
    assert_eq!(h["stored"], 2);
    let missing: Vec<&str> = h["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert_eq!(missing, ["newcastle", "usd_idr", "gold", "cpi", "brent"]);
    assert_eq!(h["tbd"], json!(["sbn_10y", "hba"]));
    assert_eq!(
        h["stale"],
        json!(["bi_rate", "bwet"]),
        "stored without a successful run"
    );
    assert!(h["detail"].as_str().unwrap().contains("never fetched"));
}

#[test]
fn a_pack_driver_with_a_source_or_a_proxy_must_exist_in_the_catalogue() {
    let base = include_str!("../research/packs/it_distribution.toml");
    assert!(Pack::parse(base).is_ok());
    let unknown_source = base.replace("id = \"usd_idr\"", "id = \"usd_eur\"");
    let err = Pack::parse(&unknown_source).unwrap_err().to_string();
    assert!(err.contains("no adapter in the driver catalogue"), "{err}");
    let tbd_unknown = unknown_source.replace("source = \"Yahoo IDR=X\"", "source = \"TBD\"");
    assert!(
        Pack::parse(&tbd_unknown).is_ok(),
        "a TBD driver needs no adapter"
    );
    let bad_proxy = base.replace(
        "source = \"Yahoo IDR=X\"",
        "source = \"TBD\"\nproxy = \"hba\"",
    );
    let err = Pack::parse(&bad_proxy).unwrap_err().to_string();
    assert!(err.contains("not a sourced catalogue series"), "{err}");
    let good_proxy = base.replace(
        "source = \"Yahoo IDR=X\"",
        "source = \"TBD\"\nproxy = \"brent\"",
    );
    let p = Pack::parse(&good_proxy).unwrap();
    assert_eq!(p.drivers[0].proxy.as_deref(), Some("brent"));
    assert!(p.missing_drivers().any(|d| d.id == "usd_idr"));
}
