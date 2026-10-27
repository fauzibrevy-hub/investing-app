//! `/api/research/*` plumbing that is not tied to one payload: the 503 without a store, the
//! auth mount, the refresh throttle, `seen` writing only `seen_at`, the removed v4 routes,
//! the manual share count write, and owner data surviving a restart and a rebuild.
//! Payload shapes have their own golden tests (`research_canon`, `research_valuation`, ...).

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use investing_app::research::api::{self, ApiState, Options};
use investing_app::research::engine::Engine;
use investing_app::research::packs::Packs;
use investing_app::research::store::{Store, LOG_FILE};
use r4_env::{call, env, Env};
use serde_json::json;
use tokio::sync::mpsc;

fn refresh_app(e: &Env, tx: Option<mpsc::UnboundedSender<Option<String>>>) -> axum::Router {
    app_with(e, "refresh-idempotency.jsonl", tx)
}

/// A router over `e.store` with its own idempotency journal; the same file name gives the
/// router a fresh process would build after a restart.
fn app_with(
    e: &Env,
    idem: &str,
    tx: Option<mpsc::UnboundedSender<Option<String>>>,
) -> axum::Router {
    let c = e.clock.clone();
    let engine = Arc::new(Engine::new(
        e.store.clone(),
        e.dir.path().join("emitens.csv"),
        Arc::new(move || *c.lock().unwrap()),
    ));
    api::router(ApiState::new(
        Some(engine),
        Options {
            idem_path: e.dir.path().join(idem),
            watchlist: Some(e.dir.path().join("research/watchlist.csv")),
            refresh_tx: tx,
            packs: Arc::new(Packs::embedded()),
            rules_dir: None,
            documents_dir: e.dir.path().join("research/documents"),
        },
    ))
}

#[tokio::test]
async fn store_unavailable_is_503() {
    let app = investing_app::research::router_with(None);
    for uri in ["/api/research/universe", "/api/research/COAL"] {
        let (s, _, _) = call(&app, Method::GET, uri, None, None).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
    }
}

#[tokio::test]
async fn the_router_is_mounted_behind_auth() {
    let t = common::test_app();
    let res = t
        .send(
            Request::get("/api/research/universe")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let (s, v) = t.get("/api/research/universe").await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["items"].as_array().unwrap().is_empty());
    let (s, v) = t.get("/api/research/rules").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["rules_version"], "2026.10-r3");
}

