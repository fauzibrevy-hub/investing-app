//! Journal at API level (research plan §6 R4.2; prd.md A4.2; design.md §3.5): entries are
//! append-only, the price is stamped from the latest close when the entry is saved,
//! annotations live apart, there is no route that edits or deletes an entry, and journal
//! levels appear in the prices payload.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::{Method, StatusCode};
use finance_app::research::store::{JournalRow, PriceBar};
use r4_env::{call, env, Env};
use serde_json::{json, Value};

async fn add(e: &Env, ticker: &str, key: &str, body: Value) -> Value {
    let (s, v) = e
        .post(&format!("/api/research/{ticker}/journal"), key, body)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}

#[tokio::test]
async fn an_entry_is_stamped_with_the_latest_close_and_never_restamped() {
    let e = env();
    let latest = e.store.last_closes("COAL", 1).unwrap()[0].1;
    let a = add(
        &e,
        "COAL",
        "j1",
        json!({"kind": "catatan", "text": "  Rapat investor menaikkan target produksi  "}),
    )
    .await;
    assert_eq!(a["kind"], "catatan");
    assert_eq!(a["text"], "Rapat investor menaikkan target produksi");
    assert_eq!(a["price_at"].as_f64().unwrap(), latest);
    assert_eq!(a["at"], "2026-10-24T09:00:00+00:00");
    assert_eq!(a["annotations"], json!([]));
    assert!(a["level"].is_null());

    // A later close does not change what the entry says its price was.
    e.store
        .upsert_prices(
            "COAL",
            &[PriceBar {
                date: "2026-10-26".into(),
                open: None,
                high: None,
                low: None,
                close: latest + 500.0,
                volume: None,
            }],
        )
        .unwrap();
    e.set_clock("2026-10-27T03:00:00+00:00");
    let b = add(
        &e,
        "COAL",
        "j2",
        json!({"kind": "keputusan", "text": "Menahan posisi"}),
    )
    .await;
    assert_eq!(b["price_at"].as_f64().unwrap(), latest + 500.0);
    let (_, list) = e.get("/api/research/COAL/journal").await;
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], b["id"], "newest first");
    assert_eq!(items[1]["price_at"].as_f64().unwrap(), latest);

    // A ticker with no price yet stamps nothing rather than a guess.
    e.store
        .with_conn(|c| Ok(c.execute("DELETE FROM prices_daily WHERE ticker = 'BANK'", [])?))
        .unwrap();
    let c = add(
        &e,
        "BANK",
        "j3",
        json!({"kind": "catatan", "text": "Belum ada harga"}),
    )
    .await;
    assert!(c["price_at"].is_null());
}

