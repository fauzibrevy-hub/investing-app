//! Area 5 against MC goldens (tests/fixtures/macro/golden, produced by
//! scripts/golden/macro_golden.py from MC's own ingestion + scoring) and a
//! local mock that replays tests/fixtures/macro/upstream.json.

mod common;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use axum::Router;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use common::{json_of, test_app_with};
use investing_app::config::Config;
use investing_app::macro_watch::observe::{self, Store};
use investing_app::macro_watch::upstream::Endpoints;
use investing_app::macro_watch::{router_with, scoring, MacroState};
use futures::StreamExt;
use serde_json::{json, Value};
use tower::ServiceExt;

fn fixture(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/macro")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 20, 8, 0, 0).unwrap() + chrono::Duration::microseconds(123456)
}

const HOSTS: [(&str, &str); 5] = [
    ("/graph/", "https://fred.stlouisfed.org"),
    ("/v8/", "https://query1.finance.yahoo.com"),
    ("/v6/", "https://open.er-api.com"),
    ("/biweb/", "https://www.bi.go.id"),
    ("/sj/", "https://www.stats.gov.cn"),
];

/// Same normalization as scripts/parity.sh plus mock-origin → real-host rewriting.
fn normalize(v: &Value, origin: &str) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), normalize(v, origin)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| normalize(v, origin)).collect()),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
            _ => v.clone(),
        },
        Value::String(s) if !origin.is_empty() => {
            let mut s = s.clone();
            for (prefix, host) in HOSTS {
                s = s.replace(&format!("{origin}{prefix}"), &format!("{host}{prefix}"));
            }
            json!(s)
        }
        other => other.clone(),
    }
}

fn assert_parity(actual: &Value, expected: &Value, origin: &str, what: &str) {
    let (a, e) = (normalize(actual, origin), normalize(expected, ""));
    if a != e {
        panic!(
            "{what} differs from MC golden\n--- expected\n{}\n--- actual\n{}",
            serde_json::to_string_pretty(&e).unwrap(),
            serde_json::to_string_pretty(&a).unwrap()
        );
    }
}

async fn replay(
    State(manifest): State<Arc<Value>>,
    OriginalUri(uri): OriginalUri,
) -> Response<Body> {
    let pq = uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let spec = manifest.as_object().unwrap().iter().find(|(url, _)| {
        url.ends_with(&pq) && url[..url.len() - pq.len()].matches('/').count() == 2
    });
    let Some((_, spec)) = spec else {
        return Response::builder().status(599).body(Body::empty()).unwrap();
    };
    let mut res = Response::builder().status(spec["status"].as_u64().unwrap() as u16);
    for (k, v) in spec["headers"].as_object().unwrap() {
        res = res.header(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v.as_str().unwrap()).unwrap(),
        );
    }
    let mut res = res
        .body(Body::from(spec["body"].as_str().unwrap().to_string()))
        .unwrap();
    if let Some(reason) = spec["reason"].as_str() {
        res.extensions_mut()
            .insert(hyper::ext::ReasonPhrase::try_from(reason.as_bytes()).unwrap());
    }
    res
}

struct Upstream {
    origin: String,
    endpoints: Endpoints,
}

async fn start_upstream() -> Upstream {
    let app = Router::new()
        .fallback(replay)
        .with_state(Arc::new(fixture("upstream.json")));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    // A port nothing listens on, for FRED isolation (MC: 127.0.0.1:54321).
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let endpoints = Endpoints {
        fred: format!("{origin}/graph/fredgraph.csv"),
        yahoo: format!("{origin}/v8/finance/chart"),
        er_api: format!("{origin}/v6/latest/USD"),
        bi_inflation: format!("{origin}/biweb/api/Inflasi/GetInflasi"),
        nbs_pmi: format!("{origin}/sj/zxfb/pmi"),
        fred_isolated: format!("http://{closed}/isolated_fred_unreachable"),
    };
    Upstream { origin, endpoints }
}

fn state(cfg: &Config, endpoints: Endpoints) -> Arc<MacroState> {
    Arc::new(MacroState::new(cfg, endpoints, Arc::new(now)))
}

fn store_fixture() -> Store {
    fixture("store.json").as_object().unwrap().clone()
}

#[tokio::test]
async fn ingestion_matches_mc() {
    let up = start_upstream().await;
    for disable_fred in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::for_tests(dir.path());
        std::fs::write(
            &cfg.macro_store_path,
            serde_json::to_string(&store_fixture()).unwrap(),
        )
        .unwrap();
        let st = state(&cfg, up.endpoints.clone());
        let changed = st.ingest(disable_fred).await;
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg.macro_store_path).unwrap()).unwrap();
        let golden = fixture(&format!("golden/ingest_disable_fred_{disable_fred}.json"));
        assert_parity(
            &saved,
            &golden["store"],
            &up.origin,
            &format!("store after ingest (disable_fred={disable_fred})"),
        );
        assert_eq!(json!(changed), golden["changed"]);
    }
}

