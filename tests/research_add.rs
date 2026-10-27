//! Add-ticker wizard backend (design.md §4.5; prd.md A6.2, A6.3): `GET /api/research/lookup`
//! (exchange-list check, filing-derived industry code, proposed pack, peer search), the
//! price backfill queued when `POST /universe` creates a row, and removal rules.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use finance_app::research::api::{self, ApiState, Options};
use finance_app::research::engine::Engine;
use r4_env::{call, env, Env};
use serde_json::{json, Value};
use tokio::sync::mpsc;

const EMITENS: &str = "ticker,name\nCOAL,Batubara Contoh\nBANK,Bank Contoh Syariah\nNONE,Tanpa Paket\nZETA,Zeta Logistik\nZETB,Zeta Pelayaran\nZETC,Zeta Energi\nZETD,Zeta Tambang\nYYYY,Perusahaan Y\n";

fn app(
    e: &Env,
    csv: Option<&str>,
    tx: Option<mpsc::UnboundedSender<Option<String>>>,
) -> axum::Router {
    let path = e.dir.path().join("emitens-add.csv");
    if let Some(csv) = csv {
        std::fs::write(&path, csv).unwrap();
    }
    let c = e.clock.clone();
    let engine = Arc::new(Engine::new(
        e.store.clone(),
        path,
        Arc::new(move || *c.lock().unwrap()),
    ));
    api::router(ApiState::new(
        Some(engine),
        Options {
            idem_path: e.dir.path().join("add-idempotency.jsonl"),
            watchlist: Some(e.dir.path().join("research/watchlist.csv")),
            refresh_tx: tx,
            packs: e.packs.clone(),
            rules_dir: None,
            documents_dir: e.dir.path().join("research/documents"),
        },
    ))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let (s, v, _) = call(app, Method::GET, uri, None, None).await;
    (s, v)
}

async fn post(app: &axum::Router, key: &str, body: Value) -> (StatusCode, Value) {
    let (s, v, _) = call(
        app,
        Method::POST,
        "/api/research/universe",
        Some(key),
        Some(body),
    )
    .await;
    (s, v)
}

#[tokio::test]
async fn lookup_returns_the_filing_code_and_proposed_pack() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    let (s, v) = get(&a, "/api/research/lookup?q=coal").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["list_available"], true);
    assert_eq!(v["query"], "COAL");
    let x = &v["exact"];
    assert_eq!(x["ticker"], "COAL");
    assert_eq!(x["name"], "Batubara Contoh");
    assert_eq!(x["listed"], true);
    assert_eq!(x["industry_code"], "A121");
    assert_eq!(x["pack"]["id"], "coal_mining");
    assert_eq!(x["in_universe"], true);
    assert_eq!(x["role"], "held");

    // No pack claims this filing's code: the code is reported, the pack is null.
    let (_, v) = get(&a, "/api/research/lookup?q=none").await;
    assert!(v["exact"]["pack"].is_null());
    assert!(v["exact"]["industry_code"].is_string());

    // Listed in the exchange list but no filing yet: no code, no pack, not in the universe.
    let (_, v) = get(&a, "/api/research/lookup?q=yyyy").await;
    assert_eq!(v["exact"]["listed"], true);
    assert!(v["exact"]["industry_code"].is_null() && v["exact"]["pack"].is_null());
    assert_eq!(v["exact"]["in_universe"], false);
    assert!(v["exact"]["role"].is_null());
}

#[tokio::test]
async fn lookup_rejects_what_the_exchange_list_does_not_know() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    let (_, v) = get(&a, "/api/research/lookup?q=qqqq").await;
    assert!(v["exact"].is_null(), "unlisted ticker has no exact match");
    assert_eq!(v["items"], json!([]));
    // Not four letters: no exact match either.
    let (_, v) = get(&a, "/api/research/lookup?q=zet").await;
    assert!(v["exact"].is_null());
    let (_, v) = get(&a, "/api/research/lookup").await;
    assert_eq!(
        (v["query"].as_str(), v["items"].as_array().map(Vec::len)),
        (Some(""), Some(0))
    );
}