#[tokio::test]
async fn kinds_levels_and_text_are_validated() {
    let e = env();
    let cases: Vec<(&str, Value)> = vec![
        ("kind", json!({"kind": "sistem", "text": "x"})),
        ("kind", json!({"kind": "tesis", "text": "x"})),
        ("kind", json!({"text": "x"})),
        ("text", json!({"kind": "catatan", "text": "   "})),
        ("text", json!({"kind": "catatan", "text": "x".repeat(2001)})),
        ("level", json!({"kind": "level", "text": "incaran"})),
        (
            "level",
            json!({"kind": "level", "text": "incaran", "level": 0}),
        ),
        (
            "level",
            json!({"kind": "level", "text": "incaran", "level": -3}),
        ),
        (
            "level",
            json!({"kind": "level", "text": "incaran", "level": "tinggi"}),
        ),
        (
            "level",
            json!({"kind": "catatan", "text": "x", "level": 1200}),
        ),
    ];
    for (i, (field, body)) in cases.into_iter().enumerate() {
        let (s, v) = e
            .post("/api/research/COAL/journal", &format!("bad{i}"), body)
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {v}");
        assert_eq!(v["detail"][0]["loc"][1], field, "{v}");
    }
    let ok = add(
        &e,
        "COAL",
        "lvl",
        json!({"kind": "level", "text": "Harga incaran", "level": 1_200.0}),
    )
    .await;
    assert_eq!(ok["level"], 1_200.0);
    let (s, _) = e
        .post(
            "/api/research/ZZZZ/journal",
            "z",
            json!({"kind": "catatan", "text": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/journal",
        None,
        Some(json!({"kind": "catatan", "text": "x"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(e.store.journal(Some("COAL")).unwrap().len(), 1);
}

#[tokio::test]
async fn a_repeated_key_replays_without_a_second_entry() {
    let e = env();
    let body = json!({"kind": "catatan", "text": "Sekali saja"});
    let first = add(&e, "COAL", "same", body.clone()).await;
    let (s, again, headers) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/journal",
        Some("same"),
        Some(body),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(again["id"], first["id"]);
    assert_eq!(headers["idempotent-replay"], "true");
    assert_eq!(e.store.journal(Some("COAL")).unwrap().len(), 1);
}

#[tokio::test]
async fn annotations_are_added_below_an_entry_and_the_entry_is_immutable() {
    let e = env();
    let entry = add(
        &e,
        "COAL",
        "j1",
        json!({"kind": "keputusan", "text": "Menambah posisi"}),
    )
    .await;
    let id = entry["id"].as_str().unwrap();
    let before = e.store.journal_entry(id).unwrap().unwrap();

    e.set_clock("2026-10-25T03:00:00+00:00");
    let (s, a) = e
        .post(
            &format!("/api/research/journal/{id}/annotations"),
            "a1",
            json!({"text": "Ternyata laporan kuartal 3 lebih lemah"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(a["journal_id"], id);
    assert_eq!(a["at"], "2026-10-25T03:00:00+00:00");
    let (_, one) = e.get(&format!("/api/research/journal/{id}")).await;
    assert_eq!(one["text"], "Menambah posisi");
    assert_eq!(
        one["annotations"][0]["text"],
        "Ternyata laporan kuartal 3 lebih lemah"
    );
    let (_, list) = e.get("/api/research/COAL/journal").await;
    assert_eq!(list["items"][0]["annotations"].as_array().unwrap().len(), 1);

    // Validation and unknown entries.
    let (s, _) = e
        .post(
            &format!("/api/research/journal/{id}/annotations"),
            "a2",
            json!({"text": " "}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = e
        .post(
            "/api/research/journal/j-missing/annotations",
            "a3",
            json!({"text": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // No route edits or removes an entry: PUT, PATCH and DELETE are refused on the entry
    // and on the ticker-scoped path, and the stored row is exactly what was written.
    let ticker_path = format!("/api/research/COAL/journal/{id}");
    let entry_path = format!("/api/research/journal/{id}");
    for (method, path) in [
        (Method::PUT, &entry_path),
        (Method::PATCH, &entry_path),
        (Method::DELETE, &entry_path),
        (Method::PUT, &ticker_path),
        (Method::PATCH, &ticker_path),
        (Method::DELETE, &ticker_path),
        (Method::PUT, &"/api/research/COAL/journal".to_string()),
        (Method::DELETE, &"/api/research/COAL/journal".to_string()),
    ] {
        let (s, _, _) = call(
            &e.app,
            method.clone(),
            path,
            Some("edit"),
            Some(json!({"kind": "catatan", "text": "diubah"})),
        )
        .await;
        assert!(
            s == StatusCode::METHOD_NOT_ALLOWED || s == StatusCode::NOT_FOUND,
            "{method} {path} answered {s}"
        );
    }
    assert_eq!(e.store.journal_entry(id).unwrap().unwrap(), before);
    assert_eq!(e.store.journal(Some("COAL")).unwrap().len(), 1);
}

#[tokio::test]
async fn the_list_pages_newest_first_twenty_at_a_time() {
    let e = env();
    for i in 0..25 {
        e.store
            .add_journal(&JournalRow {
                id: format!("j-{i:02}"),
                ticker: "COAL".into(),
                at: format!("2026-09-{:02}T01:00:00+00:00", i + 1),
                kind: "catatan".into(),
                text: format!("catatan {i}"),
                price_at: None,
                level: None,
            })
            .unwrap();
    }
    let (_, p1) = e.get("/api/research/COAL/journal").await;
    assert_eq!(p1["total"], 25);
    assert_eq!(p1["limit"], 20);
    assert_eq!(p1["items"].as_array().unwrap().len(), 20);
    assert_eq!(p1["items"][0]["id"], "j-24");
    let (_, p2) = e.get("/api/research/COAL/journal?offset=20").await;
    let ids: Vec<&str> = p2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["j-04", "j-03", "j-02", "j-01", "j-00"]);
    let (_, small) = e.get("/api/research/COAL/journal?limit=3").await;
    assert_eq!(small["items"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn journal_levels_fill_the_levels_slot_of_the_prices_payload() {
    let e = env();
    let (_, before) = e.get("/api/research/COAL/prices?range=1y").await;
    assert_eq!(before["levels"], json!([]));
    let level = add(
        &e,
        "COAL",
        "lv",
        json!({"kind": "level", "text": "Harga incaran", "level": 1_200.0}),
    )
    .await;
    add(
        &e,
        "COAL",
        "n",
        json!({"kind": "catatan", "text": "bukan level"}),
    )
    .await;
    add(
        &e,
        "BANK",
        "o",
        json!({"kind": "level", "text": "milik lain", "level": 50.0}),
    )
    .await;
    let (s, v) = e.get("/api/research/COAL/prices?range=1y").await;
    assert_eq!(s, StatusCode::OK);
    let levels = v["levels"].as_array().unwrap();
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0]["id"], level["id"]);
    assert_eq!(levels[0]["level"], 1_200.0);
    assert_eq!(levels[0]["at"], "2026-10-24");
    assert_eq!(levels[0]["text"], "Harga incaran");
    let close = v["stats"]["close"]["value"].as_f64().unwrap();
    let want = 100.0 * (1_200.0 / close - 1.0);
    assert!((levels[0]["gap_pct"].as_f64().unwrap() - want).abs() < 1e-3);
    // Levels do not depend on the range.
    let (_, short) = e.get("/api/research/COAL/prices?range=3y").await;
    assert_eq!(short["levels"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn rebuild_restores_the_journal_and_keeps_migrated_v4_entries() {
    let e = env();
    let entry = add(
        &e,
        "COAL",
        "j1",
        json!({"kind": "level", "text": "Harga incaran", "level": 1_100.0}),
    )
    .await;
    let id = entry["id"].as_str().unwrap();
    e.post(
        &format!("/api/research/journal/{id}/annotations"),
        "a1",
        json!({"text": "Catatan tambahan"}),
    )
    .await;
    // A row the v4 migration wrote straight into the table (it is not in the log).
    e.store
        .with_conn(|c| {
            Ok(c.execute(
                "INSERT INTO journal (id, ticker, at, kind, text, price_at, level)
                 VALUES ('v4note:1', 'COAL', '2026-01-02T00:00:00+00:00', 'catatan', 'catatan lama', NULL, NULL)",
                [],
            )?)
        })
        .unwrap();
    let journal = e.store.journal(None).unwrap();
    let annotations = e.store.annotations(None).unwrap();
    assert_eq!(journal.len(), 2);
    e.store.rebuild().unwrap();
    assert_eq!(e.store.journal(None).unwrap(), journal);
    assert_eq!(e.store.annotations(None).unwrap(), annotations);
}