#[test]
fn observations_and_scoring_match_mc() {
    let store = store_fixture();
    let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    for iso in [false, true] {
        assert_parity(
            &json!(observe::observations(today, iso, &store)),
            &fixture(&format!("golden/observations_isolated_{iso}.json")),
            "",
            "observations",
        );
    }
    let obs = observe::observations(today, false, &store);
    let cases: [(&str, &[&str]); 5] = [
        ("default", &[]),
        ("vix_cut", &["vix"]),
        ("hy_cut", &["us_hy_oas"]),
        ("brent_curve_cut", &["brent_curve"]),
        ("two_cuts", &["ust_10y", "dxy"]),
    ];
    let vintage = "2026-10-20T08:00:00.123456+00:00";
    for (name, disabled) in cases {
        let disabled: BTreeSet<String> = disabled.iter().map(|s| s.to_string()).collect();
        assert_parity(
            &scoring::calculate(&obs, &disabled, vintage),
            &fixture(&format!("golden/scoring_{name}.json")),
            "",
            name,
        );
    }
    let isolated = observe::observations(today, true, &store);
    assert_parity(
        &scoring::calculate(&isolated, &BTreeSet::new(), vintage),
        &fixture("golden/scoring_isolated.json"),
        "",
        "isolated",
    );
    let empty = observe::observations(today, false, &Store::new());
    assert_parity(
        &scoring::calculate(&empty, &BTreeSet::new(), vintage),
        &fixture("golden/scoring_empty_store.json"),
        "",
        "empty store",
    );
}

#[tokio::test]
async fn endpoints_shape_and_flags() {
    let t = test_app_with(|cfg| {
        std::fs::write(
            &cfg.macro_store_path,
            serde_json::to_string(&store_fixture()).unwrap(),
        )
        .unwrap()
    });
    let (s, reg) = t.get("/api/macro/registry").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (reg["status"].as_str(), reg["total_metrics"].as_u64()),
        (Some("ok"), Some(15))
    );
    assert_eq!(reg["registry"][5]["unit"], "bps");

    let (_, obs) = t.get("/api/macro/observations").await;
    assert_eq!(
        (
            obs["fred_isolated"].as_bool(),
            obs["sandbox_empty_store"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert_eq!(obs["observations"].as_array().unwrap().len(), 15);
    assert!(obs["as_of"].as_str().unwrap().ends_with("+00:00"));

    let (_, iso) = t.get("/api/macro/observations?disable_fred=true").await;
    assert_eq!(iso["fred_isolated"], true);
    assert_eq!(iso["observations"][3]["upstream_ok"], false);

    let req = Request::get("/api/macro/scoring?disable_metrics=brent_curve,%20dxy")
        .header("authorization", t.bearer())
        .header("x-macro-disable-vix", "cut")
        .header("x-macro-sandbox-empty-store", "sandbox")
        .body(Body::empty())
        .unwrap();
    let (s, sc) = json_of(t.send(req).await).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(sc["sandbox_empty_store"], true);
    assert_eq!(sc["regime_final"], "insufficient evidence");
    assert_eq!(
        sc["blocking_feeds"],
        json!(["brent_curve", "dxy", "us_hy_oas", "vix"])
    );
    assert_eq!(sc["scoring_configuration_version"], "2026.09-scoring-final");

    let (s, body) = t.get("/api/macro/observations?disable_fred=perhaps").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["type"], "bool_parsing");
}

#[tokio::test]
async fn trigger_updates_store_and_streams_events() {
    let up = start_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::for_tests(dir.path());
    let app = router_with(state(&cfg, up.endpoints.clone()));

    let res = app
        .clone()
        .oneshot(
            Request::get("/api/macro/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let mut body = res.into_body().into_data_stream();
    let first = String::from_utf8(body.next().await.unwrap().unwrap().to_vec()).unwrap();
    assert!(first.starts_with("event: init\ndata: "), "{first}");
    let init: Value =
        serde_json::from_str(first.trim_start_matches("event: init\ndata: ").trim()).unwrap();
    assert_eq!(init["total_metrics"], 15);
    assert_eq!(
        init["observations"][0]["freshness"], "unavailable",
        "empty store before first ingest"
    );

    let res = app
        .clone()
        .oneshot(
            Request::post("/api/macro/trigger")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (s, v) = json_of(res).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        v["message"],
        "Macro release check executed, store updated, and broadcasted to active SSE listeners"
    );
    assert!(cfg.macro_store_path.exists());

    let mut seen = String::new();
    while !seen.contains("release_cycle_executed") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), body.next())
            .await
            .expect("event within 5 s")
            .unwrap()
            .unwrap();
        seen.push_str(std::str::from_utf8(&chunk).unwrap());
    }
    assert!(seen.contains("event: update\ndata: "));
    assert!(
        seen.contains(r#""metric_id":"brent_front_month""#),
        "per-metric update for a changed series"
    );

    let (_, obs) = json_of(
        app.oneshot(
            Request::get("/api/macro/observations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    let vix = obs["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["metric_id"] == "vix")
        .unwrap()
        .clone();
    assert_eq!(
        (vix["value"].as_f64(), vix["freshness"].as_str()),
        (Some(18.12), Some("fresh"))
    );
}
