//! Canon v1 fact routes end to end (research plan §5, §7.2; PRD A1.1, A1.3 to A1.7):
//! `GET /api/research/filings`, `GET /api/research/{ticker}/financials`,
//! `GET /api/research/facts/{filing_id}/{element}` and the health additions, over a
//! synthetic filing directory ingested into a temp store. Also 401, 404 and 422.

#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use common::{json_of, TestApp};
use finance_app::research::store::Store;
use finance_app::research::xbrl;
use serde_json::Value;
use xbrl_fixture::{general_year, Year};

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

/// Two years of AAAA filings: 2024 complete, 2025 missing Q2 (filled from the Q3
/// comparative? no: Q2 2025 has no later comparative in-year, so it is a gap) and the Q2
/// 2025 filing restates the Q2 2024 comparative revenue by 7 million.
fn seed(dir: &std::path::Path) -> TestApp {
    let xbrl_dir = dir.join("XBRL");
    general_year(&xbrl_dir, "AAAA", &Y2024, None, &[], None);
    general_year(
        &xbrl_dir,
        "AAAA",
        &Y2025,
        Some(&Y2024),
        &["Q3", "FY"],
        Some((1, 237.0)),
    );
    // An unreadable file sits in the directory too.
    std::fs::write(
        xbrl_dir.join("2025/Q2/BBBB_2025_Q2.xbrl"),
        "<html>IDX download page</html>",
    )
    .unwrap();
    let db = dir.join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap();
        let report = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
        assert_eq!(
            report.ingested.len(),
            6,
            "{:?} {:?}",
            report.ingested,
            report.unparsed
        );
        assert_eq!(report.unparsed.len(), 1);
        // Nothing changed → nothing re-ingested.
        let again = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
        assert!(again.ingested.is_empty());
        assert_eq!(
            again.unchanged, 7,
            "the unreadable file is retried only when its hash changes"
        );
        assert!(again.unparsed.is_empty());
        assert_eq!(
            store.ingest_errors().unwrap().len(),
            1,
            "but it stays on record for health"
        );
    }
    // The app opens the seeded store (the scheduler is disabled in test configs).
    common::test_app_with(|cfg| {
        cfg.idx_xbrl_dir = xbrl_dir.clone();
        cfg.research_db_path = db.clone();
    })
}

fn cells<'a>(fin: &'a Value, row_id: &str) -> &'a Vec<Value> {
    fin["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["rows"].as_array().unwrap())
        .find(|r| r["id"] == row_id)
        .unwrap_or_else(|| panic!("no row {row_id}"))["cells"]
        .as_array()
        .unwrap()
}

