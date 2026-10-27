//! Operating KPI entry with provenance (research plan §5 R2.3; prd.md A2.3):
//! `PUT/DELETE /api/research/{ticker}/kpi/{metric}/{fy}/{q}`, `unknown` distinct from not
//! entered, provenance returned, writes appended to `research-log.jsonl`, 401/404/422.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::{json_of, TestApp};
use investing_app::research::packs::Packs;
use investing_app::research::store::{Store, LOG_FILE};
use investing_app::research::universe::{sync, update, HeldPosition, Patch};
use serde_json::{json, Value};

/// A held coal miner (pack by override, since no filing is on disk) and a manual bank.
fn app() -> TestApp {
    let t = common::test_app();
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    sync(
        &store,
        &[HeldPosition {
            ticker: "COAL".into(),
            avg_price: 100.0,
        }],
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    let packs = Packs::embedded().set();
    update(
        &store,
        &packs,
        "COAL",
        &Patch {
            pack_override: Some(Some("coal_mining".into())),
            ..Default::default()
        },
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    investing_app::research::universe::add(
        &store,
        &packs,
        &investing_app::research::universe::AddRequest {
            ticker: "NOPK".into(),
            ..Default::default()
        },
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    t
}

async fn call(
    t: &TestApp,
    method: Method,
    uri: &str,
    key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", t.bearer())
        .header("idempotency-key", key);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    json_of(t.send(req.body(body).unwrap()).await).await
}

fn entry(value: f64) -> Value {
    json!({"value": value, "document": "Laporan operasional Q2 2026, hal. 4", "document_date": "2026-07-30"})
}

#[tokio::test]
async fn value_unknown_and_not_entered_are_three_different_states() {
    let t = app();
    let uri = "/api/research/COAL/kpi/production_mt/2026/2";
    let (s, v) = call(&t, Method::PUT, uri, "k1", Some(entry(7.25))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["state"], "entered");
    assert_eq!(v["value"], 7.25);
    assert_eq!(v["unknown"], false);
    assert_eq!(v["unit"], "Mt");
    assert_eq!(v["pack"], "coal_mining");
    assert_eq!(v["metric"], "production_mt");
    assert_eq!(v["provenance"]["kind"], "manual");
    assert_eq!(v["provenance"]["document_date"], "2026-07-30");
    assert!(v["provenance"]["document"]
        .as_str()
        .unwrap()
        .starts_with("Laporan operasional"));
    assert!(v["provenance"]["entered_at"].as_str().is_some());

    // Replace with an explicit unknown: still a row, value null, unknown true.
    let (s, v) = call(
        &t,
        Method::PUT,
        uri,
        "k2",
        Some(json!({"unknown": true, "document": "Tidak diungkap", "document_date": "2026-07-30"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["state"], "unknown");
    assert!(v["value"].is_null());
    assert_eq!(v["unknown"], true);
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    let row = store
        .kpi("COAL", "production_mt", 2026, 2)
        .unwrap()
        .unwrap();
    assert!(row.unknown && row.value.is_none());
    assert!(
        store
            .kpi("COAL", "production_mt", 2026, 1)
            .unwrap()
            .is_none(),
        "not entered"
    );

    // Both value and unknown → 422; delete → not entered; delete again → 404.
    let (s, _) = call(
        &t,
        Method::PUT,
        uri,
        "k3",
        Some(json!({"value": 1, "unknown": true, "document": "x", "document_date": "2026-07-30"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = call(&t, Method::DELETE, uri, "k4", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(store
        .kpi("COAL", "production_mt", 2026, 2)
        .unwrap()
        .is_none());
    let (s, _) = call(&t, Method::DELETE, uri, "k5", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Every write landed in the log.
    let log = std::fs::read_to_string(t.cfg.research_db_path.with_file_name(LOG_FILE)).unwrap();
    assert_eq!(log.matches("\"op\":\"kpi_set\"").count(), 2);
    assert_eq!(log.matches("\"op\":\"kpi_delete\"").count(), 1);
    assert!(log.contains("Tidak diungkap"));
}

#[tokio::test]
async fn validation_and_lookup_errors() {
    let t = app();
    let base = "/api/research/COAL/kpi";
    for (key, uri, body, want) in [
        (
            "e1",
            "production_mt/2026/2",
            json!({"document": "x", "document_date": "2026-07-30"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e2",
            "production_mt/2026/2",
            json!({"value": 1, "document": "", "document_date": "2026-07-30"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e3",
            "production_mt/2026/2",
            json!({"value": 1, "document": "x", "document_date": "30-07-2026"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e4",
            "production_mt/2026/5",
            entry(1.0),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e5",
            "production_mt/26/2",
            entry(1.0),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e6",
            "reserves_mt/2025/2",
            entry(1.0),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "e7",
            "mining_cost_ratio/2026/2",
            entry(1.0),
            StatusCode::NOT_FOUND,
        ),
        ("e8", "nope/2026/2", entry(1.0), StatusCode::NOT_FOUND),
    ] {
        let (s, v) = call(&t, Method::PUT, &format!("{base}/{uri}"), key, Some(body)).await;
        assert_eq!(s, want, "{key}: {v}");
    }
    // An annual KPI lands at Q4.
    let (s, v) = call(
        &t,
        Method::PUT,
        &format!("{base}/reserves_mt/2025/4"),
        "a1",
        Some(entry(300.0)),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["q"], 4);
    // Unknown ticker, ticker without a pack, malformed ticker.
    let (s, _) = call(
        &t,
        Method::PUT,
        "/api/research/ZZZZ/kpi/production_mt/2026/2",
        "e9",
        Some(entry(1.0)),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v) = call(
        &t,
        Method::PUT,
        "/api/research/NOPK/kpi/car/2026/2",
        "e10",
        Some(entry(1.0)),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    assert!(
        v["error"].as_str().unwrap_or_default().contains("no pack")
            || v.to_string().contains("no pack"),
        "{v}"
    );
    let (s, _) = call(
        &t,
        Method::PUT,
        "/api/research/c1/kpi/production_mt/2026/2",
        "e11",
        Some(entry(1.0)),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // No Idempotency-Key → 422; no auth → 401.
    let res = t
        .send(
            Request::put(format!("{base}/production_mt/2026/2"))
                .header("authorization", t.bearer())
                .header("content-type", "application/json")
                .body(Body::from(entry(1.0).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let res = t
        .send(
            Request::put(format!("{base}/production_mt/2026/2"))
                .header("content-type", "application/json")
                .body(Body::from(entry(1.0).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn kpi_rows_survive_a_rebuild() {
    let t = app();
    let uri = "/api/research/COAL/kpi/asp_usd_t/2026/1";
    let (s, _) = call(&t, Method::PUT, uri, "r1", Some(entry(61.5))).await;
    assert_eq!(s, StatusCode::OK);
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    store
        .with_conn(|c| {
            c.execute("DELETE FROM kpi_values", [])?;
            Ok(())
        })
        .unwrap();
    assert!(store.rebuild().unwrap() >= 1);
    let row = store.kpi("COAL", "asp_usd_t", 2026, 1).unwrap().unwrap();
    assert_eq!(row.value, Some(61.5));
    assert_eq!(row.unit, "USD/t");
}

#[tokio::test]
async fn a_kpi_entered_under_an_earlier_pack_can_still_be_deleted() {
    let t = app();
    let uri = "/api/research/COAL/kpi/production_mt/2026/2";
    let (s, _) = call(&t, Method::PUT, uri, "p1", Some(entry(7.0))).await;
    assert_eq!(s, StatusCode::OK);
    // The owner switches COAL to another pack: production_mt is not a KPI there.
    let (s, _) = call(
        &t,
        Method::PUT,
        "/api/research/universe/COAL",
        "p2",
        Some(json!({"pack_override": "shipping"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&t, Method::PUT, uri, "p3", Some(entry(7.5))).await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "entering needs the metric in the pack in effect"
    );
    let (s, _) = call(&t, Method::DELETE, uri, "p4", None).await;
    assert_eq!(
        s,
        StatusCode::NO_CONTENT,
        "but a stored row is always removable"
    );
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    assert!(store
        .kpi("COAL", "production_mt", 2026, 2)
        .unwrap()
        .is_none());
}