#[tokio::test]
async fn the_v4_routes_are_gone() {
    let e = env();
    for uri in [
        "/api/research/v4/universe",
        "/api/research/v4/rules",
        "/api/research/v4/COAL",
        "/api/research/v4/COAL/financials",
        "/api/research/v4/COAL/prices",
        "/api/research/context/macro",
        "/api/research/COAL/history",
        "/api/research/COAL/notes",
        "/api/research/COAL/valuation/history",
    ] {
        let (s, _, _) = call(&e.app, Method::GET, uri, None, None).await;
        assert!(
            s == StatusCode::NOT_FOUND || s == StatusCode::METHOD_NOT_ALLOWED,
            "{uri} answered {s}"
        );
    }
    let (s, _, _) = call(
        &e.app,
        Method::PUT,
        "/api/research/COAL/target",
        Some("t-1"),
        Some(serde_json::json!({"entry_price": 10.0})),
    )
    .await;
    assert!(s == StatusCode::NOT_FOUND || s == StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn seen_records_only_the_time() {
    let e = env();
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/seen",
        Some("seen-1"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let seen = e.store.seen("COAL").unwrap().expect("seen row");
    assert_eq!(seen.ticker, "COAL");
    assert!(!seen.seen_at.is_empty());
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/ZZZZ/seen",
        Some("seen-2"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refresh_is_accepted_then_throttled() {
    let e = env();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let app = refresh_app(&e, Some(tx));
    let post = |uri: &'static str, key: &'static str| {
        let app = app.clone();
        async move { call(&app, Method::POST, uri, Some(key), None).await }
    };
    let (s, _, _) = post("/api/research/refresh?ticker=QQQQ", "f-0").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v, _) = post("/api/research/refresh?ticker=COAL", "f-1").await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(v["status"], "accepted");
    assert_eq!(rx.try_recv().unwrap(), Some("COAL".into()));
    let (s, _, h) = post("/api/research/refresh?ticker=COAL", "f-2").await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(h.get("retry-after").is_some());
    assert!(rx.try_recv().is_err(), "a throttled request is not queued");
    let (s, _, _) = post("/api/research/refresh?ticker=BANK", "f-3").await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let (s, _, _) = post("/api/research/refresh", "f-4").await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(rx.try_recv().unwrap(), Some("BANK".into()));
    assert_eq!(rx.try_recv().unwrap(), None);
    let (s, _, h) = post("/api/research/refresh?ticker=BANK", "f-3").await;
    assert_eq!(
        (s, h["idempotent-replay"].to_str().unwrap()),
        (StatusCode::ACCEPTED, "true")
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn refresh_without_a_worker_is_503() {
    let e = env();
    let app = refresh_app(&e, None);
    let (s, _, _) = call(
        &app,
        Method::POST,
        "/api/research/refresh",
        Some("x-1"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn shares_are_validated_and_idempotent() {
    let e = env();
    let uri = "/api/research/COAL/shares";
    let good =
        json!({"shares": 2500000.0, "as_of": "2026-06-30", "source_note": "laporan tahunan"});
    let (s, v, _) = call(&e.app, Method::PUT, uri, Some("s-1"), Some(good.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["shares"], 2_500_000.0);
    let (s, v2, h) = call(&e.app, Method::PUT, uri, Some("s-1"), Some(good.clone())).await;
    assert_eq!((s, &v2), (StatusCode::OK, &v));
    assert_eq!(h["idempotent-replay"], "true");
    let (s, v, _) = call(&e.app, Method::PUT, uri, None, Some(good)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"][0]["loc"], json!(["header", "idempotency-key"]));
    for bad in [
        json!({"shares": 0, "as_of": "2026-06-30", "source_note": "x"}),
        json!({"shares": 5, "as_of": "30/06/2026", "source_note": "x"}),
        json!({"shares": 5, "as_of": "2026-06-30", "source_note": ""}),
        json!({"shares": 5, "as_of": "2026-06-30", "source_note": "z".repeat(201)}),
    ] {
        let (s, _, _) = call(&e.app, Method::PUT, uri, Some("s-bad"), Some(bad.clone())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let (s, _, _) = call(
        &e.app,
        Method::PUT,
        "/api/research/ZZZZ/shares",
        Some("s-unknown"),
        Some(json!({"shares": 5, "as_of": "2026-06-30", "source_note": "x"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Counts are a dated series: `?as_of=` removes one, no query removes them all.
    let later = json!({"shares": 2600000.0, "as_of": "2026-09-30", "source_note": "rights issue"});
    let (s, _, _) = call(&e.app, Method::PUT, uri, Some("s-2"), Some(later)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.store.shares_manual_all("COAL").unwrap().len(), 2);
    let del = |q: &'static str, key: &'static str| {
        let app = e.app.clone();
        async move {
            call(&app, Method::DELETE, &format!("{uri}{q}"), Some(key), None)
                .await
                .0
        }
    };
    assert_eq!(
        del("?as_of=2026-09-30", "s-del-1").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.store.shares_manual("COAL").unwrap().unwrap().as_of,
        "2026-06-30"
    );
    assert_eq!(
        del("?as_of=30/09/2026", "s-del-bad").await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (s, _, _) = call(&e.app, Method::DELETE, uri, None, None).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a delete needs a key too"
    );
    assert_eq!(del("", "s-del").await, StatusCode::NO_CONTENT);
    assert!(e.store.shares_manual("COAL").unwrap().is_none());
}

#[tokio::test]
async fn idempotency_survives_a_restart_and_owner_data_survives_rebuild() {
    let e = env();
    let app = app_with(&e, "restart-idempotency.jsonl", None);
    let shares = json!({"shares": 1500000.0, "as_of": "2026-06-30", "source_note": "prospektus"});
    let (s, first, _) = call(
        &app,
        Method::PUT,
        "/api/research/COAL/shares",
        Some("r-s"),
        Some(shares.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = call(
        &app,
        Method::POST,
        "/api/research/universe",
        Some("r-u"),
        Some(json!({"ticker": "JJJJ"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _, _) = call(
        &app,
        Method::POST,
        "/api/research/COAL/seen",
        Some("r-seen"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // A new router over the same store and journal behaves as after a restart.
    drop(app);
    let restarted = app_with(&e, "restart-idempotency.jsonl", None);
    let (s, again, h) = call(
        &restarted,
        Method::PUT,
        "/api/research/COAL/shares",
        Some("r-s"),
        Some(shares),
    )
    .await;
    assert_eq!((s, &again), (StatusCode::OK, &first));
    assert_eq!(h["idempotent-replay"], "true");
    assert_eq!(e.store.shares_manual_all("COAL").unwrap().len(), 1);

    // Rebuilding from the log restores the same owner data, twice over.
    let snapshot = |store: &Store| {
        format!(
            "{:?} {:?} {:?}",
            store.shares_manual_all("COAL").unwrap(),
            store
                .universe_entry("JJJJ")
                .unwrap()
                .map(|u| (u.ticker, u.role)),
            store.seen("COAL").unwrap(),
        )
    };
    let before = snapshot(&e.store);
    assert!(!before.contains("None"), "{before}");
    let applied = e.store.rebuild().unwrap();
    assert!(applied >= 3, "{applied}");
    assert_eq!(snapshot(&e.store), before);
    assert_eq!(e.store.rebuild().unwrap(), applied);
    assert_eq!(snapshot(&e.store), before);

    // And after losing the database file altogether.
    drop(restarted);
    let log = e.dir.path().join(LOG_FILE);
    assert!(log.exists());
    let fresh = tempfile::tempdir().unwrap();
    std::fs::copy(&log, fresh.path().join(LOG_FILE)).unwrap();
    let restored = Store::open(&fresh.path().join("research.sqlite")).unwrap();
    assert_eq!(snapshot(&restored), before);
}