#[tokio::test]
async fn filings_are_listed_with_header_fields_and_unreadable_files() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, v) = t.get("/api/research/filings?ticker=AAAA").await;
    assert_eq!(s, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    let newest = &items[0];
    assert_eq!(newest["id"], "AAAA_2025_Q2");
    assert_eq!(newest["period"], "Q2");
    assert_eq!(newest["taxonomy"], "general");
    assert_eq!(newest["industry_code"], "C141");
    assert_eq!(newest["audited"], false);
    assert_eq!(newest["rounding"], "Jutaan");
    assert_eq!(newest["currency"], "IDR");
    assert_eq!(newest["available_from"], "2025-08-29");
    assert_eq!(newest["available_source"], "estimated");
    assert!(newest["fact_count"].as_i64().unwrap() > 6);
    let fy = items.iter().find(|i| i["id"] == "AAAA_2024_FY").unwrap();
    assert_eq!(fy["audited"], true);
    assert_eq!(fy["available_from"], "2025-03-31");
    let unparsed = v["unparsed"].as_array().unwrap();
    assert_eq!(unparsed.len(), 1);
    assert!(unparsed[0]["source_path"]
        .as_str()
        .unwrap()
        .ends_with("BBBB_2025_Q2.xbrl"));
    assert!(unparsed[0]["error"]
        .as_str()
        .unwrap()
        .contains("not an XBRL"));
    // No ticker → every filing; a malformed ticker → 404.
    let (s, all) = t.get("/api/research/filings").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(all["items"].as_array().unwrap().len(), 6);
    assert_eq!(
        t.get("/api/research/filings?ticker=abc1").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn standalone_ttm_and_annual_views_with_flags_gaps_and_shares() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let m = 1e6;

    let (s, fin) = t
        .get("/api/research/AAAA/financials?view=standalone&periods=6")
        .await;
    assert_eq!(s, StatusCode::OK, "{fin}");
    assert_eq!(fin["view"], "standalone");
    assert_eq!(fin["rules_version"], "2026.10-r4");
    assert_eq!(fin["currency"], "IDR");
    let labels: Vec<&str> = fin["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["2024Q1", "2024Q2", "2024Q3", "2024Q4", "2025Q1", "2025Q2"]
    );
    let q4 = &fin["periods"][3];
    assert_eq!(q4["filing_id"], "AAAA_2024_FY");
    assert_eq!(q4["audited"], true);
    assert_eq!(q4["derived_fy"], true);
    let groups: Vec<&str> = fin["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].as_str().unwrap())
        .collect();
    assert_eq!(groups, ["laba_rugi", "arus_kas", "neraca"]);

    let rev = cells(&fin, "revenue");
    assert_eq!(rev[0]["value"], 100.0 * m);
    assert_eq!(rev[1]["value"], 130.0 * m);
    assert_eq!(rev[3]["value"], 150.0 * m, "Q4 = FY − YTD Q3");
    assert!(rev[3]["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "derived_fy"));
    // The Q2 2025 filing restated the Q2 2024 comparative: the original stays, flagged.
    assert!(
        rev[1]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "restated"),
        "{}",
        rev[1]
    );
    assert_eq!(rev[4]["value"], 120.0 * m);
    assert_eq!(rev[5]["value"], 140.0 * m);
    // Every cell has a FactChip target that names the filing and context it came from.
    assert_eq!(rev[3]["fact"]["filing_id"], "AAAA_2024_FY");
    assert_eq!(rev[3]["fact"]["ctx"], "2024-01-01_2024-12-31");
    assert_eq!(rev[3]["inputs"], 2);
    // Instants carry the value with no derivation.
    let eq = cells(&fin, "equity");
    assert_eq!(eq[5]["value"], 460.0 * m);
    assert!(eq[5]["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "instant"));
    // No capex element in these filings → the row is absent, not zero.
    assert!(fin["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["rows"].as_array().unwrap())
        .all(|r| r["id"] != "capex"));
    // Shares: no tag in the filings and no manual entry → the EPS-implied derivation of
    // each period, labelled as such (D13 option c, R3.2); never an unlabelled guess.
    assert_eq!(fin["shares"].as_array().unwrap().len(), 6);
    assert!(fin["shares"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["source"]["kind"] == "derived" && s["shares"] == 10_000_000.0));
    assert_eq!(fin["filings"].as_array().unwrap().len(), 6);

    let (_, ttm) = t
        .get("/api/research/AAAA/financials?view=ttm&periods=3")
        .await;
    let rev = cells(&ttm, "revenue");
    // TTM(2025Q2) = YTD Q2 2025 + FY 2024 − YTD Q2 2024 (original 230).
    assert_eq!(rev[2]["value"], (260.0 + 500.0 - 230.0) * m);
    assert_eq!(rev[1]["value"], (120.0 + 500.0 - 100.0) * m);
    assert_eq!(rev[0]["value"], 500.0 * m, "TTM at Q4 is the FY");
    // Standalone equity index 5 is 2025Q2 (periods asserted above); TTM index 2 is the same quarter.
    assert_eq!(
        cells(&ttm, "equity")[2]["value"],
        cells(&fin, "equity")[5]["value"],
        "instants show the quarter-end balance in TTM"
    );

    let (_, annual) = t
        .get("/api/research/AAAA/financials?view=annual&periods=4")
        .await;
    let labels: Vec<&str> = annual["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["2024"],
        "no FY 2023 comparative in these filings and FY 2025 not filed yet: no gap column is invented"
    );
    let rev = cells(&annual, "revenue");
    assert_eq!(rev[0]["value"], 500.0 * m);

    // A manual share count is used with its source, and `elements=all` widens the table.
    {
        let store = Store::open(&t.cfg.research_db_path).unwrap();
        store
            .set_shares_manual(&finance_app::research::store::ManualShares {
                ticker: "AAAA".into(),
                shares: 5_000_000.0,
                as_of: "2025-01-15".into(),
                source_note: "prospektus sintetis".into(),
                entered_at: "2026-10-24T00:00:00+00:00".into(),
            })
            .unwrap();
    }
    let (_, fin) = t
        .get("/api/research/AAAA/financials?view=standalone&periods=6&elements=all")
        .await;
    let shares = fin["shares"].as_array().unwrap();
    assert_eq!(shares[5]["shares"], 5_000_000.0);
    assert_eq!(shares[5]["source"]["kind"], "manual");
    assert_eq!(shares[5]["source"]["after_date"], false);
    assert_eq!(shares[5]["source_label"], "manual");
    // 2024 periods predate the entry: the EPS-implied derivation of the period itself
    // is used and labelled (D13 option c, R3.2); the fixture's EPS is profit ÷ 10.
    assert_eq!(shares[0]["source"]["kind"], "derived");
    assert_eq!(shares[0]["shares"], 10_000_000.0);
    assert_eq!(
        shares[0]["source"]["eps_element"],
        "idx-cor:BasicEarningsLossPerShareFromContinuingOperations"
    );
    assert_eq!(shares[0]["source_label"], "turunan (laba ÷ LPS)");
    let rows: Vec<&str> = fin["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["rows"].as_array().unwrap())
        .map(|r| r["element"].as_str().unwrap())
        .collect();
    assert!(rows.contains(&"idx-cor:Assets") && rows.contains(&"idx-cor:SalesAndRevenue"));
}

#[tokio::test]
async fn fact_drawer_shows_filing_history_restatement_and_derivations() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let m = 1e6;
    let (s, v) = t
        .get("/api/research/facts/AAAA_2024_Q2/idx-cor:SalesAndRevenue?ctx=2024-01-01_2024-06-30")
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["fact"]["value"], 230.0 * m);
    assert_eq!(v["fact"]["unit"], "IDR");
    assert_eq!(v["fact"]["decimals"], -6);
    assert_eq!(v["fact"]["label"]["en"], "Sales And Revenue");
    assert_eq!(v["filing"]["id"], "AAAA_2024_Q2");
    assert_eq!(v["filing"]["rounding"], "Jutaan");
    // The same period as reported by the original filing and by the 2025 Q2 comparative.
    let hist = v["history"].as_array().unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0]["filing_id"], "AAAA_2024_Q2");
    assert_eq!(hist[0]["original"], true);
    assert_eq!(hist[1]["filing_id"], "AAAA_2025_Q2");
    assert_eq!(hist[1]["value"], 237.0 * m);
    let rs = v["restatements"].as_array().unwrap();
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0]["original"], 230.0 * m);
    assert_eq!(rs[0]["restated"], 237.0 * m);
    assert_eq!(rs[0]["restated_filing"], "AAAA_2025_Q2");
    // Derived cells that used this fact: Q2 and Q3 2024 standalone, plus the TTM cells.
    let derived = v["derived"].as_array().unwrap();
    let q2 = derived
        .iter()
        .find(|d| d["fy"] == 2024 && d["q"] == 2 && d["view"] == "standalone")
        .unwrap();
    assert_eq!(q2["value"], 130.0 * m);
    assert_eq!(q2["formula"], "ytd_q2 − ytd_q1");
    assert_eq!(q2["inputs"].as_array().unwrap().len(), 2);
    assert_eq!(q2["inputs"][1]["fact"]["filing_id"], "AAAA_2024_Q1");
    let q3 = derived
        .iter()
        .find(|d| d["fy"] == 2024 && d["q"] == 3 && d["view"] == "standalone")
        .unwrap();
    assert_eq!(q3["formula"], "ytd_q3 − ytd_q2");
    let ttm = derived
        .iter()
        .find(|d| d["fy"] == 2025 && d["q"] == 2 && d["view"] == "ttm")
        .unwrap();
    assert_eq!(ttm["formula"], "ytd_q2 + prev_fy − prev_ytd_q2");

    // An instant opens too, with its own formula text.
    let (s, v) = t
        .get("/api/research/facts/AAAA_2024_FY/idx-cor:Equity?ctx=2024-12-31")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["derived"][0]["formula"], "nilai pada tanggal neraca");
}

