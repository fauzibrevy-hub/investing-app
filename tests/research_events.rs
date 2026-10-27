//! Inbox and Desk at API level (research plan §6 R4.3; prd.md A4.3): each producer (new
//! filing, claim status change, KPI due, journal level crossed, restatement, stale data,
//! hard accounting flag) stores one event per fact, dismissals are logged and survive a
//! rebuild, "seen" is logged, and `GET /desk` returns `{inbox, coverage, calendar, ages}`.
//! The producers themselves are unit-tested in `events/producers.rs`.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::{Method, StatusCode};
use investing_app::research::events::react::{react, Scope};
use investing_app::research::store::PriceBar;
use investing_app::research::universe::{add, AddRequest};
use investing_app::research::valuation::PojkOffsets;
use investing_app::research::xbrl;
use r4_env::{call, env, Env};
use serde_json::{json, Value};
use xbrl_fixture::{general_year, Year};

fn offsets() -> PojkOffsets {
    PojkOffsets {
        q1: 30,
        q2: 60,
        q3: 30,
        fy: 90,
    }
}

fn run(e: &Env) -> usize {
    react(
        &e.store,
        &e.pack_set(),
        &offsets(),
        e.now(),
        e.today(),
        Scope::All,
    )
    .unwrap()
    .added
}

async fn inbox(e: &Env) -> Vec<Value> {
    let (s, v) = e.get("/api/research/desk").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["inbox"].as_array().unwrap().clone()
}

fn of_kind<'a>(items: &'a [Value], kind: &str) -> Vec<&'a Value> {
    items.iter().filter(|i| i["kind"] == kind).collect()
}

fn bar(date: &str, close: f64) -> PriceBar {
    PriceBar {
        date: date.into(),
        open: None,
        high: None,
        low: None,
        close,
        volume: None,
    }
}

