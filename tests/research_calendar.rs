//! Calendar at API level (research plan §6 R4.4; design.md §4.1 block 4): POJK reporting
//! windows from the rules file, dividend dates from `corp_events` with projections labelled
//! as such, and the owner's own dates, written through the log. The window arithmetic itself
//! is unit-tested in `calendar.rs`.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::{Method, StatusCode};
use finance_app::research::store::{CorpEventRow, EventKind};
use r4_env::{call, env};
use serde_json::{json, Value};

fn dividend(ticker: &str, date: &str, amount: f64) -> CorpEventRow {
    CorpEventRow {
        ticker: ticker.into(),
        date: date.into(),
        kind: EventKind::Dividend,
        amount: Some(amount),
        ratio_num: None,
        ratio_den: None,
    }
}

fn labels(v: &Value) -> Vec<(String, String)> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["date"].as_str().unwrap().to_string(),
                i["label"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_unfiled_pojk_window_is_listed_with_its_deadline() {
    let e = env();
    let (s, v) = e.get("/api/research/COAL/calendar").await;
    assert_eq!(s, StatusCode::OK);
    // Filings run to 2026Q2: the Q3 report is due 30 days after 2026-09-30.
    assert_eq!(
        labels(&v),
        [("2026-10-30".to_string(), "Batas laporan 2026Q3".to_string())]
    );
    let item = &v["items"][0];
    assert_eq!(item["kind"], "pojk");
    assert_eq!(item["source"], "pojk");
    assert_eq!(
        item["window"],
        json!({"start": "2026-10-01", "end": "2026-10-30"})
    );
    assert_eq!(item["overdue"], false);
    assert_eq!(item["projected"], false);

    // Two weeks later, still unfiled: overdue, still listed.
    e.set_clock("2026-11-13T03:00:00+00:00");
    let (_, v) = e.get("/api/research/COAL/calendar").await;
    assert_eq!(v["items"][0]["overdue"], true);
    // A short horizon hides a window further out; the days parameter is bounded.
    e.set_clock("2026-10-01T03:00:00+00:00");
    let (_, v) = e.get("/api/research/COAL/calendar?days=10").await;
    assert_eq!(v["items"], json!([]));
    let (_, v) = e.get("/api/research/COAL/calendar?days=9999").await;
    assert_eq!(v["days"], 366);
    let (s, _) = e.get("/api/research/ZZZZ/calendar").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dividend_dates_come_from_corp_events_and_projections_are_labelled() {
    let e = env();
    e.store
        .upsert_events(&[
            dividend("COAL", "2025-11-20", 12.0),
            dividend("COAL", "2026-09-28", 15.0),
            dividend("BANK", "2026-10-12", 7.0),
        ])
        .unwrap();
    let (_, v) = e.get("/api/research/COAL/calendar").await;
    let items = v["items"].as_array().unwrap();
    let reported = items
        .iter()
        .find(|i| i["date"] == "2026-09-28")
        .expect("a recent ex-date stays listed");
    assert_eq!(reported["kind"], "dividend");
    assert_eq!(reported["source"], "corp_events");
    assert_eq!(reported["projected"], false);
    assert_eq!(reported["amount"], 15.0);
    let projected = items
        .iter()
        .find(|i| i["projected"] == true)
        .expect("last year's date, one year on");
    assert_eq!(projected["date"], "2026-11-20");
    assert_eq!(projected["label"], "Perkiraan ex dividen (tahun lalu)");
    assert_eq!(projected["note"], "dari tanggal 2025-11-20");
    // Another ticker's dividends are not mixed in.
    assert!(items.iter().all(|i| i["ticker"] == "COAL"));
}

#[tokio::test]
async fn owner_dates_are_added_replaced_and_removed_through_the_log() {
    let e = env();
    let uri = "/api/research/COAL/calendar";
    let (s, row) = e
        .post(
            uri,
            "m1",
            json!({"date": "2026-11-15", "kind": "rups", "note": "  RUPS luar biasa  "}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(
        row,
        json!({"ticker": "COAL", "date": "2026-11-15", "kind": "rups", "note": "RUPS luar biasa"})
    );
    let (_, v) = e.get(uri).await;
    let manual: Vec<&Value> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["source"] == "manual")
        .collect();
    assert_eq!(manual.len(), 1);
    assert_eq!(manual[0]["label"], "RUPS");
    assert_eq!(manual[0]["note"], "RUPS luar biasa");

    // The same date and kind replaces the note, not the row.
    e.post(
        uri,
        "m2",
        json!({"date": "2026-11-15", "kind": "rups", "note": "diundur"}),
    )
    .await;
    let rows = e.store.calendar_manual(Some("COAL")).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].note.as_deref(), Some("diundur"));

    // It survives a rebuild from the log.
    e.store.rebuild().unwrap();
    assert_eq!(e.store.calendar_manual(Some("COAL")).unwrap(), rows);

    let bad: Vec<(&str, Value)> = vec![
        ("date", json!({"date": "bulan depan", "kind": "rups"})),
        ("date", json!({"kind": "rups"})),
        ("kind", json!({"date": "2026-11-15", "kind": "beli"})),
        (
            "note",
            json!({"date": "2026-11-15", "kind": "rups", "note": "x".repeat(201)}),
        ),
    ];
    for (i, (field, body)) in bad.into_iter().enumerate() {
        let (s, v) = e.post(uri, &format!("bad{i}"), body).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {v}");
        assert_eq!(v["detail"][0]["loc"][1], field);
    }
    let (s, _) = e
        .post(
            "/api/research/ZZZZ/calendar",
            "z",
            json!({"date": "2026-11-15", "kind": "rups"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let del = format!("{uri}/2026-11-15/rups");
    let (s, _, _) = call(&e.app, Method::DELETE, &del, Some("d1"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = call(&e.app, Method::DELETE, &del, Some("d2"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(e.store.calendar_manual(Some("COAL")).unwrap().is_empty());
    e.store.rebuild().unwrap();
    assert!(e.store.calendar_manual(Some("COAL")).unwrap().is_empty());
}
