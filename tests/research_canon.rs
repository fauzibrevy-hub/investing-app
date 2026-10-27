//! R5-A1: the canon endpoints the desktop workspace consumes, over one synthetic store, and the
//! golden files the web lanes type against (`tests/fixtures/research/canon/*.json`).
//! `UPDATE_GOLDEN=1` rewrites them; otherwise one test compares every file. The store has five
//! tickers (two packs and a peer, one with no pack, one with a restated and a comparative-only
//! quarter), a failing ticker for the accounting flag, claims in all four statuses, journal
//! entries with a level and an annotation, a document linked as evidence, events of every kind,
//! a KPI gap and an explicit unknown, and a scenario row set. Everything is invented.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use chrono::{DateTime, Utc};
use investing_app::research::api::{self, ApiState, Options};
use investing_app::research::claims::source::reevaluate;
use investing_app::research::engine::Engine;
use investing_app::research::events::react::{react, Scope};
use investing_app::research::packs::Packs;
use investing_app::research::store::{JournalRow, PriceBar, Store};
use investing_app::research::universe::{add, sync, AddRequest, HeldPosition, Role};
use investing_app::research::valuation::PojkOffsets;
use investing_app::research::xbrl;
use r4_env::{closes, Env, HELD, NOW};
use serde_json::{json, Value};
use xbrl_fixture::{general_year, Year};

fn canon_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/research/canon")
}