fn texts(items: &[&Value]) -> Vec<String> {
    items
        .iter()
        .map(|i| i["text"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn new_filings_and_due_kpis_are_reported_once_and_a_repeat_stores_nothing() {
    let e = env();
    assert!(run(&e) > 0);
    let items = inbox(&e).await;
    // The newest filing of each ticker, not the 18 that arrived with it.
    let filings = of_kind(&items, "filing_new");
    let mut tickers: Vec<&str> = filings
        .iter()
        .map(|i| i["ticker"].as_str().unwrap())
        .collect();
    tickers.sort_unstable();
    assert_eq!(tickers, ["BANK", "COAL", "NONE"]);
    assert_eq!(texts(&filings)[0], "Laporan Q2 26 masuk (belum diaudit)");
    assert_eq!(filings[0]["group"], "laporan");
    assert_eq!(filings[0]["section"], "financials");
    assert_eq!(filings[0]["payload"]["label"], "2026Q2");

    // KPIs of 2026Q2 came due 14 days after the filing became available (2026-09-12).
    let due = of_kind(&items, "kpi_due");
    let due_tickers: Vec<&str> = due.iter().map(|i| i["ticker"].as_str().unwrap()).collect();
    assert!(
        due_tickers.contains(&"COAL") && due_tickers.contains(&"BANK"),
        "{due_tickers:?}"
    );
    let coal = due.iter().find(|i| i["ticker"] == "COAL").unwrap();
    assert_eq!(coal["group"], "kpi");
    assert_eq!(coal["section"], "pack");
    assert!(coal["text"]
        .as_str()
        .unwrap()
        .starts_with("KPI Q2 26 belum diisi: "));
    assert!(coal["payload"]["missing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "Produksi"));
    assert_eq!(coal["payload"]["due"], "2026-09-12");
    // Nothing else fires on this data.
    for quiet in [
        "stale_data",
        "accounting_flag",
        "restatement",
        "level_crossed",
        "claim_status",
    ] {
        assert!(of_kind(&items, quiet).is_empty(), "{quiet}");
    }
    // Running again, also for a single ticker, stores nothing new.
    assert_eq!(run(&e), 0);
    let again = react(
        &e.store,
        &e.pack_set(),
        &offsets(),
        e.now(),
        e.today(),
        Scope::Ticker("COAL"),
    )
    .unwrap();
    assert_eq!(again.added, 0);
    assert_eq!(inbox(&e).await.len(), items.len());
}

#[tokio::test]
async fn a_kpi_is_not_due_before_its_grace_period_and_stops_when_answered() {
    let e = env();
    // Two days after the 2026Q2 filing became available: nothing is due yet.
    e.set_clock("2026-08-31T03:00:00+00:00");
    run(&e);
    assert!(of_kind(&inbox(&e).await, "kpi_due").is_empty());
    // After the grace period, with a KPI answered as "unknown" the rest is still listed.
    e.set_clock("2026-09-15T03:00:00+00:00");
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "k1",
            json!({"unknown": true, "document": "belum dilaporkan", "document_date": "2026-09-10"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    run(&e);
    let items = inbox(&e).await;
    let coal = of_kind(&items, "kpi_due")
        .into_iter()
        .find(|i| i["ticker"] == "COAL")
        .unwrap()
        .clone();
    let missing = coal["payload"]["missing"].as_array().unwrap();
    assert!(
        !missing.iter().any(|m| m == "Produksi"),
        "answered: {missing:?}"
    );
    assert!(missing.iter().any(|m| m == "Penjualan"));
}

#[tokio::test]
async fn a_claim_changing_status_reaches_the_inbox_with_both_words() {
    let e = env();
    let (s, c) = e
        .post(
            "/api/research/COAL/claims",
            "c1",
            json!({
                "statement": "Produksi minimal lima", "metric": "coal_mining.production_mt",
                "comparator": ">=", "threshold": 5, "window": "latest", "due": "2026-12-31",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(c["status_label"], "Belum teruji");
    // Entering the KPI re-evaluates the claim in the same request and the event follows.
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "k1",
            json!({"value": 6, "document": "laporan operasional sintetis", "document_date": "2026-09-10"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let items = inbox(&e).await;
    let changes = of_kind(&items, "claim_status");
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0]["text"],
        "Klaim \"Produksi minimal lima\": Belum teruji → Berlaku (nilai 6)"
    );
    assert_eq!(changes[0]["group"], "klaim");
    assert_eq!(changes[0]["section"], "claims");
    assert_eq!(changes[0]["ticker"], "COAL");
    // The same status on the next pass is not a change.
    run(&e);
    assert_eq!(of_kind(&inbox(&e).await, "claim_status").len(), 1);
    // A value that breaks the claim is a second event.
    e.set_clock("2026-10-25T03:00:00+00:00");
    e.send(
        Method::PUT,
        "/api/research/COAL/kpi/production_mt/2026/2",
        "k2",
        json!({"value": 4, "document": "laporan operasional sintetis", "document_date": "2026-09-10"}),
    )
    .await;
    let items = inbox(&e).await;
    let texts = texts(&of_kind(&items, "claim_status"));
    assert_eq!(texts.len(), 2);
    assert!(
        texts.contains(&"Klaim \"Produksi minimal lima\": Berlaku → Patah (nilai 4)".to_string())
    );
}

#[tokio::test]
async fn a_journal_level_crossed_by_a_close_is_reported_in_each_direction() {
    let e = env();
    let (s, _) = e
        .post(
            "/api/research/COAL/journal",
            "l1",
            json!({"kind": "level", "text": "Harga incaran", "level": 1_500.0}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    // The level was written on 2026-10-24; nothing crossed it yet.
    run(&e);
    assert!(of_kind(&inbox(&e).await, "level_crossed").is_empty());
    e.set_clock("2026-10-28T03:00:00+00:00");
    e.store
        .upsert_prices(
            "COAL",
            &[bar("2026-10-26", 1_600.0), bar("2026-10-27", 1_400.0)],
        )
        .unwrap();
    run(&e);
    let items = inbox(&e).await;
    let crossed = of_kind(&items, "level_crossed");
    let mut got = texts(&crossed);
    got.sort();
    assert_eq!(
        got,
        [
            "Harga 1.400 turun melewati level 1.500 (\"Harga incaran\")",
            "Harga 1.600 naik melewati level 1.500 (\"Harga incaran\")",
        ]
    );
    assert!(crossed
        .iter()
        .all(|i| i["group"] == "level" && i["section"] == "price"));
    run(&e);
    assert_eq!(of_kind(&inbox(&e).await, "level_crossed").len(), 2);
}

/// Two more tickers built by writing synthetic filings and ingesting them through the real
/// parser and derivation: `REST` has a later filing that restates an earlier comparative,
/// `BUST` ends 2025 with losses in the last two quarters and negative equity.
fn ingest_extras(e: &Env) {
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
    let dir = e.dir.path().join("XBRL-extra");
    general_year(&dir, "REST", &Y2024, None, &[], None);
    // The 2025 Q2 filing restates the Q2 2024 comparative revenue (230 → 237 million).
    general_year(
        &dir,
        "REST",
        &Y2025,
        Some(&Y2024),
        &["Q3", "FY"],
        Some((1, 237.0)),
    );
    general_year(&dir, "BUST", &BUST, None, &[], None);
    let report = xbrl::ingest_new(&e.store, &dir, e.now()).unwrap();
    assert!(report.unparsed.is_empty(), "{:?}", report.unparsed);
    for t in ["REST", "BUST"] {
        add(
            &e.store,
            &e.pack_set(),
            &AddRequest {
                ticker: t.into(),
                ..Default::default()
            },
            r4_env::NOW,
        )
        .unwrap();
    }
}

#[tokio::test]
async fn a_restatement_and_hard_accounting_flags_come_from_ingested_filings() {
    let e = env();
    ingest_extras(&e);
    run(&e);
    let items = inbox(&e).await;
    let restated = of_kind(&items, "restatement");
    assert_eq!(restated.len(), 1);
    assert_eq!(restated[0]["ticker"], "REST");
    assert_eq!(
        restated[0]["text"],
        "SalesAndRevenue 2024-06-30 disajikan kembali: 230 juta → 237 juta"
    );
    assert_eq!(restated[0]["group"], "data");
    let flags = of_kind(&items, "accounting_flag");
    let mut got = texts(&flags);
    got.sort();
    assert_eq!(
        got,
        [
            "Ekuitas negatif pada Q4 25: −40 juta rupiah",
            "Rugi bersih dua kuartal berturut-turut: Q3 25 −8 juta rupiah, Q4 25 −10 juta rupiah",
        ]
    );
    assert!(flags
        .iter()
        .all(|i| i["group"] == "bendera" && i["ticker"] == "BUST"));
    // Healthy tickers raise no flag.
    assert!(flags.iter().all(|i| i["ticker"] != "REST"));
    // Facts with numbers, no verdict vocabulary in what the system says (KPI-due lines are
    // left out: they list pack metric names such as "Harga jual rata-rata", not status words).
    let own: Vec<&Value> = items.iter().filter(|i| i["kind"] != "kpi_due").collect();
    let all = json!(own).to_string().to_lowercase();
    let verdicts =
        regex::Regex::new(r"\b(beli|jual|murah|mahal|layak|wajar|coret|fair)\b").unwrap();
    assert!(
        !verdicts.is_match(&all),
        "verdict word in the inbox: {:?}",
        verdicts
            .find(&all)
            .map(|m| &all[m.start().saturating_sub(40)..m.end() + 40])
    );
}

#[tokio::test]
async fn stale_prices_and_an_overdue_report_make_a_ticker_basi() {
    let e = env();
    e.set_clock("2026-11-20T03:00:00+00:00");
    run(&e);
    let (_, desk) = e.get("/api/research/desk").await;
    let items = desk["inbox"].as_array().unwrap();
    let stale = of_kind(items, "stale_data");
    // Three tickers, each with old prices and a 2026Q3 report that never came.
    assert_eq!(stale.len(), 6);
    let mut got = texts(&stale);
    got.sort();
    assert!(got.contains(&"Harga terakhir 2026-10-23, 28 hari lalu".to_string()));
    assert!(got.contains(&"Laporan Q3 26 belum masuk; batas 2026-10-30, lewat 21 hari".to_string()));
    assert!(stale.iter().all(|i| i["group"] == "data"));
    let sections: Vec<&str> = stale
        .iter()
        .map(|i| i["section"].as_str().unwrap())
        .collect();
    assert!(sections.contains(&"price") && sections.contains(&"sources"));
    for row in desk["coverage"].as_array().unwrap() {
        assert_eq!(row["data"]["state"], "basi", "{row}");
        assert_eq!(row["data"]["label"], "Basi");
        assert_eq!(row["data"]["reasons"].as_array().unwrap().len(), 2);
    }
    assert_eq!(desk["ages"]["prices"]["COAL"], 28);
    // Fresh prices and the report within its grace: not stale.
    let e2 = env();
    run(&e2);
    let (_, desk) = e2.get("/api/research/desk").await;
    assert!(of_kind(desk["inbox"].as_array().unwrap(), "stale_data").is_empty());
    assert_ne!(desk["coverage"][0]["data"]["state"], "basi");
}

#[tokio::test]
async fn a_dismissal_is_logged_survives_a_rebuild_and_is_never_produced_again() {
    let e = env();
    run(&e);
    let items = inbox(&e).await;
    let total = items.len();
    let target = of_kind(&items, "filing_new")[0].clone();
    let id = target["id"].as_str().unwrap();
    let uri = format!("/api/research/events/{id}/dismiss");

    let (s, v) = e.post(&uri, "d1", json!({})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["dismissed_at"], "2026-10-24T09:00:00+00:00");
    assert_eq!(inbox(&e).await.len(), total - 1);
    // A replay with the same key answers the same; another key finds it already done.
    let (s, again, headers) = call(&e.app, Method::POST, &uri, Some("d1"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again, v);
    assert_eq!(headers["idempotent-replay"], "true");
    let (s, _) = e.post(&uri, "d2", json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = e
        .post(
            "/api/research/events/filing_new-abcdefabcdefabcd/dismiss",
            "d3",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // The producer meets the same fact again and leaves the dismissal alone.
    assert_eq!(run(&e), 0);
    assert_eq!(inbox(&e).await.len(), total - 1);
    // The dismissal is in the log: a rebuild restores events and dismissals alike.
    let before = e.store.events_list(false).unwrap();
    e.store.rebuild().unwrap();
    assert_eq!(e.store.events_list(false).unwrap(), before);
    assert_eq!(inbox(&e).await.len(), total - 1);
    let row = e.store.event(id).unwrap().unwrap();
    assert_eq!(
        row.dismissed_at.as_deref(),
        Some("2026-10-24T09:00:00+00:00")
    );
}

#[tokio::test]
async fn seen_is_logged_and_shows_on_the_coverage_board() {
    let e = env();
    let (_, desk) = e.get("/api/research/desk").await;
    assert!(desk["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["last_seen"].is_null()));
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/seen",
        Some("s1"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/ZZZZ/seen",
        Some("s2"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, desk) = e.get("/api/research/desk").await;
    let coal = desk["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ticker"] == "COAL")
        .unwrap();
    assert_eq!(coal["last_seen"], "2026-10-24T09:00:00+00:00");
    e.store.rebuild().unwrap();
    assert!(e.store.seen_all().unwrap().contains_key("COAL"));
}

#[tokio::test]
async fn the_desk_payload_has_inbox_coverage_calendar_and_ages() {
    let e = env();
    run(&e);
    e.post(
        "/api/research/COAL/claims",
        "c1",
        json!({
            "statement": "Harga di atas seribu", "metric": "price.close", "comparator": ">=",
            "threshold": 1000, "window": "latest", "due": "2026-12-31",
        }),
    )
    .await;
    e.post(
        "/api/research/COAL/claims",
        "c2",
        json!({
            "statement": "Penjualan minimal lima", "metric": "coal_mining.sales_mt",
            "comparator": ">=", "threshold": 5, "window": "latest", "due": "2026-12-31",
        }),
    )
    .await;
    e.store
        .upsert_drivers(
            "bi_rate",
            "test",
            "2026-10-24T00:00:00+00:00",
            &[("2026-10-20".to_string(), 4.75)],
        )
        .unwrap();
    let (s, desk) = e.get("/api/research/desk").await;
    assert_eq!(s, StatusCode::OK);
    // The footer names a driver by its catalogue label, not its id.
    assert_eq!(desk["ages"]["driver_labels"]["bi_rate"], "BI rate");
    assert!(desk["ages"]["drivers"]["bi_rate"].is_i64());
    for key in [
        "inbox",
        "coverage",
        "calendar",
        "ages",
        "filters",
        "inbox_total",
    ] {
        assert!(desk.get(key).is_some(), "{key}");
    }
    // Held first, then by ticker.
    let order: Vec<&str> = desk["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["ticker"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["BANK", "COAL", "NONE"]);
    let coal = &desk["coverage"][1];
    assert_eq!(coal["role"], "held");
    assert_eq!(coal["pack"]["id"], "coal_mining");
    assert_eq!(
        coal["claims"],
        json!({"holding": 1, "broken": 0, "not_testable": 1, "expired": 0})
    );
    assert!(coal["xbrl"]["pct"].as_f64().unwrap() > 0.0);
    assert!(!coal["kpi"]["missing"].as_array().unwrap().is_empty());
    assert_eq!(
        coal["next_report"],
        json!({"label": "2026Q3", "deadline": "2026-10-30", "overdue": false})
    );
    assert_eq!(coal["data"]["state"], "sebagian");
    assert!(desk["coverage"][2]["pack"]["id"].is_null());
    assert_eq!(desk["coverage"][2]["pack"]["action"], "Tentukan paket");
    // The calendar mixes tickers, sorted by date.
    let cal = desk["calendar"].as_array().unwrap();
    assert_eq!(cal.len(), 3);
    assert!(cal
        .iter()
        .all(|i| i["date"] == "2026-10-30" && i["kind"] == "pojk"));
    // Ages per ticker and driver.
    assert_eq!(desk["ages"]["prices"]["BANK"], 1);
    assert!(desk["ages"]["filings"]["COAL"].as_i64().unwrap() > 0);
    // Filter chips carry counts and the Indonesian labels.
    let filters = desk["filters"].as_array().unwrap();
    let labels: Vec<&str> = filters
        .iter()
        .map(|f| f["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Semua",
            "Laporan baru",
            "Klaim",
            "KPI",
            "Level",
            "Bendera",
            "Data"
        ]
    );
    assert_eq!(filters[0]["count"], desk["inbox_total"]);
    assert_eq!(filters[1]["count"], 3);
    // Newest first.
    let ats: Vec<&str> = desk["inbox"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["at"].as_str().unwrap())
        .collect();
    let mut sorted = ats.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(ats, sorted);
}

#[tokio::test]
async fn kpi_and_stale_events_are_refreshed_and_retired_when_their_condition_clears() {
    let e = env();
    run(&e);
    let due = |items: &[Value]| {
        of_kind(items, "kpi_due")
            .into_iter()
            .find(|i| i["ticker"] == "COAL")
            .cloned()
    };
    let before = due(&inbox(&e).await).expect("COAL has KPIs due");
    let missing_before = before["payload"]["missing"].as_array().unwrap().len();
    assert!(missing_before >= 2);

    // The owner answers one KPI of the quarter: the open event now lists one fewer.
    e.send(
        Method::PUT,
        "/api/research/COAL/kpi/production_mt/2026/2",
        "k1",
        json!({"value": 6, "document": "laporan operasional sintetis", "document_date": "2026-09-10"}),
    )
    .await;
    let after = due(&inbox(&e).await).unwrap();
    assert_eq!(after["id"], before["id"], "same event, refreshed");
    assert_eq!(
        after["payload"]["missing"].as_array().unwrap().len(),
        missing_before - 1
    );
    assert!(!after["text"].as_str().unwrap().contains("Produksi"));

    // Every quarterly KPI answered: the event goes away without a manual dismissal.
    let packs = e.pack_set();
    let coal = packs.get("coal_mining").unwrap();
    for (i, m) in packs
        .core()
        .kpi_metrics()
        .chain(coal.kpi_metrics())
        .filter(|m| m.cadence != Some(investing_app::research::packs::Cadence::Annual))
        .enumerate()
    {
        e.send(
            Method::PUT,
            &format!("/api/research/COAL/kpi/{}/2026/2", m.id),
            &format!("all{i}"),
            json!({"unknown": true, "document": "belum dilaporkan", "document_date": "2026-09-10"}),
        )
        .await;
    }
    assert!(due(&inbox(&e).await).is_none());
    // Not produced again by the next pass either.
    run(&e);
    assert!(due(&inbox(&e).await).is_none());

    // Stale prices: a fresh close ends the episode.
    e.set_clock("2026-11-20T03:00:00+00:00");
    run(&e);
    let stale_prices = |items: &[Value]| {
        of_kind(items, "stale_data")
            .into_iter()
            .filter(|i| i["ticker"] == "COAL" && i["payload"]["source"] == "prices")
            .count()
    };
    assert_eq!(stale_prices(&inbox(&e).await), 1);
    e.store
        .upsert_prices("COAL", &[bar("2026-11-19", 1_300.0)])
        .unwrap();
    run(&e);
    assert_eq!(stale_prices(&inbox(&e).await), 0);
    // The overdue report is still overdue.
    assert!(of_kind(&inbox(&e).await, "stale_data")
        .iter()
        .any(|i| i["ticker"] == "COAL" && i["payload"]["source"] == "filing"));
}

#[tokio::test]
async fn levels_and_their_crossings_follow_a_split_after_they_were_written() {
    use investing_app::research::store::{CorpEventRow, EventKind};
    let e = env();
    let (s, _) = e
        .post(
            "/api/research/COAL/journal",
            "l1",
            json!({"kind": "level", "text": "Harga incaran", "level": 1_500.0}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    e.store
        .upsert_events(&[CorpEventRow {
            ticker: "COAL".into(),
            date: "2026-10-30".into(),
            kind: EventKind::Split,
            amount: None,
            ratio_num: Some(2.0),
            ratio_den: Some(1.0),
        }])
        .unwrap();
    // After the split the closes are on the new basis: 1500 is 750, and a close of 700
    // after one of 800 crosses it downwards.
    e.set_clock("2026-11-04T03:00:00+00:00");
    e.store
        .upsert_prices(
            "COAL",
            &[bar("2026-11-02", 800.0), bar("2026-11-03", 700.0)],
        )
        .unwrap();
    run(&e);
    let items = inbox(&e).await;
    let crossed = of_kind(&items, "level_crossed");
    assert_eq!(
        texts(&crossed),
        ["Harga 700 turun melewati level 750 (\"Harga incaran\")"]
    );
    let (_, v) = e.get("/api/research/COAL/prices?range=1y").await;
    assert_eq!(v["levels"][0]["level"], 750.0);
    assert_eq!(v["levels"][0]["entered"], 1_500.0);
}