#[tokio::test]
async fn health_reports_filings_facts_unreadable_files_and_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, h) = t.get("/api/health?deep=1").await;
    assert_eq!(s, StatusCode::OK);
    let src = &h["sources"];
    assert_eq!(src["research_filings"]["filings"], 6);
    assert_eq!(src["research_filings"]["unparsed"], 1);
    assert_eq!(
        src["research_filings"]["ok"], false,
        "an unreadable file is never hidden"
    );
    assert!(src["research_filings"]["detail"]
        .as_str()
        .unwrap()
        .contains("BBBB_2025_Q2.xbrl"));
    assert_eq!(src["research_facts"]["ok"], true);
    assert!(src["research_facts"]["facts"].as_i64().unwrap() > 30);
    assert_eq!(src["research_reconciliation"]["failures"], 0);
    assert_eq!(src["research_reconciliation"]["ok"], true);
    assert_eq!(src["research_reconciliation"]["restated_years"], 0);
    assert_eq!(
        src["research_reconciliation"]["restated"],
        serde_json::json!([])
    );
    assert_eq!(src["research_store"]["ok"], true);
}

/// AAAA's FY 2024 revenue is restated by the FY 2025 filing (500 → 520): the year is explained
/// by the restatement, so ingest reports no failure and health counts it as a restated year.
#[tokio::test]
async fn restated_fy_is_counted_as_restated_year_not_as_reconciliation_failure() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    general_year(&xbrl_dir, "AAAA", &Y2024, None, &[], None);
    general_year(
        &xbrl_dir,
        "AAAA",
        &Y2025,
        Some(&Y2024),
        &["Q3"],
        Some((3, 520.0)),
    );
    let db = dir.path().join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap();
        let report = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
        assert!(
            report.reconciliation_failures.is_empty(),
            "{:?}",
            report.reconciliation_failures
        );
        assert!(store.reconciliation_failures().unwrap().is_empty());
        assert_eq!(
            store.restated_years().unwrap(),
            [("AAAA".to_string(), 2024)]
        );
    }
    let t = common::test_app_with(|cfg| {
        cfg.idx_xbrl_dir = xbrl_dir.clone();
        cfg.research_db_path = db.clone();
    });
    let (s, h) = t.get("/api/health?deep=1").await;
    assert_eq!(s, StatusCode::OK);
    let rec = &h["sources"]["research_reconciliation"];
    assert_eq!(rec["failures"], 0);
    assert_eq!(
        rec["ok"], true,
        "restated years are explained, not failures"
    );
    assert_eq!(rec["restated_years"], 1);
    assert_eq!(
        rec["restated"],
        serde_json::json!([{"ticker": "AAAA", "fy": 2024}])
    );
    assert!(
        rec["detail"].as_str().unwrap().contains("AAAA FY2024"),
        "{rec}"
    );
}