#[tokio::test]
async fn lookup_searches_by_ticker_prefix_then_name_and_caps_the_list() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    let (_, v) = get(&a, "/api/research/lookup?q=zet").await;
    let tickers: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["ticker"].as_str().unwrap())
        .collect();
    assert_eq!(tickers, ["ZETA", "ZETB", "ZETC", "ZETD"]);
    let (_, v) = get(&a, "/api/research/lookup?q=zet&limit=2").await;
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    // A name match ranks after ticker prefixes.
    let (_, v) = get(&a, "/api/research/lookup?q=syariah").await;
    assert_eq!(v["items"][0]["ticker"], "BANK");
    let (_, v) = get(&a, "/api/research/lookup?q=y").await;
    assert_eq!(v["items"][0]["ticker"], "YYYY");
}

#[tokio::test]
async fn lookup_without_an_exchange_list_says_it_could_not_check() {
    let e = env();
    let a = app(&e, None, None);
    let (_, v) = get(&a, "/api/research/lookup?q=qqqq").await;
    assert_eq!(v["list_available"], false);
    assert_eq!(v["exact"]["ticker"], "QQQQ");
    assert!(v["exact"]["listed"].is_null());
    // Filings on disk still count as known.
    let (_, v) = get(&a, "/api/research/lookup?q=coal").await;
    assert_eq!(v["exact"]["listed"], true);
}