/// Floats to six decimals and any run of ten digits in a string (hashes, paths) cut, so no
/// fixture holds a long digit run (plan §9 real-data check) and the files are byte-stable.
fn scrub(v: &Value, tmp: &str, long: &regex::Regex) -> Value {
    match v {
        // Rupiah amounts of a billion or more scaled down by 1e6, as `research_api.rs` does.
        Value::Number(n) if n.as_f64().is_some_and(|x| x.abs() >= 1e9) => {
            json!((n.as_f64().unwrap() / 1e6 * 1e4).round() / 1e4)
        }
        Value::Number(n) if !(n.is_i64() || n.is_u64()) => {
            json!((n.as_f64().unwrap() * 1e6).round() / 1e6)
        }
        Value::String(s) => {
            Value::String(long.replace_all(&s.replace(tmp, "<tmp>"), "#").into_owned())
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| scrub(x, tmp, long)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), scrub(x, tmp, long)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Compares `actual` with `canon/<name>.json`, or writes it under `UPDATE_GOLDEN`.
fn golden(e: &Env, name: &str, actual: &Value) {
    let long = regex::Regex::new("[0-9]{10,}").unwrap();
    let tmp = e.dir.path().display().to_string();
    let actual = scrub(actual, &tmp, &long);
    let mut text = serde_json::to_string_pretty(&actual).unwrap();
    text.push('\n');
    assert!(!long.is_match(&text), "{name}: a run of ten digits");
    let path = canon_dir().join(format!("{name}.json"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(canon_dir()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing canon/{name}.json (run with UPDATE_GOLDEN=1)"));
    assert_eq!(
        expected, text,
        "canon/{name}.json differs; re-run with UPDATE_GOLDEN=1 if the change is intended"
    );
}

const EMITENS: &str = "ticker,name,taxonomy_type\nCOAL,PT Batubara Contoh Tbk,general\nBANK,PT Bank Syariah Contoh Tbk,banking\nREST,PT Mesin Contoh Tbk,general\n";

fn bar(date: &str, close: f64) -> PriceBar {
    PriceBar {
        date: date.into(),
        open: None,
        high: None,
        low: None,
        close,
        volume: Some(1_000),
    }
}

fn offsets() -> PojkOffsets {
    PojkOffsets {
        q1: 30,
        q2: 60,
        q3: 30,
        fy: 90,
    }
}

/// The R4 environment plus a named company file, a peer, a restated ticker and a failing one.
fn canon_env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    let mut files = 0;
    let mut peer = pack_fixture::coal();
    peer.ticker = "COPR";
    for spec in [
        pack_fixture::coal(),
        pack_fixture::bank(),
        pack_fixture::none(),
        peer,
    ] {
        files += pack_fixture::write_ticker(&xbrl_dir, &spec);
    }
    const Y2024: Year = Year {
        fy: 2024,
        revenue: [100.0, 230.0, 350.0, 500.0],
        profit: [10.0, 22.0, 33.0, 50.0],
        equity: [400.0, 410.0, 420.0, 440.0],
    };
    const Y2025: Year = Year {
        fy: 2025,
        revenue: [120.0, 260.0, 390.0, 560.0],
        profit: [12.0, 26.0, 39.0, 56.0],
        equity: [450.0, 460.0, 470.0, 490.0],
    };
    const BUST: Year = Year {
        fy: 2025,
        revenue: [20.0, 45.0, 70.0, 90.0],
        profit: [-5.0, -12.0, -20.0, -30.0],
        equity: [50.0, 20.0, -10.0, -40.0],
    };
    // REST: the 2024 Q3 filing is missing (its quarter comes from the 2025 Q3 comparative) and
    // the 2025 Q1 filing restates the Q1 2024 revenue (100 → 105 million).
    files += general_year(&xbrl_dir, "REST", &Y2024, None, &["Q3"], None).len();
    files += general_year(
        &xbrl_dir,
        "REST",
        &Y2025,
        Some(&Y2024),
        &[],
        Some((0, 105.0)),
    )
    .len();
    files += general_year(&xbrl_dir, "BUST", &BUST, None, &[], None).len();

    let store = Arc::new(Store::open(&dir.path().join("research.sqlite")).unwrap());
    let at = DateTime::parse_from_rfc3339(NOW)
        .unwrap()
        .with_timezone(&Utc);
    let report = xbrl::ingest_new(&store, &xbrl_dir, at).unwrap();
    assert!(report.unparsed.is_empty(), "{:?}", report.unparsed);
    assert_eq!(report.ingested.len(), files);
    let held: Vec<HeldPosition> = HELD
        .iter()
        .map(|t| HeldPosition {
            ticker: (*t).into(),
            avg_price: 1_000.0,
        })
        .collect();
    sync(&store, &held, NOW).unwrap();
    std::fs::write(dir.path().join("emitens.csv"), EMITENS).unwrap();
    let clock = Arc::new(Mutex::new(at));
    let c2 = clock.clone();
    let engine = Arc::new(Engine::new(
        store.clone(),
        dir.path().join("emitens.csv"),
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
    let e = Env {
        dir,
        store,
        app,
        clock,
        packs,
    };
    let set = e.pack_set();
    for (t, role, peer_of) in [
        ("COPR", Role::Peer, Some("COAL")),
        ("REST", Role::Manual, None),
        ("BUST", Role::Manual, None),
    ] {
        add(
            &e.store,
            &set,
            &AddRequest {
                ticker: t.into(),
                role: Some(role),
                peer_of: peer_of.map(str::to_string),
                ..Default::default()
            },
            NOW,
        )
        .unwrap();
    }
    for t in HELD {
        e.store
            .upsert_prices(t, &closes("2026-10-23", 1_000.0))
            .unwrap();
    }
    e.store
        .upsert_prices("COPR", &closes("2026-10-23", 500.0))
        .unwrap();
    // REST's last close is old: a stale-price event.
    e.store
        .upsert_prices("REST", &closes("2026-09-01", 800.0))
        .unwrap();
    e
}

async fn send(
    e: &Env,
    method: Method,
    uri: &str,
    key: &str,
    body: Value,
    want: StatusCode,
) -> Value {
    let (s, v) = e.send(method, uri, key, body).await;
    assert_eq!(s, want, "{uri}: {v}");
    v
}

fn claim(
    statement: &str,
    metric: &str,
    cmp: &str,
    threshold: f64,
    window: &str,
    due: &str,
) -> Value {
    json!({
        "statement": statement, "metric": metric, "comparator": cmp,
        "threshold": threshold, "window": window, "due": due,
    })
}

/// Everything the screens show, written through the API and the R4 passes under a moving
/// clock. Returns the claim ids in creation order.
async fn populate(e: &Env) -> Vec<String> {
    const CREATED: StatusCode = StatusCode::CREATED;
    const NO_CONTENT: StatusCode = StatusCode::NO_CONTENT;
    const OK: StatusCode = StatusCode::OK;
    let (post, put) = (Method::POST, Method::PUT);
    send(
        e,
        post.clone(),
        "/api/research/COAL/seen",
        "seen1",
        json!({}),
        NO_CONTENT,
    )
    .await;
    let mut ids = Vec::new();
    let claims = [
        (
            "COAL",
            "c1",
            claim(
                "Laba bersih kuartal 2 positif",
                "core.net_income",
                ">",
                0.0,
                "quarter:2026Q2",
                "2026-12-31",
            ),
        ),
        (
            "COAL",
            "c2",
            claim(
                "Laba bersih kuartal 2 di bawah nol",
                "core.net_income",
                "<",
                0.0,
                "quarter:2026Q2",
                "2026-12-31",
            ),
        ),
        (
            "COAL",
            "c3",
            claim(
                "Produksi minimal lima juta ton",
                "coal_mining.production_mt",
                ">=",
                5.0,
                "latest",
                "2026-12-31",
            ),
        ),
        (
            "COAL",
            "c4",
            claim(
                "Laba bersih kuartal 3 positif",
                "core.net_income",
                ">",
                0.0,
                "quarter:2026Q3",
                "2026-12-31",
            ),
        ),
        // Due soon with no data behind it: expires once the clock passes the date.
        (
            "COAL",
            "c5",
            claim(
                "Laba bersih kuartal 3 di atas lima",
                "core.net_income",
                ">",
                5.0,
                "quarter:2026Q3",
                "2026-10-25",
            ),
        ),
        (
            "BANK",
            "c6",
            claim(
                "Ekuitas positif",
                "core.total_equity",
                ">",
                0.0,
                "latest",
                "2026-12-31",
            ),
        ),
    ];
    for (t, key, body) in claims {
        let v = send(
            e,
            post.clone(),
            &format!("/api/research/{t}/claims"),
            key,
            body,
            CREATED,
        )
        .await;
        ids.push(v["id"].as_str().unwrap().to_string());
    }

    // KPI: production entered (the claim above turns Berlaku), sales an explicit unknown, the
    // rest of the pack's KPIs stay gaps.
    let kpi = |extra: Value| {
        let mut b =
            json!({"document": "laporan operasional sintetis", "document_date": "2026-10-20"});
        b.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        b
    };
    send(
        e,
        put.clone(),
        "/api/research/COAL/kpi/production_mt/2026/2",
        "k1",
        kpi(json!({"value": 6.0})),
        OK,
    )
    .await;
    send(
        e,
        put.clone(),
        "/api/research/COAL/kpi/sales_mt/2026/2",
        "k2",
        kpi(json!({"unknown": true})),
        OK,
    )
    .await;

    // Journal: a level above the last close, a note, a decision, and an old level that the
    // 30-day calibration gate has already released.
    let level = send(
        e,
        post.clone(),
        "/api/research/COAL/journal",
        "j1",
        json!({"kind": "level", "text": "Harga incaran", "level": 1_300.0}),
        CREATED,
    )
    .await;
    send(
        e,
        post.clone(),
        "/api/research/COAL/journal",
        "j2",
        json!({"kind": "catatan", "text": "Manajemen menaikkan target produksi"}),
        CREATED,
    )
    .await;
    send(
        e,
        post.clone(),
        "/api/research/COAL/journal",
        "j3",
        json!({"kind": "keputusan", "text": "Menambah posisi"}),
        CREATED,
    )
    .await;
    e.store
        .add_journal(&JournalRow {
            id: "j-old".into(),
            ticker: "COAL".into(),
            at: "2026-08-20T01:00:00+00:00".into(),
            kind: "level".into(),
            text: "Level lama".into(),
            price_at: Some(1_000.0),
            level: Some(1_100.0),
        })
        .unwrap();
    let annotate = format!(
        "/api/research/journal/{}/annotations",
        level["id"].as_str().unwrap()
    );
    send(
        e,
        post.clone(),
        &annotate,
        "a1",
        json!({"text": "Level digeser setelah laporan kuartal 2"}),
        CREATED,
    )
    .await;

    // Documents: one linked to the production claim, one to a KPI cell; a manual date.
    let d1 = send(e, post.clone(), "/api/research/COAL/documents", "d1", json!({"kind": "update_operasional", "url": "https://example.test/ops", "doc_date": "2026-10-20", "claim_id": ids[2]}), CREATED).await;
    assert_eq!(d1["evidence"][0]["claim_id"], ids[2]);
    send(e, post.clone(), "/api/research/COAL/documents", "d2", json!({"kind": "catatan", "text": "Manajemen menyebut stripping ratio turun", "doc_date": "2026-09-30", "kpi_key": "production_mt:2026Q2"}), CREATED).await;
    send(
        e,
        post.clone(),
        "/api/research/COAL/calendar",
        "cal1",
        json!({"date": "2026-11-15", "kind": "rups", "note": "RUPS luar biasa"}),
        CREATED,
    )
    .await;

    // Valuation inputs and a scenario row set.
    send(
        e,
        put.clone(),
        "/api/research/COAL/shares",
        "s1",
        json!({"shares": 500_000_000.0, "as_of": "2026-06-30", "source_note": "data sintetis"}),
        OK,
    )
    .await;
    send(
        e,
        put,
        "/api/research/COAL/scenarios",
        "sc1",
        json!({
            "bear": {"growth": 0.0, "margin": 8.0, "exit_multiple": 8.0, "years": 5},
            "base": {"growth": 5.0, "margin": 10.0, "exit_multiple": 10.0, "years": 5},
            "bull": {"growth": 10.0, "margin": 12.0, "exit_multiple": 12.0, "years": 5},
        }),
        OK,
    )
    .await;

    // Later: closes cross the level both ways, the due claim lapses, every pass runs.
    e.set_clock("2026-10-28T03:00:00+00:00");
    e.store
        .upsert_prices(
            "COAL",
            &[bar("2026-10-26", 1_310.0), bar("2026-10-27", 1_290.0)],
        )
        .unwrap();
    reevaluate(&e.store, &e.pack_set(), e.now(), e.today(), None).unwrap();
    react(
        &e.store,
        &e.pack_set(),
        &offsets(),
        e.now(),
        e.today(),
        Scope::All,
    )
    .unwrap();
    ids
}

async fn get(e: &Env, uri: &str) -> Value {
    let (s, v) = e.get(uri).await;
    assert_eq!(s, StatusCode::OK, "{uri}: {v}");
    v
}

#[tokio::test]
async fn canon_goldens_match() {
    let e = canon_env();
    let ids = populate(&e).await;
    let g = |name: &str, v: &Value| golden(&e, name, v);

    let desk = get(&e, "/api/research/desk").await;
    // Every inbox kind is present, a dismissed one included.
    let first = desk["inbox"][0]["id"].as_str().unwrap().to_string();
    send(
        &e,
        Method::POST,
        &format!("/api/research/events/{first}/dismiss"),
        "dis1",
        json!({}),
        StatusCode::OK,
    )
    .await;
    let kinds: std::collections::BTreeSet<String> = e
        .store
        .events_list(false)
        .unwrap()
        .iter()
        .map(|x| x.kind.clone())
        .collect();
    assert_eq!(kinds.len(), 7, "{kinds:?}");
    g("desk", &get(&e, "/api/research/desk").await);
    g(
        "desk-COAL",
        &get(&e, "/api/research/desk?ticker=COAL").await,
    );

    for t in ["COAL", "BANK", "NONE", "REST", "COPR"] {
        g(
            &format!("ticker-{t}"),
            &get(&e, &format!("/api/research/{t}")).await,
        );
        // The bank pack payload alone is over a megabyte: three packs' worth is enough.
        if ["COAL", "NONE", "REST"].contains(&t) {
            g(
                &format!("pack-{t}"),
                &get(&e, &format!("/api/research/{t}/pack")).await,
            );
        }
        g(
            &format!("valuation-{t}"),
            &get(&e, &format!("/api/research/{t}/valuation")).await,
        );
    }
    for t in ["COAL", "REST"] {
        for view in ["standalone", "ttm", "annual"] {
            g(
                &format!("financials-{t}-{view}"),
                &get(&e, &format!("/api/research/{t}/financials?view={view}")).await,
            );
        }
        g(
            &format!("filings-{t}"),
            &get(&e, &format!("/api/research/filings?ticker={t}")).await,
        );
    }
    // The fact behind the latest standalone revenue cell, as a `FactChip` opens it.
    let fin = get(&e, "/api/research/COAL/financials?view=standalone").await;
    let fact = fin["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|grp| grp["rows"].as_array().unwrap())
        .find(|r| r["id"] == "revenue")
        .and_then(|r| {
            r["cells"]
                .as_array()
                .unwrap()
                .last()
                .map(|c| c["fact"].clone())
        })
        .unwrap();
    let uri = format!(
        "/api/research/facts/{}/{}?ctx={}",
        fact["filing_id"].as_str().unwrap(),
        fact["element"].as_str().unwrap(),
        fact["ctx"].as_str().unwrap()
    );
    g("facts-COAL", &get(&e, &uri).await);
    g(
        "prices-COAL-1y",
        &get(&e, "/api/research/COAL/prices?range=1y").await,
    );
    g("claims-COAL", &get(&e, "/api/research/COAL/claims").await);
    g(
        "claim-evals-c1",
        &get(&e, &format!("/api/research/claims/{}/evals", ids[0])).await,
    );
    g(
        "claim-evidence-c3",
        &get(&e, &format!("/api/research/claims/{}/evidence", ids[2])).await,
    );
    g("journal-COAL", &get(&e, "/api/research/COAL/journal").await);
    g(
        "documents-COAL",
        &get(&e, "/api/research/COAL/documents").await,
    );
    g(
        "calendar-COAL",
        &get(&e, "/api/research/COAL/calendar").await,
    );
    for pack in ["coal_mining", "bank_sharia", "heavy_equipment_contracting"] {
        g(
            &format!("compare-{pack}"),
            &get(&e, &format!("/api/research/compare?pack={pack}")).await,
        );
    }
    g(
        "calibration-COAL",
        &get(&e, "/api/research/calibration?ticker=COAL").await,
    );
    g("calibration", &get(&e, "/api/research/calibration").await);
    g("packs", &get(&e, "/api/research/packs").await);
    g("rules", &get(&e, "/api/research/rules").await);
    g("universe", &get(&e, "/api/research/universe").await);

    // The structure map as the Struktur block reads it: a drafted map with an accepted, two
    // proposed and a rejected node (web `structure.test.tsx` builds its variants from it).
    let src = |page: u32, snippet: &str| json!({"document": "laporan-tahunan-2025.pdf", "page": page, "snippet": snippet});
    let draft = json!({"version": 1, "ticker": "COAL", "nodes": [
        {"id": "n1", "parent": null, "label": "Segmen batubara", "kind": "segmen",
         "source": src(12, "Segmen batubara")},
        {"id": "n2", "parent": "n1", "label": "Batubara termal ekspor", "kind": "produk",
         "metric": "production_mt", "source": src(37, "Penjualan batubara termal ke pembeli di Asia")},
        {"id": "n3", "parent": "n2", "label": "Pembeli di Asia", "kind": "pelanggan",
         "source": src(38, "Pembeli utama berada di Asia")},
        {"id": "n4", "parent": null, "label": "Kontraktor tambang", "kind": "biaya",
         "source": src(40, "Jasa penambangan oleh kontraktor")},
    ]});
    let structure_dir = e.dir.path().join("research/structure");
    std::fs::create_dir_all(&structure_dir).unwrap();
    std::fs::write(structure_dir.join("COAL.draft.json"), draft.to_string()).unwrap();
    let base = "/api/research/COAL/structure";
    let draft_uri = format!("{base}/draft");
    send(
        &e,
        Method::POST,
        &draft_uri,
        "st1",
        json!({}),
        StatusCode::OK,
    )
    .await;
    for (id, state) in [("n1", "accepted"), ("n4", "rejected")] {
        let uri = format!("{base}/nodes/{id}");
        let body = json!({ "state": state });
        send(
            &e,
            Method::PUT,
            &uri,
            &format!("st-{id}"),
            body,
            StatusCode::OK,
        )
        .await;
    }
    g("structure-COAL", &get(&e, base).await);
}

#[tokio::test]
async fn the_ticker_page_header_reads_from_the_r2_to_r4_modules() {
    let e = canon_env();
    populate(&e).await;
    let coal = get(&e, "/api/research/COAL").await;
    assert_eq!(coal["name"], "PT Batubara Contoh Tbk");
    assert_eq!(coal["role"], "held");
    assert_eq!(coal["avg_price"], 1_000.0);
    assert_eq!(coal["pack"]["id"], "coal_mining");
    assert_eq!(coal["pack"]["source"], "filing");
    assert!(coal["pack"]["action"].is_null());
    assert_eq!(coal["latest_filing"]["fy"], 2026);
    assert_eq!(coal["latest_filing"]["period"], "Q2");
    assert_eq!(coal["latest_close"]["date"], "2026-10-27");
    assert_eq!(coal["latest_close"]["age_days"], 1);
    assert_eq!(coal["seen_at"], NOW);
    // Claims in all four statuses across the list; the header counts COAL's.
    assert_eq!(
        coal["claims"],
        json!({"holding": 2, "broken": 1, "not_testable": 1, "expired": 1})
    );
    // The coverage object is the one `/pack` returns.
    let pack = get(&e, "/api/research/COAL/pack").await;
    assert_eq!(coal["coverage"], pack["coverage"]);
    // Changes since the visit, newest first, with a section and the words of the inbox.
    let changes = coal["changes"].as_array().unwrap();
    assert!(!changes.is_empty() && changes.len() <= 50);
    assert!(changes
        .windows(2)
        .all(|w| w[0]["at"].as_str() >= w[1]["at"].as_str()));
    assert!(changes
        .iter()
        .all(|c| c["section"].is_string() && c["text"].is_string()));

    // No pack: the action, a null name, no seen time, a 90-day window.
    let none = get(&e, "/api/research/NONE").await;
    assert!(none["name"].is_null());
    assert!(none["pack"]["id"].is_null());
    assert_eq!(none["pack"]["action"], "Tentukan paket");
    assert!(none["seen_at"].is_null());
    // A peer carries the ticker it belongs to.
    let peer = get(&e, "/api/research/COPR").await;
    assert_eq!(peer["role"], "peer");
    assert_eq!(peer["peer_of"], "COAL");
    assert!(peer["avg_price"].is_null());
    // Unknown and malformed tickers.
    for uri in ["/api/research/ZZZZ", "/api/research/abc"] {
        assert_eq!(e.get(uri).await.0, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn compare_uses_the_pack_evaluator_and_marks_what_a_pack_lacks() {
    let e = canon_env();
    populate(&e).await;
    let table = get(&e, "/api/research/compare?pack=coal_mining").await;
    assert_eq!(table["pack"]["id"], "coal_mining");
    let tickers: Vec<&str> = table["tickers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["ticker"].as_str().unwrap())
        .collect();
    assert_eq!(
        tickers,
        ["COAL", "COPR"],
        "members of the pack, then their peers"
    );
    let rows = table["rows"].as_array().unwrap();
    // Core rows first, then the pack's dimensions.
    let first_pack = rows.iter().position(|r| r["group"] != "core").unwrap();
    assert!(first_pack > 0 && rows[..first_pack].iter().all(|r| r["group"] == "core"));
    assert!(rows[first_pack..].iter().all(|r| r["group"] != "core"));

    // The same value as the pack section shows for the ticker.
    let pack = get(&e, "/api/research/COAL/pack").await;
    let metric = |id: &str| {
        pack["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["metrics"].as_array().unwrap())
            .find(|m| m["id"] == id)
            .unwrap()
            .clone()
    };
    for row in rows {
        let id = row["metric"]["id"].as_str().unwrap();
        let cell = &row["cells"][0];
        assert_eq!(cell["ticker"], "COAL");
        if row["metric"]["layer"] == "xbrl" && cell["state"] == "value" {
            let m = metric(id);
            assert_eq!(cell["value"], m["latest"]["value"], "{id}");
            assert_eq!(cell["period"], m["latest"]["label"], "{id}");
            assert_eq!(cell["percentile"], m["percentile"]["value"], "{id}");
        }
    }
    // A single-element metric carries its fact; a ratio carries none.
    let single = rows
        .iter()
        .find(|r| r["cells"][0]["fact"].is_object())
        .expect("a metric with one plain input");
    assert!(single["cells"][0]["fact"]["filing_id"].is_string());
    assert!(single["cells"][0]["fact"]["element"].is_string());
    let ratio = rows
        .iter()
        .find(|r| r["metric"]["unit"] == "%" && r["cells"][0]["state"] == "value")
        .unwrap();
    assert!(ratio["cells"][0]["fact"].is_null());
    // KPIs: entered, explicit unknown and never entered are three different states.
    let state = |id: &str, i: usize| {
        rows.iter().find(|r| r["metric"]["id"] == id).unwrap()["cells"][i]["state"].clone()
    };
    assert_eq!(state("production_mt", 0), "value");
    assert_eq!(state("sales_mt", 0), "unknown");
    assert_eq!(state("asp_usd_t", 0), "gap");
    assert_eq!(state("production_mt", 1), "gap", "the peer entered nothing");

    // A ticker whose pack lacks the metric gets a gap, not a missing cell.
    let mixed = get(
        &e,
        "/api/research/compare?pack=coal_mining&tickers=COAL,BANK,NONE",
    )
    .await;
    let mixed_rows = mixed["rows"].as_array().unwrap();
    for row in mixed_rows.iter().filter(|r| r["group"] != "core") {
        for i in [1, 2] {
            assert_eq!(row["cells"][i]["state"], "gap");
            assert!(row["cells"][i]["value"].is_null());
        }
    }
    assert!(mixed_rows
        .iter()
        .any(|r| r["group"] == "core" && r["cells"][2]["state"] == "value"));

    // `ticker` alone resolves the pack in effect; no pack means the core rows only.
    let by_ticker = get(&e, "/api/research/compare?ticker=BANK").await;
    assert_eq!(by_ticker["pack"]["id"], "bank_sharia");
    assert_eq!(by_ticker["tickers"][0]["ticker"], "BANK");
    let none = get(&e, "/api/research/compare?ticker=NONE").await;
    assert!(none["pack"].is_null());
    assert!(none["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["group"] == "core"));
    let core = get(&e, "/api/research/compare?pack=core").await;
    assert_eq!(core["pack"]["id"], "core");
    assert_eq!(core["tickers"].as_array().unwrap().len(), 6);

    // Errors: unknown pack or ticker 404, malformed list 422, nothing to go on 422.
    for uri in [
        "/api/research/compare?pack=nope",
        "/api/research/compare?ticker=ZZZZ",
        "/api/research/compare?pack=coal_mining&tickers=COAL,ZZZZ",
    ] {
        assert_eq!(e.get(uri).await.0, StatusCode::NOT_FOUND, "{uri}");
    }
    let many = [
        "AAAA", "BBBB", "CCCC", "DDDD", "EEEE", "FFFF", "GGGG", "HHHH", "IIII", "JJJJ", "KKKK",
        "LLLL", "MMMM",
    ]
    .join(",");
    for uri in [
        "/api/research/compare?pack=core&tickers=coal".to_string(),
        "/api/research/compare?pack=core&tickers=COAL,".to_string(),
        format!("/api/research/compare?pack=core&tickers={many}"),
        "/api/research/compare".to_string(),
    ] {
        let (s, v) = e.get(&uri).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {v}");
    }
}

#[tokio::test]
async fn the_desk_filters_every_list_to_one_ticker() {
    let e = canon_env();
    populate(&e).await;
    let all = get(&e, "/api/research/desk").await;
    let one = get(&e, "/api/research/desk?ticker=COAL").await;
    let inbox = one["inbox"].as_array().unwrap();
    assert!(!inbox.is_empty() && inbox.len() < all["inbox"].as_array().unwrap().len());
    assert!(inbox.iter().all(|i| i["ticker"] == "COAL"));
    assert_eq!(one["inbox_total"], inbox.len());
    assert_eq!(one["filters"][0]["count"], inbox.len());
    let coverage = one["coverage"].as_array().unwrap();
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0]["ticker"], "COAL");
    assert!(one["calendar"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["ticker"] == "COAL"));
    let ages: Vec<&String> = one["ages"]["prices"].as_object().unwrap().keys().collect();
    assert_eq!(ages, ["COAL"]);
    assert_eq!(
        e.get("/api/research/desk?ticker=ZZZZ").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn compare_list_rules_and_empty_parameters() {
    let e = canon_env();
    populate(&e).await;
    let order = |v: &Value| -> Vec<String> {
        v["tickers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["ticker"].as_str().unwrap().to_string())
            .collect()
    };
    // Order is kept and duplicates dropped; `ticker` leads a list that omits it.
    let v = get(&e, "/api/research/compare?pack=core&tickers=BANK,COAL,BANK").await;
    assert_eq!(order(&v), ["BANK", "COAL"]);
    let v = get(&e, "/api/research/compare?ticker=COAL&tickers=BANK,REST").await;
    assert_eq!(order(&v), ["COAL", "BANK", "REST"]);
    assert_eq!(v["pack"]["id"], "coal_mining");
    // `pack` wins over the pack of `ticker`; empty values count as absent.
    let v = get(&e, "/api/research/compare?pack=bank_sharia&ticker=COAL").await;
    assert_eq!(v["pack"]["id"], "bank_sharia");
    let v = get(&e, "/api/research/compare?pack=&ticker=BANK&tickers=").await;
    assert_eq!(v["pack"]["id"], "bank_sharia");
    assert_eq!(v["truncated"], false);
    // A cell says why it has no value; a pack the ticker is not on is "not_defined".
    let v = get(
        &e,
        "/api/research/compare?pack=coal_mining&tickers=COAL,BANK",
    )
    .await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["metric"]["id"] == "asp_usd_t")
        .unwrap();
    assert_eq!(row["cells"][0]["reason"], "not_entered");
    assert_eq!(row["cells"][1]["reason"], "not_defined");
    // A standalone quarter rests on year-to-date facts: no single fact. A balance-sheet value has one.
    let revenue = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["metric"]["id"] == "revenue")
        .unwrap();
    assert!(revenue["cells"][0]["fact"].is_null());
    let equity = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["metric"]["id"] == "total_equity")
        .unwrap();
    assert!(equity["cells"][0]["fact"]["filing_id"].is_string());
    // The desk: an empty ticker is no filter, a misspelled parameter is refused.
    let all = get(&e, "/api/research/desk?ticker=").await;
    assert_eq!(all["coverage"].as_array().unwrap().len(), 6);
    assert_eq!(
        e.get("/api/research/desk?tickers=COAL").await.0,
        StatusCode::BAD_REQUEST
    );
}