/// The restated-year set and the failure set partition the Q4 cells whatever flags the YTD Q3
/// input adds: a FY restated while Q3 is a sign conflict is a restated year; a FY that is
/// itself a sign conflict stays a failure; per-share elements are in neither.
#[tokio::test]
async fn restated_years_and_failures_partition_the_q4_cells() {
    use finance_app::research::derive::{Cell, CellInputs, Flag, Restatement};
    use finance_app::research::xbrl::CtxKind;
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    let revenue = "idx-cor:Revenue";
    let eps = "idx-cor:BasicEarningsLossPerShareFromContinuingOperations";
    let q4 = |element: &str, fy: i32, flags: &[Flag]| Cell {
        element: element.into(),
        fy,
        q: 4,
        standalone: Some(1.0),
        ttm: None,
        annual: Some(1.0),
        flags: flags.iter().copied().collect::<BTreeSet<_>>(),
        inputs: CellInputs::default(),
    };
    let row = |element: &str, fy: i32| Restatement {
        element: element.into(),
        ctx_kind: CtxKind::Duration,
        start: chrono::NaiveDate::from_ymd_opt(fy, 1, 1),
        end: chrono::NaiveDate::from_ymd_opt(fy, 12, 31).unwrap(),
        dims: String::new(),
        original: 10.0,
        original_filing: format!("AAAA_{fy}_FY"),
        restated: 20.0,
        restated_filing: format!("AAAA_{}_FY", fy + 1),
    };
    let flags = |names: &[Flag]| names.to_vec();
    {
        let store = Store::open(&db).unwrap();
        let now = "2026-10-25T09:00:00+00:00";
        let cells = [
            // FY restated; Q3 is a sign conflict: explained, a restated year.
            q4(revenue, 2022, &flags(&[Flag::Restated, Flag::SignConflict])),
            // FY itself a sign conflict (Q3 restated): unexplained, a failure.
            q4(
                revenue,
                2023,
                &flags(&[Flag::Restated, Flag::SignConflict, Flag::ReconcileFailed]),
            ),
            // Restated per-share FY: neither.
            q4(eps, 2021, &flags(&[Flag::Restated])),
            // Q3 restated, FY not: no FY restatement row, neither.
            q4(revenue, 2020, &flags(&[Flag::Restated])),
        ];
        let rows = [row(revenue, 2022), row(revenue, 2023), row(eps, 2021)];
        store.replace_derived("AAAA", &cells, &rows, now).unwrap();
        assert_eq!(
            store.restated_years().unwrap(),
            [("AAAA".to_string(), 2022)]
        );
        assert_eq!(
            store.reconciliation_failures().unwrap(),
            [("AAAA".to_string(), revenue.to_string(), 2023)]
        );
    }
    let t = common::test_app_with(|cfg| {
        cfg.research_db_path = db.clone();
    });
    let (s, h) = t.get("/api/health?deep=1").await;
    assert_eq!(s, StatusCode::OK);
    let rec = &h["sources"]["research_reconciliation"];
    assert_eq!(rec["ok"], false, "an unexplained failure is red");
    assert_eq!(rec["failures"], 1);
    assert_eq!(rec["restated_years"], 1);
    assert_eq!(
        rec["restated"],
        serde_json::json!([{"ticker": "AAAA", "fy": 2022}])
    );
}

