//! Canon universe routes (research plan §5; prd.md A2.4, A2.5, A2.6, A6.3):
//! `GET/POST /api/research/universe`, `PUT/DELETE /api/research/universe/{ticker}`, over a
//! temp store with one held ticker. Writes carry an `Idempotency-Key`.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::{json_of, TestApp};
use investing_app::research::store::Store;
use investing_app::research::universe::{sync, HeldPosition};
use serde_json::{json, Value};

fn app() -> TestApp {
    let t = common::test_app();
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    sync(
        &store,
        &[HeldPosition {
            ticker: "AAAA".into(),
            avg_price: 1_250.0,
        }],
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

#[tokio::test]
async fn universe_lists_held_manual_and_peers_with_pack_choice() {
    let t = app();
    let (s, v) = t.get("/api/research/universe").await;
    assert_eq!(s, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ticker"], "AAAA");
    assert_eq!(items[0]["role"], "held");
    assert_eq!(items[0]["avg_price"], 1250.0);
    assert_eq!(
        items[0]["pack"]["source"], "none",
        "no filing yet: no pack (A2.5)"
    );
    assert!(items[0]["pack"]["id"].is_null());

    // Manual add with an override and a note.
    let (s, v) = call(
        &t,
        Method::POST,
        "/api/research/universe",
        "u1",
        Some(json!({"ticker": " bbbb ", "pack_override": "it_distribution", "business_note": "Distributor sintetis"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["ticker"], "BBBB");
    assert_eq!(v["role"], "manual");
    assert_eq!(v["pack"]["id"], "it_distribution");
    assert_eq!(v["pack"]["source"], "override");
    assert!(v["pack"]["filing_default"].is_null());
    assert_eq!(v["business_note"], "Distributor sintetis");

    // A peer of BBBB, then peers via PUT, capped at three.
    let (s, v) = call(
        &t,
        Method::POST,
        "/api/research/universe",
        "u2",
        Some(json!({"ticker": "CCCC", "role": "peer", "peer_of": "BBBB"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["role"], "peer");
    assert_eq!(v["peer_of"], "BBBB");
    let (s, v) = call(
        &t,
        Method::PUT,
        "/api/research/universe/bbbb",
        "u3",
        Some(json!({"peers": ["CCCC", "DDDD", "EEEE", "FFFF"]})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (s, v) = call(
        &t,
        Method::PUT,
        "/api/research/universe/bbbb",
        "u4",
        Some(json!({"peers": ["CCCC", "DDDD"], "pack_override": null, "business_note": "Baru"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["peers"], json!(["CCCC", "DDDD"]));
    assert_eq!(v["pack"]["source"], "none", "override cleared");
    assert_eq!(v["business_note"], "Baru");

    let (_, v) = t.get("/api/research/universe").await;
    let tickers: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["ticker"].as_str().unwrap())
        .collect();
    assert_eq!(tickers, ["AAAA", "BBBB", "CCCC", "DDDD"]);
    // The scraper watchlist carries the peers (A2.6).
    let csv = std::fs::read_to_string(&t.cfg.research_scraper_watchlist).unwrap();
    assert_eq!(csv, "ticker\nAAAA\nBBBB\nCCCC\nDDDD\n");

    // Bad inputs: unknown pack, unknown peer_of, bad role, self peer.
    for (key, body) in [
        ("e1", json!({"ticker": "GGGG", "pack_override": "nope"})),
        ("e2", json!({"ticker": "GGGG", "role": "owner"})),
        ("e3", json!({"ticker": "GGGG", "role": "peer"})),
        (
            "e4",
            json!({"ticker": "GGGG", "role": "peer", "peer_of": "GGGG"}),
        ),
    ] {
        let (s, v) = call(&t, Method::POST, "/api/research/universe", key, Some(body)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {v}");
    }
    let (s, _) = call(
        &t,
        Method::POST,
        "/api/research/universe",
        "e5",
        Some(json!({"ticker": "GGGG", "role": "peer", "peer_of": "ZZZZ"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &t,
        Method::PUT,
        "/api/research/universe/ZZZZ",
        "e6",
        Some(json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Removal: held → 409, peer → 204 and gone, unknown → 404, malformed → 422.
    let (s, _) = call(
        &t,
        Method::DELETE,
        "/api/research/universe/AAAA",
        "d1",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(
        &t,
        Method::DELETE,
        "/api/research/universe/dddd",
        "d2",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(
        &t,
        Method::DELETE,
        "/api/research/universe/DDDD",
        "d3",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(&t, Method::DELETE, "/api/research/universe/d1", "d4", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Removing BBBB takes its peer-only row CCCC with it.
    let (s, _) = call(
        &t,
        Method::DELETE,
        "/api/research/universe/BBBB",
        "d5",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v) = t.get("/api/research/universe").await;
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(&t.cfg.research_scraper_watchlist).unwrap(),
        "ticker\nAAAA\n"
    );
}

#[tokio::test]
async fn writes_need_a_key_and_auth() {
    let t = app();
    let res = t
        .send(
            Request::post("/api/research/universe")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ticker":"BBBB"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = t
        .send(
            Request::post("/api/research/universe")
                .header("authorization", t.bearer())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ticker":"BBBB"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "no Idempotency-Key"
    );
}

#[tokio::test]
async fn non_string_fields_are_rejected_on_add() {
    let t = app();
    for (key, body) in [
        ("n1", json!({"ticker": "BBBB", "role": 5})),
        ("n2", json!({"ticker": "BBBB", "pack_override": 5})),
        ("n3", json!({"ticker": "BBBB", "business_note": ["x"]})),
        (
            "n4",
            json!({"ticker": "BBBB", "role": "peer", "peer_of": 7}),
        ),
    ] {
        let (s, v) = call(&t, Method::POST, "/api/research/universe", key, Some(body)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {v}");
    }
    let (_, v) = t.get("/api/research/universe").await;
    assert_eq!(v["items"].as_array().unwrap().len(), 1, "nothing was added");
}
