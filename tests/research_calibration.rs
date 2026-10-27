//! Calibration at API level (research plan §6 R4.6; prd.md A4.4): every journal level and
//! decision and every claim with the move since it was written, shown only once 30 days
//! have passed, on the split-adjusted basis, with the claims' hit rate as text. The row
//! arithmetic is unit-tested in `calibration.rs`.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::http::StatusCode;
use finance_app::research::store::{CorpEventRow, EventKind, PriceBar};
use r4_env::{env, Env};
use serde_json::{json, Value};

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

/// On 2026-10-24: a level, a decision, a note (never calibrated) and a claim on COAL.
async fn write_entries(e: &Env) -> f64 {
    let p0 = e.store.last_closes("COAL", 1).unwrap()[0].1;
    for (key, body) in [
        (
            "lv",
            json!({"kind": "level", "text": "Harga incaran", "level": 1_500.0}),
        ),
        ("kp", json!({"kind": "keputusan", "text": "Menahan posisi"})),
        (
            "ct",
            json!({"kind": "catatan", "text": "Bukan bahan kalibrasi"}),
        ),
    ] {
        let (s, v) = e.post("/api/research/COAL/journal", key, body).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
    }
    let (s, v) = e
        .post(
            "/api/research/COAL/claims",
            "cl",
            json!({
                "statement": "Harga di atas seribu", "metric": "price.close",
                "comparator": ">=", "threshold": 1000, "window": "latest", "due": "2027-03-31",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    p0
}

fn by_kind<'a>(v: &'a Value, kind: &str) -> &'a Value {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} row in {v}"))
}

#[tokio::test]
async fn no_return_until_thirty_days_have_passed_then_the_move_since() {
    let e = env();
    let p0 = write_entries(&e).await;

    // 29 days later: everything is listed, nothing has a return.
    e.set_clock("2026-11-22T03:00:00+00:00");
    let (s, v) = e.get("/api/research/calibration?ticker=COAL").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["maturity_days"], 30);
    assert_eq!(
        (
            v["total"].as_i64(),
            v["matured"].as_i64(),
            v["pending"].as_i64()
        ),
        (Some(3), Some(0), Some(3))
    );
    for row in v["items"].as_array().unwrap() {
        assert_eq!(row["matured"], false);
        assert_eq!(row["days"], 29);
        assert!(row["return_pct"].is_null());
        assert_eq!(row["reason"], "belum 30 hari");
        assert_eq!(row["matures_on"], "2026-11-23");
    }

    // Day 30 with later closes: the level was reached on 11-02, the last close is 1400.
    e.set_clock("2026-11-23T03:00:00+00:00");
    e.store
        .upsert_prices(
            "COAL",
            &[bar("2026-11-02", 1_550.0), bar("2026-11-20", 1_400.0)],
        )
        .unwrap();
    let (_, v) = e.get("/api/research/calibration?ticker=COAL").await;
    assert_eq!(v["matured"], 3);
    let want = (1_400.0 / p0 - 1.0) * 100.0;
    for kind in ["klaim", "level", "keputusan"] {
        let row = by_kind(&v, kind);
        assert_eq!(row["matured"], true, "{kind}");
        assert_eq!(row["price_then"].as_f64().unwrap(), p0, "{kind}");
        assert_eq!(row["price_now"], 1_400.0);
        assert_eq!(row["price_now_date"], "2026-11-20");
        assert!(
            (row["return_pct"].as_f64().unwrap() - want).abs() < 1e-3,
            "{kind}"
        );
        assert_eq!(row["date"], "2026-10-24");
        assert!(row["reason"].is_null());
    }
    let level = by_kind(&v, "level");
    assert_eq!(level["level"], 1_500.0);
    assert_eq!(level["reached"], true);
    assert_eq!(level["status"], "Tercapai");
    assert_eq!(by_kind(&v, "klaim")["status"], "Berlaku");
    assert!(by_kind(&v, "keputusan")["status"].is_null());
    // The note never appears.
    assert!(v["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["text"] != "Bukan bahan kalibrasi"));
    // Footer: hit rate of the claims as text.
    assert_eq!(v["hit_rates"][0]["text"], "COAL: 1 Berlaku, 0 Patah (100%)");
}

#[tokio::test]
async fn a_level_not_reached_says_so_and_a_split_moves_the_stamp_to_the_adjusted_basis() {
    let e = env();
    let p0 = write_entries(&e).await;
    e.set_clock("2026-12-01T03:00:00+00:00");
    e.store
        .upsert_prices("COAL", &[bar("2026-11-30", 700.0)])
        .unwrap();
    // A 2-for-1 split between the entry and today: stored closes are already on the new
    // basis, stamped prices are not.
    e.store
        .upsert_events(&[CorpEventRow {
            ticker: "COAL".into(),
            date: "2026-11-15".into(),
            kind: EventKind::Split,
            amount: None,
            ratio_num: Some(2.0),
            ratio_den: Some(1.0),
        }])
        .unwrap();
    let (_, v) = e.get("/api/research/calibration?ticker=coal").await;
    assert_eq!(v["ticker"], "COAL", "the filter is normalised");
    let decision = by_kind(&v, "keputusan");
    assert_eq!(decision["price_then"].as_f64().unwrap(), p0 / 2.0);
    assert!(
        (decision["return_pct"].as_f64().unwrap() - (700.0 / (p0 / 2.0) - 1.0) * 100.0).abs()
            < 1e-3
    );
    let level = by_kind(&v, "level");
    assert_eq!(level["level"], 750.0, "the line moves with the split");
    assert_eq!(level["reached"], false);
    assert_eq!(level["status"], "Belum tercapai");
}

#[tokio::test]
async fn the_type_and_ticker_filters_and_their_errors() {
    let e = env();
    write_entries(&e).await;
    let (s, _) = e
        .post(
            "/api/research/BANK/journal",
            "bk",
            json!({"kind": "keputusan", "text": "Menambah posisi"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    e.set_clock("2026-12-01T03:00:00+00:00");

    let (_, all) = e.get("/api/research/calibration").await;
    assert_eq!(all["total"], 4);
    assert!(all["ticker"].is_null());
    let (_, only) = e.get("/api/research/calibration?type=keputusan").await;
    assert_eq!(only["total"], 2);
    assert_eq!(only["type"], "keputusan");
    let (_, coal) = e
        .get("/api/research/calibration?ticker=COAL&type=klaim")
        .await;
    assert_eq!(coal["total"], 1);
    assert_eq!(coal["items"][0]["kind_label"], "Klaim");
    let (_, bank) = e.get("/api/research/calibration?ticker=BANK").await;
    assert_eq!(bank["total"], 1);
    assert!(bank["hit_rates"].as_array().unwrap().is_empty());

    let (s, v) = e.get("/api/research/calibration?type=catatan").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"][0]["loc"], json!(["query", "type"]));
    let (s, _) = e.get("/api/research/calibration?ticker=ZZZZ").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_broken_claim_counts_against_the_hit_rate_and_nothing_is_called_good_or_bad() {
    let e = env();
    write_entries(&e).await;
    let (s, _) = e
        .post(
            "/api/research/COAL/claims",
            "cl2",
            json!({
                "statement": "Harga di bawah seribu", "metric": "price.close",
                "comparator": "<", "threshold": 1000, "window": "latest", "due": "2027-03-31",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    e.set_clock("2026-12-01T03:00:00+00:00");
    let (_, v) = e.get("/api/research/calibration?ticker=COAL").await;
    assert_eq!(v["hit_rates"][0]["text"], "COAL: 1 Berlaku, 1 Patah (50%)");
    // No verdict vocabulary, and no field that scores or ranks a result.
    let text = v.to_string().to_lowercase();
    let verdicts =
        regex::Regex::new(r"\b(beli|jual|murah|mahal|layak|wajar|coret|fair)\b").unwrap();
    assert!(!verdicts.is_match(&text));
    for key in ["score", "rank", "grade", "color", "colour"] {
        assert!(!text.contains(&format!("\"{key}\"")), "{key}");
    }
}