#[tokio::test]
async fn auth_not_found_and_validation_errors() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    for uri in [
        "/api/research/filings?ticker=AAAA",
        "/api/research/AAAA/financials",
        "/api/research/facts/AAAA_2024_Q2/idx-cor:SalesAndRevenue?ctx=2024-01-01_2024-06-30",
    ] {
        let res = t.send(Request::get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    for uri in [
        "/api/research/ZZZZ/financials",
        "/api/research/abc/financials",
        "/api/research/facts/ZZZZ_2024_Q2/idx-cor:SalesAndRevenue?ctx=2024-01-01_2024-06-30",
        "/api/research/facts/AAAA_2024_Q2/idx-cor:Nothing?ctx=2024-01-01_2024-06-30",
        "/api/research/facts/AAAA_2024_Q2/idx-cor:SalesAndRevenue?ctx=2024-06-30",
    ] {
        assert_eq!(t.get(uri).await.0, StatusCode::NOT_FOUND, "{uri}");
    }
    for uri in [
        "/api/research/AAAA/financials?view=quarter",
        "/api/research/AAAA/financials?periods=0",
        "/api/research/AAAA/financials?periods=abc",
        "/api/research/facts/AAAA_2024_Q2/idx-cor:SalesAndRevenue",
        "/api/research/facts/AAAA_2024_Q2/idx-cor:SalesAndRevenue?ctx=June",
    ] {
        let (s, body) = t.get(uri).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {body}");
        assert!(body["detail"].is_array(), "{uri}: {body}");
    }
    // The v4 screen keeps its route and payload shape under /v4/ until R5.
    let (s, _) = t.get("/api/research/v4/AAAA/financials?view=ttm").await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "AAAA is not in the v4 universe here"
    );
    let (s, v) = json_of(
        t.send(
            Request::get("/api/research/filings")
                .header("authorization", t.bearer())
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["items"].is_array());
}