#[tokio::test]
async fn adding_an_unlisted_ticker_is_refused_and_stores_nothing() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    let (s, v) = post(&a, "k-1", json!({"ticker": "QQQQ"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"][0]["type"], "not_listed");
    assert!(e.store.universe_entry("QQQQ").unwrap().is_none());
}

#[tokio::test]
async fn a_new_row_queues_a_price_backfill_once() {
    let e = env();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let a = app(&e, Some(EMITENS), Some(tx));
    let (s, v) = post(
        &a,
        "k-2",
        json!({"ticker": "zeta", "business_note": "Logistik contoh"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(v["ticker"], "ZETA");
    assert_eq!(v["role"], "manual");
    assert_eq!(rx.try_recv().unwrap(), Some("ZETA".into()));
    // The scraper watchlist carries it.
    let list = std::fs::read_to_string(e.dir.path().join("research/watchlist.csv")).unwrap();
    assert!(list.lines().any(|l| l == "ZETA"), "{list}");
    // Usable before any KPI: the dossier answers with an empty KPI board.
    let (s, d) = get(&a, "/api/research/ZETA").await;
    assert_eq!(s, StatusCode::OK, "{d}");

    // Same key replays; a second request for the same ticker is not a new row: no refetch.
    let (s, _) = post(
        &a,
        "k-2",
        json!({"ticker": "zeta", "business_note": "Logistik contoh"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = post(
        &a,
        "k-3",
        json!({"ticker": "ZETA", "business_note": "Catatan baru"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(rx.try_recv().is_err(), "an existing row is not refetched");

    // A peer is a new row too.
    let (s, v) = post(
        &a,
        "k-4",
        json!({"ticker": "ZETB", "role": "peer", "peer_of": "ZETA"}),
    )
    .await;
    assert_eq!(
        (s, v["role"].as_str(), v["peer_of"].as_str()),
        (StatusCode::CREATED, Some("peer"), Some("ZETA"))
    );
    assert_eq!(rx.try_recv().unwrap(), Some("ZETB".into()));
}

#[tokio::test]
async fn held_cannot_be_removed_but_manual_and_peer_can() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    post(&a, "r-1", json!({"ticker": "ZETA"})).await;
    post(
        &a,
        "r-2",
        json!({"ticker": "ZETB", "role": "peer", "peer_of": "ZETA"}),
    )
    .await;
    let del = |key: &'static str, t: &'static str| {
        let a = a.clone();
        async move {
            call(
                &a,
                Method::DELETE,
                &format!("/api/research/universe/{t}"),
                Some(key),
                None,
            )
            .await
            .0
        }
    };
    assert_eq!(del("r-3", "COAL").await, StatusCode::CONFLICT);
    assert!(e.store.universe_entry("COAL").unwrap().is_some());
    assert_eq!(del("r-4", "ZETB").await, StatusCode::NO_CONTENT);
    assert_eq!(del("r-5", "ZETA").await, StatusCode::NO_CONTENT);
    let list = std::fs::read_to_string(e.dir.path().join("research/watchlist.csv")).unwrap();
    assert!(list.lines().any(|l| l == "COAL"));
    assert!(!list.lines().any(|l| l == "ZETA" || l == "ZETB"), "{list}");
}

async fn put_peers(app: &axum::Router, key: &str, t: &str, peers: Value) -> (StatusCode, Value) {
    let (s, v, _) = call(
        app,
        Method::PUT,
        &format!("/api/research/universe/{t}"),
        Some(key),
        Some(json!({ "peers": peers })),
    )
    .await;
    (s, v)
}

#[tokio::test]
async fn peer_cycles_of_any_length_are_refused_on_post_and_put() {
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    // ZETD <- ZETC <- ZETB (each a peer of the previous), via POST.
    assert_eq!(
        post(&a, "c-1", json!({"ticker": "ZETD"})).await.0,
        StatusCode::CREATED
    );
    let (s, _) = post(
        &a,
        "c-2",
        json!({"ticker": "ZETC", "role": "peer", "peer_of": "ZETD"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = post(
        &a,
        "c-3",
        json!({"ticker": "ZETB", "role": "peer", "peer_of": "ZETC"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    // Closing the loop: ZETD as a peer of ZETB (3-cycle) is refused.
    let (s, v) = post(
        &a,
        "c-4",
        json!({"ticker": "ZETD", "role": "manual", "peer_of": "ZETB"}),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    // Same through PUT: ZETB's peers = [ZETD] closes the loop.
    let (s, v) = put_peers(&a, "c-5", "ZETB", json!(["ZETD"])).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(
        e.store.universe_entry("ZETD").unwrap().unwrap().peer_of,
        None
    );
}

#[tokio::test]
async fn delete_inside_a_stored_peer_cycle_answers_normally() {
    use finance_app::research::universe::{Role, UniverseEntry};
    let e = env();
    let a = app(&e, Some(EMITENS), None);
    let row = |t: &str, of: &str| UniverseEntry {
        ticker: t.into(),
        role: Role::Peer,
        peer_of: Some(of.into()),
        pack_override: None,
        business_note: None,
        added_at: "2026-10-22T00:00:00Z".into(),
        synced_at: "2026-10-22T00:00:00Z".into(),
    };
    // A cycle written straight to the store (older data): ZETD <- ZETE <- YYYY <- ZETD.
    for r in [
        row("ZETD", "YYYY"),
        row("ZETA", "ZETD"),
        row("YYYY", "ZETA"),
    ] {
        e.store.upsert_universe(&r).unwrap();
    }
    let (s, _, _) = call(
        &a,
        Method::DELETE,
        "/api/research/universe/ZETD",
        Some("d-1"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    for t in ["ZETD", "ZETA", "YYYY"] {
        assert!(e.store.universe_entry(t).unwrap().is_none(), "{t}");
    }
    // PUT closing a stored loop's peers also answers.
    assert_eq!(
        put_peers(&a, "d-2", "COAL", json!([])).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn put_peers_checks_the_exchange_list_and_queues_a_backfill() {
    let e = env();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let a = app(&e, Some(EMITENS), Some(tx));
    let (s, v) = put_peers(&a, "p-1", "NONE", json!(["QQQQ"])).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["detail"][0]["type"], "not_listed");
    assert!(e.store.universe_entry("QQQQ").unwrap().is_none());
    assert!(rx.try_recv().is_err());
    let (s, v) = put_peers(&a, "p-2", "NONE", json!(["zeta"])).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(rx.try_recv().unwrap(), Some("ZETA".into()));
    // A repeat is not a new row.
    assert_eq!(
        put_peers(&a, "p-3", "NONE", json!(["ZETA"])).await.0,
        StatusCode::OK
    );
    assert!(rx.try_recv().is_err());
}
