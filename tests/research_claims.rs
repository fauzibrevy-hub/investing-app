//! Claims at API level (research plan §6 R4.1; prd.md A4.1, §6.2): a claim bound to a pack
//! metric, a KPI or the price evaluates to Berlaku, Patah, Belum teruji or Kedaluwarsa, the
//! evaluation records its value and inputs, and the re-evaluation hook moves the status when
//! data arrives. The pure comparator and window cases are unit tests in `claims/mod.rs`.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::{Method, StatusCode};
use investing_app::research::claims::source::reevaluate;
use investing_app::research::store::PriceBar;
use r4_env::{call, closes, env, Env};
use serde_json::{json, Value};

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

async fn create(e: &Env, ticker: &str, key: &str, body: Value) -> Value {
    let (s, v) = e
        .post(&format!("/api/research/{ticker}/claims"), key, body)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}

fn rerun(e: &Env) -> Vec<investing_app::research::claims::source::StatusChange> {
    reevaluate(&e.store, &e.pack_set(), e.now(), e.today(), None).unwrap()
}

async fn evals(e: &Env, id: &str) -> Vec<Value> {
    let (s, v) = e.get(&format!("/api/research/claims/{id}/evals")).await;
    assert_eq!(s, StatusCode::OK);
    v["items"].as_array().unwrap().clone()
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

#[tokio::test]
async fn a_pack_metric_claim_evaluates_on_save_and_records_value_and_inputs() {
    let e = env();
    let holding = create(
        &e,
        "COAL",
        "k1",
        claim(
            "Laba bersih kuartal 2 positif",
            "core.net_income",
            ">",
            0.0,
            "quarter:2026Q2",
            "2026-12-31",
        ),
    )
    .await;
    assert_eq!(holding["status"], "holding");
    assert_eq!(holding["status_label"], "Berlaku");
    assert!(holding["preview"]
        .as_str()
        .unwrap()
        .contains("saat laporan 2026Q2 tersedia"));
    let value = holding["last_eval"]["value"].as_f64().unwrap();
    assert!(value > 0.0);

    let broken = create(
        &e,
        "COAL",
        "k2",
        claim(
            "Laba bersih kuartal 2 di bawah nol",
            "core.net_income",
            "<",
            0.0,
            "quarter:2026Q2",
            "2026-12-31",
        ),
    )
    .await;
    assert_eq!(broken["status"], "broken");
    assert_eq!(broken["status_label"], "Patah");

    // A quarter that has no filing yet is not testable, with the reason on the evaluation.
    let waiting = create(
        &e,
        "COAL",
        "k3",
        claim(
            "Laba kuartal 3 lebih dari nol",
            "core.net_income",
            ">",
            0.0,
            "quarter:2026Q3",
            "2026-12-31",
        ),
    )
    .await;
    assert_eq!(waiting["status_label"], "Belum teruji");

    // The evaluation stores the value and the operands of the formula it used.
    let rows = evals(&e, holding["id"].as_str().unwrap()).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["value"].as_f64().unwrap(), value);
    let obs = &rows[0]["inputs"]["observations"][0];
    assert_eq!(obs["quarter"], json!({"fy": 2026, "q": 2}));
    assert_eq!(obs["inputs"]["kind"], "xbrl");
    assert!(!obs["inputs"]["operands"].as_array().unwrap().is_empty());
    assert_eq!(rows[0]["inputs"]["window"], "quarter:2026Q2");
    assert_eq!(rows[0]["inputs"]["as_of"], "2026-10-24");

    // The list puts Patah first, then Belum teruji, then Berlaku, and counts them.
    let (_, list) = e.get("/api/research/COAL/claims").await;
    let order: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["status"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["broken", "not_testable", "holding"]);
    assert_eq!(
        list["counts"],
        json!({"holding": 1, "broken": 1, "not_testable": 1, "expired": 0})
    );
    let metrics = list["metrics"].as_array().unwrap();
    assert!(metrics
        .iter()
        .any(|m| m["id"] == "coal_mining.production_mt" && m["layer"] == "kpi"));
    assert!(metrics
        .iter()
        .any(|m| m["id"] == "price.close" && m["quarterly"] == false));
}

