//! Documents and evidence at API level (research plan §6 R4.5; design.md §4.2 Sumber): the
//! owner files a document by URL or pasted text, may attach it to a claim or a KPI cell in
//! the same save or later, links are unique, and everything is logged and rebuilt.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::{Method, StatusCode};
use r4_env::{call, env, Env};
use serde_json::{json, Value};

async fn claim(e: &Env, ticker: &str, key: &str) -> String {
    let (s, v) = e
        .post(
            &format!("/api/research/{ticker}/claims"),
            key,
            json!({
                "statement": "Harga di atas seribu", "metric": "price.close",
                "comparator": ">=", "threshold": 1000, "window": "latest", "due": "2026-12-31",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v["id"].as_str().unwrap().to_string()
}

async fn doc(e: &Env, ticker: &str, key: &str, body: Value) -> Value {
    let (s, v) = e
        .post(&format!("/api/research/{ticker}/documents"), key, body)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}

#[tokio::test]
async fn a_document_is_filed_by_url_or_text_and_listed_newest_first() {
    let e = env();
    let a = doc(
        &e,
        "COAL",
        "d1",
        json!({
            "kind": "presentasi", "url": " https://example.test/paparan-q2 ",
            "doc_date": "2026-08-20",
        }),
    )
    .await;
    assert_eq!(a["kind_label"], "Presentasi");
    assert_eq!(a["url"], "https://example.test/paparan-q2");
    assert!(a["text"].is_null());
    assert_eq!(a["added_at"], "2026-10-24T09:00:00+00:00");
    assert_eq!(a["evidence"], json!([]));
    let b = doc(
        &e,
        "COAL",
        "d2",
        json!({"kind": "catatan", "text": "Manajemen menyebut stripping ratio turun", "doc_date": "2026-09-30"}),
    )
    .await;
    doc(
        &e,
        "COAL",
        "d3",
        json!({"kind": "artikel", "url": "https://example.test/berita"}),
    )
    .await;
    doc(
        &e,
        "BANK",
        "d4",
        json!({"kind": "laporan", "text": "milik BANK"}),
    )
    .await;
    let (s, list) = e.get("/api/research/COAL/documents").await;
    assert_eq!(s, StatusCode::OK);
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "only this ticker's documents");
    // Newest by document date; one without a date sorts by the day it was filed.
    let ids: Vec<&str> = items.iter().map(|d| d["id"].as_str().unwrap()).collect();
    assert_eq!(ids[0], items[0]["id"]);
    assert_eq!(items[0]["kind"], "artikel");
    assert_eq!(items[1]["id"], b["id"]);
    assert_eq!(items[2]["id"], a["id"]);
    assert_eq!(list["kinds"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn a_document_can_be_attached_to_a_claim_in_the_same_save_or_later() {
    let e = env();
    let c = claim(&e, "COAL", "c1").await;
    let d = doc(
        &e,
        "COAL",
        "d1",
        json!({"kind": "update_operasional", "url": "https://example.test/ops", "claim_id": c}),
    )
    .await;
    assert_eq!(d["evidence"], json!([{"claim_id": c, "kpi_key": null}]));
    // The claim card counts it and lists it under Bukti.
    let (_, claims) = e.get("/api/research/COAL/claims").await;
    assert_eq!(claims["items"][0]["evidence_count"], 1);
    let (_, ev) = e.get(&format!("/api/research/claims/{c}/evidence")).await;
    assert_eq!(ev["items"].as_array().unwrap().len(), 1);
    assert_eq!(ev["items"][0]["id"], d["id"]);

    // A second document, linked afterwards; linking it again is a no-op.
    let d2 = doc(
        &e,
        "COAL",
        "d2",
        json!({"kind": "artikel", "text": "analisis pihak ketiga"}),
    )
    .await;
    let link = json!({"document_id": d2["id"], "claim_id": c});
    let (s, first) = e.post("/api/research/evidence", "e1", link.clone()).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(first["created"], true);
    let (s, again) = e.post("/api/research/evidence", "e2", link).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(again["created"], false);
    let (_, claims) = e.get("/api/research/COAL/claims").await;
    assert_eq!(claims["items"][0]["evidence_count"], 2);
    let (_, list) = e.get("/api/research/COAL/documents").await;
    let linked = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["evidence"].as_array().is_some_and(|x| !x.is_empty()))
        .count();
    assert_eq!(linked, 2);
    let (s, _) = e.get("/api/research/claims/c-missing/evidence").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_document_can_be_attached_to_a_kpi_cell_of_the_tickers_pack() {
    let e = env();
    let d = doc(
        &e,
        "COAL",
        "d1",
        json!({"kind": "update_operasional", "url": "https://example.test/ops", "kpi_key": "production_mt:2026Q2"}),
    )
    .await;
    assert_eq!(
        d["evidence"],
        json!([{"claim_id": null, "kpi_key": "production_mt:2026Q2"}])
    );
    // The KPI need not be entered yet, but it must be a KPI of the pack in effect.
    let other = doc(&e, "COAL", "d2", json!({"kind": "artikel", "text": "x"})).await;
    for (i, key) in [
        "net_income:2026Q2",
        "unknown_kpi:2026Q2",
        "production_mt:2026Q9",
        "production_mt",
    ]
    .into_iter()
    .enumerate()
    {
        let (s, v) = e
            .post(
                "/api/research/evidence",
                &format!("k{i}"),
                json!({"document_id": other["id"], "kpi_key": key}),
            )
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {v}");
        assert_eq!(v["detail"][0]["loc"][1], "kpi_key");
    }
    // A ticker with no pack has no KPI to attach to.
    let n = doc(&e, "NONE", "d3", json!({"kind": "artikel", "text": "x"})).await;
    let (s, _) = e
        .post(
            "/api/research/evidence",
            "none",
            json!({"document_id": n["id"], "kpi_key": "production_mt:2026Q2"}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn bad_documents_and_targets_are_refused_and_leave_nothing_behind() {
    let e = env();
    let bank_claim = claim(&e, "BANK", "c1").await;
    let cases: Vec<(&str, Value)> = vec![
        ("kind", json!({"kind": "berita", "text": "x"})),
        ("kind", json!({"text": "x"})),
        ("url", json!({"kind": "artikel"})),
        (
            "url",
            json!({"kind": "artikel", "url": "ftp://example.test/a"}),
        ),
        ("url", json!({"kind": "artikel", "url": "example.test/a"})),
        (
            "text",
            json!({"kind": "catatan", "text": "x".repeat(20_001)}),
        ),
        (
            "doc_date",
            json!({"kind": "catatan", "text": "x", "doc_date": "2026-10-25"}),
        ),
        (
            "doc_date",
            json!({"kind": "catatan", "text": "x", "doc_date": "kemarin"}),
        ),
        // Targets: a claim of another ticker, an unknown claim, both targets at once.
        (
            "claim_id",
            json!({"kind": "catatan", "text": "x", "claim_id": bank_claim}),
        ),
        (
            "claim_id",
            json!({"kind": "catatan", "text": "x", "claim_id": "c-missing"}),
        ),
        (
            "claim_id",
            json!({"kind": "catatan", "text": "x", "claim_id": bank_claim, "kpi_key": "production_mt:2026Q2"}),
        ),
    ];
    for (i, (field, body)) in cases.into_iter().enumerate() {
        let (s, v) = e
            .post("/api/research/COAL/documents", &format!("bad{i}"), body)
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {v}");
        assert_eq!(v["detail"][0]["loc"][1], field, "{v}");
    }
    // A refused target does not leave the document behind.
    assert!(e.store.documents("COAL").unwrap().is_empty());
    // /evidence: unknown document, and no target at all.
    let (s, v) = e
        .post(
            "/api/research/evidence",
            "x1",
            json!({"document_id": "d-missing", "claim_id": bank_claim}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"][0]["loc"][1], "document_id");
    let d = doc(&e, "COAL", "ok", json!({"kind": "catatan", "text": "x"})).await;
    let (s, _) = e
        .post(
            "/api/research/evidence",
            "x2",
            json!({"document_id": d["id"]}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = e
        .post(
            "/api/research/ZZZZ/documents",
            "z",
            json!({"kind": "catatan", "text": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/documents",
        None,
        Some(json!({"kind": "catatan", "text": "x"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_repeated_key_replays_and_rebuild_restores_documents_and_links() {
    let e = env();
    let c = claim(&e, "COAL", "c1").await;
    let body = json!({"kind": "artikel", "url": "https://example.test/a", "claim_id": c});
    let first = doc(&e, "COAL", "same", body.clone()).await;
    let (s, again, headers) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/documents",
        Some("same"),
        Some(body),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(again["id"], first["id"]);
    assert_eq!(headers["idempotent-replay"], "true");
    assert_eq!(e.store.documents("COAL").unwrap().len(), 1);

    let docs = e.store.documents("COAL").unwrap();
    let links = e.store.evidence("COAL").unwrap();
    assert_eq!(links.len(), 1);
    e.store.rebuild().unwrap();
    assert_eq!(e.store.documents("COAL").unwrap(), docs);
    assert_eq!(e.store.evidence("COAL").unwrap(), links);
}

#[tokio::test]
async fn migration_008_adds_the_evidence_uniqueness_and_reopening_changes_nothing() {
    let e = env();
    let version = e.store.user_version().unwrap();
    assert_eq!(version, finance_app::research::store::latest_version());
    assert!(version >= 7, "008 is user_version 7");
    let d = doc(&e, "COAL", "d1", json!({"kind": "catatan", "text": "x"})).await;
    let id = d["id"].as_str().unwrap().to_string();
    // The NULLable key columns would let SQLite store a link twice; the index refuses it.
    let twice = e
        .store
        .with_conn(|c| {
            let sql = "INSERT INTO evidence (claim_id, kpi_key, document_id) VALUES (NULL, 'production_mt:2026Q2', ?1)";
            c.execute(sql, [&id])?;
            Ok(c.execute(sql, [&id]).is_err())
        })
        .unwrap();
    assert!(twice, "a repeated link must violate evidence_unique");
    // A second open of the same file runs no migration and keeps the data.
    let again =
        finance_app::research::store::Store::open(&e.dir.path().join("research.sqlite")).unwrap();
    assert_eq!(again.user_version().unwrap(), version);
    assert_eq!(again.documents("COAL").unwrap().len(), 1);
}