#[tokio::test]
async fn claim_input_is_validated_field_by_field() {
    let e = env();
    let ok = claim(
        "Harga di atas seribu",
        "price.close",
        ">=",
        1_000.0,
        "latest",
        "2026-12-31",
    );
    let cases: Vec<(&str, Value)> = vec![
        ("statement", json!({"statement": ""})),
        ("statement", json!({"statement": "x".repeat(201)})),
        ("comparator", json!({"comparator": "=="})),
        ("threshold", json!({"threshold": "tinggi"})),
        ("threshold_hi", json!({"threshold_hi": 5.0})),
        ("threshold_hi", json!({"comparator": "between"})),
        (
            "threshold_hi",
            json!({"comparator": "between", "threshold_hi": 999.0}),
        ),
        ("window", json!({"window": "daily"})),
        ("window", json!({"window": "every_quarter"})),
        ("window", json!({"window": "quarter:2026Q3"})),
        ("due", json!({"due": "2026-10-23"})),
        ("due", json!({"due": "besok"})),
        ("metric", json!({"metric": "price.volume"})),
        ("metric", json!({"metric": "bank_sharia.car"})),
        ("metric", json!({"metric": "whatever"})),
    ];
    for (i, (field, patch)) in cases.into_iter().enumerate() {
        let mut body = ok.clone();
        for (k, v) in patch.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (s, v) = e
            .post("/api/research/COAL/claims", &format!("bad{i}"), body)
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {v}");
        assert_eq!(v["detail"][0]["loc"][1], field, "{v}");
    }
    // A driver series has no quarterly value; a ticker without a pack still has core.
    let (s, _) = e
        .post(
            "/api/research/COAL/claims",
            "driver",
            claim(
                "Harga batubara",
                "coal_mining.newcastle",
                ">",
                1.0,
                "latest",
                "2026-12-31",
            ),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let none = create(
        &e,
        "NONE",
        "nopack",
        claim(
            "Ekuitas positif",
            "core.total_equity",
            ">",
            0.0,
            "latest",
            "2026-12-31",
        ),
    )
    .await;
    assert_eq!(none["status"], "holding");
    // Unknown ticker, missing key.
    let (s, _) = e.post("/api/research/ZZZZ/claims", "k", ok.clone()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/claims",
        None,
        Some(ok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Nothing was stored by the refused requests.
    assert!(e.store.claims(Some("COAL")).unwrap().is_empty());
}

#[tokio::test]
async fn a_repeated_key_replays_and_a_changed_body_conflicts() {
    let e = env();
    let body = claim(
        "Harga di atas seribu",
        "price.close",
        ">=",
        1_000.0,
        "latest",
        "2026-12-31",
    );
    let first = create(&e, "COAL", "same", body.clone()).await;
    let (s, again, headers) = call(
        &e.app,
        Method::POST,
        "/api/research/COAL/claims",
        Some("same"),
        Some(body.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(again["id"], first["id"]);
    assert_eq!(headers["idempotent-replay"], "true");
    let mut other = body;
    other["threshold"] = json!(1_001.0);
    let (s, _) = e.post("/api/research/COAL/claims", "same", other).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(e.store.claims(Some("COAL")).unwrap().len(), 1);
}

#[tokio::test]
async fn kpi_entry_re_evaluates_the_claim_and_a_future_document_waits_for_its_date() {
    let e = env();
    let c = create(
        &e,
        "COAL",
        "kpi",
        claim(
            "Produksi minimal lima",
            "coal_mining.production_mt",
            ">=",
            5.0,
            "latest",
            "2026-12-31",
        ),
    )
    .await;
    let id = c["id"].as_str().unwrap();
    assert_eq!(c["status_label"], "Belum teruji");
    let kpi = |value: f64, doc_date: &str| json!({"value": value, "document": "laporan operasional sintetis", "document_date": doc_date});
    // A document dated after today is not known yet: nothing changes (point in time).
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "p1",
            kpi(6.0, "2026-11-30"),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(evals(&e, id).await.len(), 1);
    let (_, list) = e.get("/api/research/COAL/claims").await;
    assert_eq!(list["items"][0]["status"], "not_testable");
    // The date comes: the pass picks the value up.
    e.set_clock("2026-12-01T03:00:00+00:00");
    let changes = rerun(&e);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].from.as_str(), changes[0].to.as_str()),
        ("not_testable", "holding")
    );
    assert_eq!(evals(&e, id).await.len(), 2);
    // An unchanged pass stores nothing.
    assert!(rerun(&e).is_empty());
    assert_eq!(evals(&e, id).await.len(), 2);
    // A new value for the quarter, entered through the API, re-evaluates in the same request.
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "p2",
            kpi(4.0, "2026-11-30"),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let rows = evals(&e, id).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["status"], "broken");
    assert_eq!(rows[2]["value"], 4.0);
    assert_eq!(
        rows[2]["inputs"]["observations"][0]["inputs"]["document"],
        "laporan operasional sintetis"
    );
    // Removing the entry makes the claim untestable again.
    let (s, _, _) = call(
        &e.app,
        Method::DELETE,
        "/api/research/COAL/kpi/production_mt/2026/2",
        Some("p3"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let rows = evals(&e, id).await;
    assert_eq!(rows.last().unwrap()["status"], "not_testable");
}

#[tokio::test]
async fn price_claims_follow_new_closes_and_due_date_claims_wait() {
    let e = env();
    let above = create(
        &e,
        "BANK",
        "px",
        claim(
            "Harga di atas 1100",
            "price.close",
            ">=",
            1_100.0,
            "latest",
            "2027-03-31",
        ),
    )
    .await;
    assert_eq!(above["status"], "holding");
    let id = above["id"].as_str().unwrap();
    // The MA200 metrics exist once 200 closes are stored.
    let ma = create(
        &e,
        "BANK",
        "ma",
        claim(
            "Harga di atas MA200",
            "price.vs_ma200_pct",
            ">",
            0.0,
            "latest",
            "2027-03-31",
        ),
    )
    .await;
    assert_eq!(ma["status"], "holding", "{ma}");
    let at_due = create(
        &e,
        "BANK",
        "due",
        claim(
            "Harga akhir November",
            "price.close",
            ">=",
            1_400.0,
            "at_due",
            "2026-11-30",
        ),
    )
    .await;
    assert_eq!(at_due["status_label"], "Belum teruji");
    let at_due_id = at_due["id"].as_str().unwrap();

    // A close below the threshold arrives.
    e.store
        .upsert_prices("BANK", &[bar("2026-10-23", 900.0)])
        .unwrap();
    let changes = rerun(&e);
    let moved: Vec<(&str, &str, &str)> = changes
        .iter()
        .map(|c| (c.claim_id.as_str(), c.from.as_str(), c.to.as_str()))
        .collect();
    assert!(moved.contains(&(id, "holding", "broken")), "{moved:?}");
    assert_eq!(evals(&e, id).await.len(), 2);
    assert_eq!(evals(&e, at_due_id).await.len(), 1, "due date not reached");

    // The due date passes with closes on both sides: the one on or before it decides.
    e.set_clock("2026-12-01T03:00:00+00:00");
    let mut late = closes("2026-12-01", 1_000.0);
    late.retain(|b| b.date.as_str() >= "2026-11-27");
    for b in &mut late {
        b.close = if b.date == "2026-11-30" {
            1_450.0
        } else {
            100.0
        };
    }
    e.store.upsert_prices("BANK", &late).unwrap();
    rerun(&e);
    let rows = evals(&e, at_due_id).await;
    assert_eq!(rows.last().unwrap()["status"], "holding");
    assert_eq!(rows.last().unwrap()["value"], 1_450.0);
}

#[tokio::test]
async fn a_claim_without_data_expires_and_a_closed_claim_is_left_alone() {
    let e = env();
    let waiting = create(
        &e,
        "COAL",
        "w",
        claim(
            "Penjualan minimal lima",
            "coal_mining.sales_mt",
            ">=",
            5.0,
            "latest",
            "2026-10-25",
        ),
    )
    .await;
    let id = waiting["id"].as_str().unwrap();
    e.set_clock("2026-10-27T03:00:00+00:00");
    let changes = rerun(&e);
    assert_eq!(
        (changes[0].from.as_str(), changes[0].to.as_str()),
        ("not_testable", "expired")
    );
    let rows = evals(&e, id).await;
    assert_eq!(rows.last().unwrap()["label"], "Kedaluwarsa");
    assert_eq!(
        rows.last().unwrap()["inputs"]["reason"],
        "lewat jatuh tempo tanpa data"
    );

    // Closing keeps the status and the history and stops re-evaluation.
    let px = create(
        &e,
        "COAL",
        "px",
        claim(
            "Harga di atas 1100",
            "price.close",
            ">=",
            1_100.0,
            "latest",
            "2027-03-31",
        ),
    )
    .await;
    let pid = px["id"].as_str().unwrap();
    let uri = format!("/api/research/claims/{pid}");
    let (s, v) = e
        .send(Method::PATCH, &uri, "c1", json!({"closed": true}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["closed_at"].is_string());
    assert_eq!(v["status"], "holding");
    let (s, _) = e
        .send(Method::PATCH, &uri, "c2", json!({"closed": true}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = e
        .send(Method::PATCH, &uri, "c3", json!({"closed": false}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = e
        .send(
            Method::PATCH,
            "/api/research/claims/c-missing",
            "c4",
            json!({"closed": true}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    e.store
        .upsert_prices("COAL", &[bar("2026-10-23", 10.0)])
        .unwrap();
    assert!(rerun(&e).is_empty());
    assert_eq!(evals(&e, pid).await.len(), 1);
    // No PUT or DELETE on a claim: the record is edited by closing it, not rewritten.
    let (s, _) = e.send(Method::PUT, &uri, "c5", json!({})).await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
    let (s, _, _) = call(&e.app, Method::DELETE, &uri, Some("c6"), None).await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
    // Counts leave closed claims out.
    let (_, list) = e.get("/api/research/COAL/claims").await;
    assert_eq!(list["counts"]["expired"], 1);
    assert_eq!(list["counts"]["holding"], 0);
}

#[tokio::test]
async fn rebuild_restores_claims_and_their_evaluations_from_the_log() {
    let e = env();
    let c = create(
        &e,
        "COAL",
        "r",
        claim(
            "Laba bersih positif",
            "core.net_income",
            ">",
            0.0,
            "latest",
            "2026-12-31",
        ),
    )
    .await;
    let id = c["id"].as_str().unwrap();
    let before_claims = e.store.claims(None).unwrap();
    let before_evals = e.store.claim_evals(id).unwrap();
    assert_eq!(before_evals.len(), 1);
    e.store.rebuild().unwrap();
    assert_eq!(e.store.claims(None).unwrap(), before_claims);
    assert_eq!(e.store.claim_evals(id).unwrap(), before_evals);
}

#[tokio::test]
async fn price_thresholds_follow_a_split_that_came_after_the_claim() {
    use investing_app::research::store::{CorpEventRow, EventKind};
    let e = env();
    let ok = create(
        &e,
        "COAL",
        "ok",
        claim(
            "Harga di atas 1100",
            "price.close",
            ">=",
            1_100.0,
            "latest",
            "2027-03-31",
        ),
    )
    .await;
    let high = create(
        &e,
        "COAL",
        "high",
        claim(
            "Harga di atas 1400",
            "price.close",
            ">=",
            1_400.0,
            "latest",
            "2027-03-31",
        ),
    )
    .await;
    assert_eq!(ok["status"], "holding");
    assert_eq!(high["status"], "broken");
    // A 2-for-1 split: the price source now serves closes on the new basis (about half).
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
    e.set_clock("2026-11-02T03:00:00+00:00");
    e.store
        .upsert_prices("COAL", &[bar("2026-11-02", 640.0)])
        .unwrap();
    rerun(&e);
    // 1100 and 1400 written before the split are 550 and 700 on the new basis: 640 clears
    // the first and not the second, as 1280 would have before the split.
    let (_, list) = e.get("/api/research/COAL/claims").await;
    let status = |id: &str| {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status(ok["id"].as_str().unwrap()), "holding");
    assert_eq!(status(high["id"].as_str().unwrap()), "broken");
    assert!(
        evals(&e, ok["id"].as_str().unwrap()).await.len() <= 2,
        "no spurious flip to broken and back"
    );
}

#[tokio::test]
async fn every_quarter_does_not_judge_a_claim_by_history_that_existed_before_it() {
    let e = env();
    let c = create(
        &e,
        "COAL",
        "eq",
        claim(
            "Laba bersih tiap kuartal positif",
            "core.net_income",
            ">",
            0.0,
            "every_quarter",
            "2027-06-30",
        ),
    )
    .await;
    // Eighteen quarters are already on file; none of them was reported since the claim was
    // written, so the claim waits for the next report.
    assert_eq!(c["status"], "not_testable", "{c}");
    assert!(c["preview"]
        .as_str()
        .unwrap()
        .contains("yang terbit sejak 2026-10-24"));
}

#[tokio::test]
async fn a_claim_whose_metric_the_packs_no_longer_offer_becomes_not_testable() {
    let e = env();
    let c = create(
        &e,
        "COAL",
        "kpi",
        claim(
            "Produksi minimal lima",
            "coal_mining.production_mt",
            ">=",
            5.0,
            "latest",
            "2026-12-31",
        ),
    )
    .await;
    let id = c["id"].as_str().unwrap();
    e.send(
        Method::PUT,
        "/api/research/COAL/kpi/production_mt/2026/2",
        "k1",
        json!({"value": 6, "document": "laporan operasional sintetis", "document_date": "2026-09-10"}),
    )
    .await;
    let (_, list) = e.get("/api/research/COAL/claims").await;
    assert_eq!(list["items"][0]["status"], "holding");
    // The ticker is moved to another pack: the coal metric is no longer in its packs.
    let (s, v) = e
        .send(
            Method::PUT,
            "/api/research/universe/COAL",
            "ov",
            json!({"pack_override": "bank_sharia"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let changes = rerun(&e);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].to, "not_testable");
    let rows = evals(&e, id).await;
    assert_eq!(
        rows.last().unwrap()["inputs"]["reason"],
        "metrik tidak tersedia di paket saat ini"
    );
}
