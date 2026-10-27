//! Original filings read from a PDF (`xbrl/pdf.rs`, migration 012): the BUMI 2024 Q1 filing
//! that IDX never published as XBRL. The fixture `tests/fixtures/pdf-filings/BUMI_2024_Q1.json`
//! was read from the owner's PDF; every expectation here is computed from that file or is a
//! figure of the published BUMI H1 2024 XBRL (`BUMI_2024_Q2.xbrl`), never from the engine.

#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

mod common;

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use chrono::{TimeZone, Utc};
use investing_app::research::derive::Flag;
use investing_app::research::prices::yahoo::YahooSource;
use investing_app::research::scheduler::{Clock, Scheduler};
use investing_app::research::sources::portfolio::HeldProvider;
use investing_app::research::sources::SourceError;
use investing_app::research::store::{latest_version, Store};
use investing_app::research::universe::{sync, HeldPosition};
use investing_app::research::xbrl::{self, pdf, CtxKind, Period, SourceKind};
use serde_json::{json, Value};
use std::sync::Arc;
use xbrl_fixture::{Spec, F};

struct NoHeld;

impl HeldProvider for NoHeld {
    async fn held(&self) -> Result<Vec<HeldPosition>, SourceError> {
        Ok(Vec::new())
    }
}

const NOW: &str = "2026-10-26T09:00:00+00:00";

/// Published in `BUMI_2024_Q2.xbrl` (H1 2024, full USD).
const H1_REVENUE: f64 = 595_841_018.0;
const H1_PROFIT: f64 = 92_957_578.0;
const H1_CASH: f64 = 55_771_197.0;
const H1_EQUITY: f64 = 2_865_459_057.0;
const H1_SHORT_BANK: f64 = 88_548_832.0;
const H1_LONG_BANK: f64 = 19_433_850.0;
const H1_CURRENT_BANK: f64 = 26_271_810.0;
/// Nine months, `BUMI_2024_Q3.xbrl`.
const M9_REVENUE: f64 = 926_881_924.0;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 9, 0, 0).unwrap()
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-filings/BUMI_2024_Q1.json")
}

fn fixture() -> Value {
    serde_json::from_slice(&std::fs::read(fixture_path()).unwrap()).unwrap()
}

/// The `value` the fixture file carries for one element.
fn fixture_value(element: &str) -> f64 {
    fixture()["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["element"] == element)
        .unwrap_or_else(|| panic!("fixture has no {element}"))["value"]
        .as_f64()
        .unwrap()
}

fn write_pdf_dir(dir: &Path, doc: &Value) -> PathBuf {
    let pdf_dir = dir.join("pdf-filings");
    std::fs::create_dir_all(&pdf_dir).unwrap();
    std::fs::write(
        pdf_dir.join("BUMI_2024_Q1.json"),
        serde_json::to_vec_pretty(doc).unwrap(),
    )
    .unwrap();
    pdf_dir
}

fn bumi_spec(period: &'static str, facts: Vec<F>) -> Spec {
    Spec {
        ticker: "BUMI",
        fy: 2024,
        period,
        industry: "Umum / General",
        subindustry: "A121. Coal Production",
        currency: "USD",
        rounding: "Satuan Penuh / Full Amount",
        audited: false,
        facts,
        extra_contexts: String::new(),
    }
}

/// BUMI's published H1 and nine-month filings; Q1 is the one IDX has no XBRL for.
fn write_bumi_xbrl(dir: &Path) {
    let h1: Vec<F> = vec![
        (
            "SalesAndRevenue",
            "CurrentYearDuration",
            H1_REVENUE,
            0,
            "USD",
        ),
        ("ProfitLoss", "CurrentYearDuration", H1_PROFIT, 0, "USD"),
        (
            "CashAndCashEquivalents",
            "CurrentYearInstant",
            H1_CASH,
            0,
            "USD",
        ),
        ("Equity", "CurrentYearInstant", H1_EQUITY, 0, "USD"),
        (
            "ShortTermBankLoans",
            "CurrentYearInstant",
            H1_SHORT_BANK,
            0,
            "USD",
        ),
        (
            "LongTermBankLoans",
            "CurrentYearInstant",
            H1_LONG_BANK,
            0,
            "USD",
        ),
        (
            "CurrentMaturitiesOfBankLoans",
            "CurrentYearInstant",
            H1_CURRENT_BANK,
            0,
            "USD",
        ),
    ];
    xbrl_fixture::write(dir, &bumi_spec("Q2", h1));
    let m9: Vec<F> = vec![(
        "SalesAndRevenue",
        "CurrentYearDuration",
        M9_REVENUE,
        0,
        "USD",
    )];
    xbrl_fixture::write(dir, &bumi_spec("Q3", m9));
}

fn derived_cell(
    store: &Store,
    element: &str,
    q: u32,
) -> investing_app::research::store::DerivedRow {
    store
        .derived("BUMI", Some(element))
        .unwrap()
        .into_iter()
        .find(|r| r.fy == 2024 && r.q == q)
        .unwrap_or_else(|| panic!("no derived {element} 2024Q{q}"))
}

// ---- the file ----

#[test]
fn the_fixture_is_a_valid_filing_and_every_fact_cites_its_pdf_page() {
    let doc = fixture();
    let p = pdf::parse_pdf_filing(&fixture_path()).unwrap();
    let f = &p.filing;
    assert_eq!(f.id, "BUMI_2024_Q1");
    assert_eq!(f.source_kind, SourceKind::Pdf);
    assert_eq!((f.fy, f.period), (2024, Period::Q1));
    assert_eq!(f.currency, "USD");
    assert!(!f.audited);
    assert_eq!(f.industry_code.as_deref(), Some("A121"));
    assert_eq!(f.available_from.to_string(), doc["document_date"]);

    let listed = doc["facts"].as_array().unwrap();
    assert_eq!(p.facts.len(), listed.len());
    for (fact, raw) in p.facts.iter().zip(listed) {
        assert_eq!(fact.element, raw["element"]);
        assert_eq!(fact.value, raw["value"].as_f64().unwrap());
        assert_eq!(fact.unit, "USD");
        assert_eq!(fact.decimals, Some(0));
        assert!(fact.dims.is_empty());
        let cite = fact
            .cite
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no cite", fact.element));
        assert_eq!(i64::from(cite.page), raw["page"].as_i64().unwrap());
        assert!(!cite.label.is_empty());
    }
}

#[test]
fn the_fixture_has_current_period_facts_only() {
    let p = pdf::parse_pdf_filing(&fixture_path()).unwrap();
    for f in &p.facts {
        match f.ctx_kind {
            CtxKind::Duration => {
                assert_eq!(f.start.as_deref(), Some("2024-01-01"), "{}", f.element);
                assert_eq!(f.end, "2024-03-31", "{}", f.element);
            }
            CtxKind::Instant => {
                assert_eq!(f.start, None, "{}", f.element);
                assert_eq!(f.end, "2024-03-31", "{}", f.element);
            }
        }
    }
}

/// Parses the fixture after `mutate`, returning the error text.
fn reject(mutate: impl FnOnce(&mut Value)) -> String {
    let mut doc = fixture();
    mutate(&mut doc);
    let bytes = serde_json::to_vec(&doc).unwrap();
    match pdf::parse_pdf_filing_bytes(&bytes, Path::new("BUMI_2024_Q1.json"), "h".into()) {
        Ok(_) => panic!("the file was accepted"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_unknown_element_rejects_the_whole_file() {
    let e = reject(|d| d["facts"][3]["element"] = json!("idx-cor:NotAnElement"));
    assert!(
        e.contains("facts[3] idx-cor:NotAnElement: unknown element"),
        "{e}"
    );
}

#[test]
fn a_fact_without_a_page_rejects_the_file() {
    let e = reject(|d| {
        d["facts"][0].as_object_mut().unwrap().remove("page");
    });
    assert!(
        e.contains("facts[0]") && e.contains("page is missing"),
        "{e}"
    );
    let e = reject(|d| d["facts"][0]["page"] = json!(0));
    assert!(e.contains("page 0 is not a PDF page"), "{e}");
}

#[test]
fn a_value_that_is_not_a_finite_number_rejects_the_file() {
    // 1e999 overflows an f64; null and a missing key are not zero either.
    let text = std::fs::read_to_string(fixture_path()).unwrap();
    let sales = fixture_value("idx-cor:SalesAndRevenue");
    let overflow = text.replacen(&format!("{}", sales as i64), "1e999", 1);
    assert_ne!(overflow, text);
    let e = pdf::parse_pdf_filing_bytes(overflow.as_bytes(), Path::new("x.json"), "h".into())
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("out of range") || e.contains("not a finite"),
        "{e}"
    );
    let e = reject(|d| d["facts"][1]["value"] = Value::Null);
    assert!(
        e.contains("facts[1]") && e.contains("value is missing"),
        "{e}"
    );
    let e = reject(|d| d["facts"][1]["value"] = json!("NaN"));
    assert!(e.contains("not a pdf-filing JSON file"), "{e}");
}

#[test]
fn dates_that_do_not_match_the_period_reject_the_file() {
    // A comparative column: the same line one year earlier.
    let e = reject(|d| {
        d["facts"][0]["start"] = json!("2023-01-01");
        d["facts"][0]["end"] = json!("2023-03-31");
    });
    assert!(
        e.contains("are not the filing period 2024-01-01..2024-03-31"),
        "{e}"
    );
    // The period the header claims is not a calendar quarter.
    let e = reject(|d| d["period_end"] = json!("2024-03-30"));
    assert!(e.contains("is not the end of 2024 Q1"), "{e}");
    let e = reject(|d| d["period"] = json!("Q2"));
    assert!(e.contains("is not the end of 2024 Q2"), "{e}");
    // An instant on another date, a duration without a start.
    let e = reject(|d| {
        let i = d["facts"]
            .as_array()
            .unwrap()
            .iter()
            .position(|f| f["ctx_kind"] == "instant")
            .unwrap();
        d["facts"][i]["end"] = json!("2023-12-31");
    });
    assert!(e.contains("is not the period end 2024-03-31"), "{e}");
    let e = reject(|d| {
        d["facts"][0].as_object_mut().unwrap().remove("start");
    });
    assert!(e.contains("a duration fact needs start"), "{e}");
    let e = reject(|d| d["document_date"] = json!("2024-03-01"));
    assert!(e.contains("before the period end"), "{e}");
}

#[test]
fn a_repeated_fact_or_a_wrong_sum_rejects_the_file() {
    let e = reject(|d| {
        let first = d["facts"][0].clone();
        d["facts"].as_array_mut().unwrap().push(first);
    });
    assert!(e.contains("listed twice for the same period"), "{e}");
    // The composite facts must add up to their printed lines.
    let e = reject(|d| {
        let i = d["facts"]
            .as_array()
            .unwrap()
            .iter()
            .position(|f| f["components"].is_array())
            .unwrap();
        d["facts"][i]["value"] = json!(1);
    });
    assert!(e.contains("components add up to"), "{e}");
}

#[test]
fn a_page_that_does_not_fit_rejects_the_file_instead_of_dropping_the_fact() {
    let e = reject(|d| d["facts"][0]["page"] = json!(5_000_000_000i64));
    assert!(e.contains("facts[0]") && e.contains("too large"), "{e}");
    let e = reject(|d| d["facts"][0]["page"] = json!(-3));
    assert!(e.contains("page -3 is not a PDF page"), "{e}");
    let e = reject(|d| d["facts"][0]["page"] = json!(1.5));
    assert!(e.contains("not a pdf-filing JSON file"), "{e}");
}

#[test]
fn each_header_and_fact_rule_names_what_it_refused() {
    type Case = (&'static str, Box<dyn Fn(&mut Value)>, &'static str);
    let cases: Vec<Case> = vec![
        (
            "format",
            Box::new(|d| d["format"] = json!("pdf-filing/2")),
            "format:",
        ),
        (
            "ticker",
            Box::new(|d| d["ticker"] = json!("bumi")),
            "ticker:",
        ),
        ("period", Box::new(|d| d["period"] = json!("H1")), "period:"),
        (
            "period_start",
            Box::new(|d| d["period_start"] = json!("2024-02-01")),
            "is not 1 January of fy 2024",
        ),
        (
            "currency",
            Box::new(|d| d["currency"] = json!("usd")),
            "currency:",
        ),
        (
            "rounding",
            Box::new(|d| d["rounding"] = json!("Miliaran")),
            "rounding:",
        ),
        (
            "taxonomy",
            Box::new(|d| d["taxonomy"] = json!("  ")),
            "taxonomy is empty",
        ),
        (
            "source.file",
            Box::new(|d| d["source"]["file"] = json!("")),
            "source.file is empty",
        ),
        (
            "sha short",
            Box::new(|d| d["source"]["sha256"] = json!("ab")),
            "source.sha256",
        ),
        (
            "sha upper",
            Box::new(|d| d["source"]["sha256"] = json!("D".repeat(64))),
            "source.sha256",
        ),
        (
            "no facts",
            Box::new(|d| d["facts"] = json!([])),
            "facts is empty",
        ),
        (
            "ctx_kind",
            Box::new(|d| d["facts"][0]["ctx_kind"] = json!("point")),
            "ctx_kind `point`",
        ),
        (
            "instant start",
            Box::new(|d| {
                let i = d["facts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|f| f["ctx_kind"] == "instant")
                    .unwrap();
                d["facts"][i]["start"] = json!("2024-01-01");
            }),
            "an instant fact has no start",
        ),
        (
            "bad date",
            Box::new(|d| d["facts"][0]["end"] = json!("31/03/2024")),
            "is not a YYYY-MM-DD date",
        ),
        (
            "label blank",
            Box::new(|d| d["facts"][0]["label"] = json!("  ")),
            "label as printed is missing",
        ),
        (
            "label missing",
            Box::new(|d| {
                d["facts"][0].as_object_mut().unwrap().remove("label");
            }),
            "label as printed is missing",
        ),
        (
            "percent ratio",
            Box::new(|d| d["facts"][0]["element"] = json!("idx-cor:NonPerformingFinancingGross")),
            "not a monetary element",
        ),
        (
            "unknown key",
            Box::new(|d| d["facts"][0]["pages"] = json!(7)),
            "not a pdf-filing JSON file",
        ),
        (
            "unknown header key",
            Box::new(|d| d["comparatives"] = json!([])),
            "not a pdf-filing JSON file",
        ),
        (
            "component page",
            Box::new(|d| {
                let i = d["facts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|f| f["components"].is_array())
                    .unwrap();
                d["facts"][i]["components"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("page");
            }),
            "components[0]: page is missing",
        ),
        (
            "component value",
            Box::new(|d| {
                let i = d["facts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|f| f["components"].is_array())
                    .unwrap();
                d["facts"][i]["components"][1]["value"] = Value::Null;
            }),
            "components[1]: value is missing",
        ),
    ];
    for (name, mutate, expect) in cases {
        let e = reject(mutate);
        assert!(e.contains(expect), "{name}: {e}");
    }
}

#[test]
fn a_thousands_rounding_sets_decimals_and_refuses_values_typed_as_printed() {
    // The fixture's full-USD values are not multiples of a thousand: refused under Ribuan.
    let e = reject(|d| d["rounding"] = json!("Ribuan"));
    assert!(e.contains("is not a multiple of 1000"), "{e}");
    // Rounded to thousands (components too, value = their sum) the file is accepted at decimals -3.
    let mut doc = fixture();
    doc["rounding"] = json!("Ribuan");
    for f in doc["facts"].as_array_mut().unwrap() {
        let round = |v: f64| (v / 1000.0).round() * 1000.0;
        if let Some(cs) = f.get_mut("components").and_then(Value::as_array_mut) {
            let mut sum = 0.0;
            for c in cs {
                let v = round(c["value"].as_f64().unwrap());
                c["value"] = json!(v);
                sum += v;
            }
            f["value"] = json!(sum);
        } else {
            f["value"] = json!(round(f["value"].as_f64().unwrap()));
        }
    }
    let p = pdf::parse_pdf_filing_bytes(
        &serde_json::to_vec(&doc).unwrap(),
        Path::new("x.json"),
        "h".into(),
    )
    .unwrap();
    assert!(p.facts.iter().all(|f| f.decimals == Some(-3)));
}

#[test]
fn fact_dates_are_stored_canonical_so_the_derivation_and_the_cite_lookup_find_them() {
    let parse = |mutate: &dyn Fn(&mut Value)| {
        let mut d = fixture();
        mutate(&mut d);
        pdf::parse_pdf_filing_bytes(
            &serde_json::to_vec(&d).unwrap(),
            Path::new("x.json"),
            "h".into(),
        )
    };
    // Padding and a short month parse, and are stored as YYYY-MM-DD.
    let p = parse(&|d| {
        d["facts"][0]["start"] = json!(" 2024-1-1");
        d["facts"][0]["end"] = json!("2024-03-31 ");
    })
    .unwrap();
    assert_eq!(p.facts[0].start.as_deref(), Some("2024-01-01"));
    assert_eq!(p.facts[0].end, "2024-03-31");
    // The same period written two ways is one fact listed twice.
    let e = parse(&|d| {
        let mut again = d["facts"][0].clone();
        again["start"] = json!("2024-1-1");
        d["facts"].as_array_mut().unwrap().push(again);
    })
    .unwrap_err()
    .to_string();
    assert!(e.contains("listed twice for the same period"), "{e}");
}

#[test]
fn every_error_is_listed_up_to_five() {
    let e = reject(|d| {
        for i in 0..8 {
            d["facts"][i]["element"] = json!("idx-cor:Bogus");
        }
    });
    assert_eq!(e.matches("unknown element").count(), 5, "{e}");
    assert!(e.ends_with("and 3 more"), "{e}");
}

// ---- the ingest pass ----

#[test]
fn ingest_stores_the_filing_as_pdf_with_the_page_of_every_fact() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_dir = write_pdf_dir(dir.path(), &fixture());
    let xbrl_dir = dir.path().join("XBRL");
    std::fs::create_dir_all(&xbrl_dir).unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());

    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(report.ingested, ["BUMI_2024_Q1"], "{:?}", report.unparsed);
    assert!(report.unparsed.is_empty());
    assert_eq!(report.derived_tickers, ["BUMI"]);

    let row = store.filing("BUMI_2024_Q1").unwrap().unwrap();
    assert_eq!(row.filing.source_kind, SourceKind::Pdf);
    assert_eq!(
        row.fact_count,
        fixture()["facts"].as_array().unwrap().len() as i64
    );
    assert_eq!(store.pdf_filing_count().unwrap(), 1);
    // Equity (page 6 of the PDF) with the label as printed.
    let equity = store
        .fact(
            "BUMI_2024_Q1",
            "idx-cor:Equity",
            CtxKind::Instant,
            None,
            "2024-03-31",
            "",
        )
        .unwrap()
        .unwrap();
    assert_eq!(equity.value, fixture_value("idx-cor:Equity"));
    let cite = equity.cite.unwrap();
    assert_eq!(cite.page, 6);
    assert!(cite.label.contains("Ekuitas - Neto"));

    // Unchanged file: nothing is read again. A reingest rewrites the same facts.
    let again = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert!(again.ingested.is_empty());
    assert_eq!(again.unchanged, 1);
    let re = xbrl::reingest_all_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now()).unwrap();
    assert_eq!(re.ingested, ["BUMI_2024_Q1"]);
    assert_eq!(store.fact_count().unwrap(), row.fact_count);
}

#[test]
fn a_missing_pdf_dir_is_an_empty_source() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let nowhere = dir.path().join("no-such-dir");
    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&nowhere), now(), None).unwrap();
    assert_eq!(report.ingested.len(), 2);
    assert!(report.unparsed.is_empty());
    assert_eq!(store.pdf_filing_count().unwrap(), 0);
}

#[test]
fn a_rejected_file_is_an_ingest_error_and_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = fixture();
    doc["facts"][2]["element"] = json!("idx-cor:NotAnElement");
    let pdf_dir = write_pdf_dir(dir.path(), &doc);
    let xbrl_dir = dir.path().join("XBRL");
    std::fs::create_dir_all(&xbrl_dir).unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert!(report.ingested.is_empty());
    assert_eq!(report.unparsed.len(), 1);
    assert!(
        report.unparsed[0].1.contains("unknown element"),
        "{:?}",
        report.unparsed
    );
    assert_eq!(store.filing_count().unwrap(), 0);
    assert_eq!(store.fact_count().unwrap(), 0);
    let errors = store.ingest_errors().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].error.contains("facts[2]"));

    // Fixing the file (new hash) clears the error and ingests it.
    write_pdf_dir(dir.path(), &fixture());
    let fixed = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(fixed.ingested, ["BUMI_2024_Q1"]);
    assert!(store.ingest_errors().unwrap().is_empty());
}

#[test]
fn the_scheduler_pass_reads_the_pdf_dir_beside_the_xbrl_dir() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    let s = Scheduler::new(
        Arc::new(Store::open(&dir.path().join("research.sqlite")).unwrap()),
        YahooSource::new(Some("http://127.0.0.1:9/never-called")),
        NoHeld,
        xbrl_dir,
        dir.path().join("research/watchlist.csv"),
        Arc::new(Utc::now) as Clock,
    )
    .with_pdf_dir(dir.path().join("pdf-filings"));

    // The directory does not exist yet: the XBRL pass is unaffected.
    assert_eq!(s.ingest_new(), ["BUMI_2024_Q2", "BUMI_2024_Q3"]);
    // The owner copies the file in; the next tick picks it up, and then leaves it alone.
    write_pdf_dir(dir.path(), &fixture());
    assert_eq!(s.ingest_new(), ["BUMI_2024_Q1"]);
    assert!(s.ingest_new().is_empty());
    assert_eq!(s.store.pdf_filing_count().unwrap(), 1);
}

// ---- precedence ----

fn q1_xbrl(dir: &Path, revenue: f64) {
    xbrl_fixture::write(
        dir,
        &bumi_spec(
            "Q1",
            vec![("SalesAndRevenue", "CurrentYearDuration", revenue, 0, "USD")],
        ),
    );
}

#[test]
fn an_xbrl_filing_of_the_same_id_wins_over_the_pdf_one() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_dir = write_pdf_dir(dir.path(), &fixture());
    let xbrl_dir = dir.path().join("XBRL");
    q1_xbrl(&xbrl_dir, 311_000_000.0);
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(report.ingested, ["BUMI_2024_Q1"]);
    assert_eq!(report.pdf_superseded, ["BUMI_2024_Q1"]);
    assert!(
        report.unparsed.is_empty(),
        "kept out is not an error: {:?}",
        report.unparsed
    );
    assert!(store.ingest_errors().unwrap().is_empty());
    let row = store.filing("BUMI_2024_Q1").unwrap().unwrap();
    assert_eq!(row.filing.source_kind, SourceKind::Xbrl);
    assert_eq!(store.pdf_filing_count().unwrap(), 0);
    let sales = store
        .fact(
            "BUMI_2024_Q1",
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            Some("2024-01-01"),
            "2024-03-31",
            "",
        )
        .unwrap()
        .unwrap();
    assert_eq!(sales.value, 311_000_000.0);
    assert!(sales.cite.is_none());
}

#[test]
fn an_xbrl_filing_that_arrives_later_replaces_the_pdf_one() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_dir = write_pdf_dir(dir.path(), &fixture());
    let xbrl_dir = dir.path().join("XBRL");
    std::fs::create_dir_all(&xbrl_dir).unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(store.pdf_filing_count().unwrap(), 1);

    // The scraper finally gets the XBRL: the PDF facts go, no duplicate-id error.
    q1_xbrl(&xbrl_dir, 311_000_000.0);
    let later = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(later.ingested, ["BUMI_2024_Q1"]);
    assert_eq!(later.pdf_superseded, ["BUMI_2024_Q1"]);
    assert!(later.unparsed.is_empty(), "{:?}", later.unparsed);
    let row = store.filing("BUMI_2024_Q1").unwrap().unwrap();
    assert_eq!(row.filing.source_kind, SourceKind::Xbrl);
    assert_eq!(row.fact_count, 1, "only the XBRL facts remain");
    assert!(store.pdf_cites("BUMI").unwrap().is_empty());
}

#[test]
fn scheduler_ticks_after_xbrl_took_over_leave_the_xbrl_facts_alone() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    q1_xbrl(&xbrl_dir, 311_000_000.0);
    write_pdf_dir(dir.path(), &fixture());
    let s = Scheduler::new(
        Arc::new(Store::open(&dir.path().join("research.sqlite")).unwrap()),
        YahooSource::new(Some("http://127.0.0.1:9/never-called")),
        NoHeld,
        xbrl_dir,
        dir.path().join("research/watchlist.csv"),
        Arc::new(Utc::now) as Clock,
    )
    .with_pdf_dir(dir.path().join("pdf-filings"));
    assert_eq!(s.ingest_new(), ["BUMI_2024_Q1"]);
    let hash = s
        .store
        .filing("BUMI_2024_Q1")
        .unwrap()
        .unwrap()
        .filing
        .file_hash;
    for _ in 0..3 {
        assert!(
            s.ingest_new().is_empty(),
            "the superseded PDF file ingests nothing"
        );
    }
    let row = s.store.filing("BUMI_2024_Q1").unwrap().unwrap();
    assert_eq!(row.filing.source_kind, SourceKind::Xbrl);
    assert_eq!(row.filing.file_hash, hash);
    assert_eq!(row.fact_count, 1);
    assert!(s.store.ingest_errors().unwrap().is_empty());
    // A reingest with both present keeps the XBRL facts too.
    let xbrl_dir = dir.path().join("XBRL");
    let re = xbrl::reingest_all_with_pdf(
        &s.store,
        &xbrl_dir,
        Some(&dir.path().join("pdf-filings")),
        now(),
    )
    .unwrap();
    assert_eq!(re.pdf_superseded, ["BUMI_2024_Q1"]);
    assert_eq!(
        s.store.filing("BUMI_2024_Q1").unwrap().unwrap().fact_count,
        1
    );
}

#[test]
fn two_pdf_files_for_one_filing_id_keep_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_dir = write_pdf_dir(dir.path(), &fixture());
    std::fs::copy(fixture_path(), pdf_dir.join("BUMI_2024_Q1_copy.json")).unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    std::fs::create_dir_all(&xbrl_dir).unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(report.ingested.len(), 1);
    assert_eq!(report.unparsed.len(), 1);
    assert!(report.unparsed[0]
        .1
        .contains("duplicate filing id BUMI_2024_Q1"));
}

// ---- derivation: the gap the filing closes ----

#[test]
fn the_pdf_filing_is_the_original_q1_that_h1_minus_q1_needs() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();

    // Before: no Q1 original, so every Q2 standalone value is a gap.
    xbrl::ingest_new(&store, &xbrl_dir, now()).unwrap();
    let before = derived_cell(&store, "idx-cor:SalesAndRevenue", 2);
    assert_eq!(before.standalone, None);
    assert!(before.flags.contains(&Flag::Gap));
    assert!(store
        .derived("BUMI", Some("idx-cor:Equity"))
        .unwrap()
        .iter()
        .all(|r| r.q != 1));

    // After: the PDF filing is picked up by the next pass.
    let pdf_dir = write_pdf_dir(dir.path(), &fixture());
    let report = xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    assert_eq!(report.ingested, ["BUMI_2024_Q1"]);
    assert!(report.reconciliation_failures.is_empty());

    let q1_revenue = fixture_value("idx-cor:SalesAndRevenue");
    let q2 = derived_cell(&store, "idx-cor:SalesAndRevenue", 2);
    assert_eq!(q2.standalone, Some(H1_REVENUE - q1_revenue));
    assert_eq!(q2.standalone, Some(284_825_145.0));
    assert!(!q2.flags.contains(&Flag::Gap) && !q2.flags.contains(&Flag::FromComparative));
    let operands: Vec<(&str, &str)> = q2
        .inputs
        .standalone
        .iter()
        .map(|i| (i.role.as_str(), i.fact.filing_id.as_str()))
        .collect();
    assert_eq!(
        operands,
        [("ytd_q2", "BUMI_2024_Q2"), ("ytd_q1", "BUMI_2024_Q1")]
    );

    // Q1 itself: the YTD figure, and the balance-sheet items that were gaps.
    let q1 = derived_cell(&store, "idx-cor:SalesAndRevenue", 1);
    assert_eq!(q1.standalone, Some(q1_revenue));
    let equity = derived_cell(&store, "idx-cor:Equity", 1);
    assert_eq!(equity.standalone, Some(fixture_value("idx-cor:Equity")));
    let cash = derived_cell(&store, "idx-cor:CashAndCashEquivalents", 1);
    assert_eq!(
        cash.standalone,
        Some(fixture_value("idx-cor:CashAndCashEquivalents"))
    );
    // The nine-month filing differences against H1 as before.
    let q3 = derived_cell(&store, "idx-cor:SalesAndRevenue", 3);
    assert_eq!(q3.standalone, Some(M9_REVENUE - H1_REVENUE));
}

// ---- the screens' payloads ----

fn metric<'a>(v: &'a Value, id: &str) -> Option<&'a Value> {
    match v {
        Value::Object(m) => {
            if m.get("id").and_then(Value::as_str) == Some(id) && m.contains_key("cells") {
                return Some(v);
            }
            m.values().find_map(|x| metric(x, id))
        }
        Value::Array(a) => a.iter().find_map(|x| metric(x, id)),
        _ => None,
    }
}

fn cell<'a>(metric: &'a Value, label: &str) -> &'a Value {
    metric["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == label)
        .unwrap_or_else(|| panic!("no cell {label}"))
}

fn seeded_app(dir: &Path) -> common::TestApp {
    let xbrl_dir = dir.join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    let pdf_dir = write_pdf_dir(dir, &fixture());
    let db = dir.join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
        sync(
            &store,
            &[HeldPosition {
                ticker: "BUMI".into(),
                avg_price: 100.0,
            }],
            NOW,
        )
        .unwrap();
    }
    common::test_app_with(|cfg| {
        cfg.idx_xbrl_dir = xbrl_dir.clone();
        cfg.research_pdf_filings_dir = pdf_dir.clone();
        cfg.research_db_path = db.clone();
    })
}

#[tokio::test]
async fn the_filings_list_and_the_fact_drawer_say_pdf_and_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let t = seeded_app(dir.path());
    let (s, v) = t.get("/api/research/filings?ticker=BUMI").await;
    assert_eq!(s, StatusCode::OK);
    let kind = |id: &str| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap()["source_kind"]
            .clone()
    };
    assert_eq!(kind("BUMI_2024_Q1"), "pdf");
    assert_eq!(kind("BUMI_2024_Q2"), "xbrl");

    let (s, v) = t
        .get("/api/research/facts/BUMI_2024_Q1/idx-cor:Equity?ctx=2024-03-31")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["filing"]["source_kind"], "pdf");
    assert_eq!(v["fact"]["cite"]["kind"], "pdf");
    assert_eq!(v["fact"]["cite"]["page"], 6);
    assert!(v["fact"]["cite"]["label"]
        .as_str()
        .unwrap()
        .contains("Ekuitas - Neto"));

    // A derived cell names the PDF page of its Q1 operand; an XBRL fact has no cite.
    let (s, v) = t
        .get("/api/research/facts/BUMI_2024_Q1/idx-cor:SalesAndRevenue?ctx=2024-01-01_2024-03-31")
        .await;
    assert_eq!(s, StatusCode::OK);
    let q2 = v["derived"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["fy"] == 2024 && d["q"] == 2 && d["view"] == "standalone")
        .expect("Q2 standalone is derived from the Q1 fact");
    let operands = q2["inputs"].as_array().unwrap();
    let from_pdf: Vec<&Value> = operands
        .iter()
        .filter(|i| i["cite"]["kind"] == "pdf")
        .collect();
    assert_eq!(from_pdf.len(), 1);
    assert_eq!(from_pdf[0]["role"], "ytd_q1");
    assert_eq!(from_pdf[0]["cite"]["page"], 7);
    let (_, h1) = t
        .get("/api/research/facts/BUMI_2024_Q2/idx-cor:SalesAndRevenue?ctx=2024-01-01_2024-06-30")
        .await;
    assert!(h1["fact"]["cite"].is_null());
    assert_eq!(h1["filing"]["source_kind"], "xbrl");
}

#[tokio::test]
async fn the_quarter_table_marks_a_period_that_comes_from_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let t = seeded_app(dir.path());
    let (s, v) = t.get("/api/research/BUMI/financials?view=standalone").await;
    assert_eq!(s, StatusCode::OK);
    let kinds: Vec<(String, Value)> = v["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["label"].as_str().unwrap().to_string(),
                p["source_kind"].clone(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("2024Q1".to_string(), json!("pdf")),
            ("2024Q2".to_string(), json!("xbrl")),
            ("2024Q3".to_string(), json!("xbrl")),
        ]
    );
}

#[tokio::test]
async fn pack_cells_resting_on_the_pdf_filing_cite_it_with_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let t = seeded_app(dir.path());
    let (s, v) = t.get("/api/research/BUMI/pack").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["pack"]["id"], "coal_mining");

    // Q1 net debt: the three bank-loan lines less cash, all read from the PDF.
    let debt = [
        "ShortTermBankLoans",
        "LongTermBankLoans",
        "CurrentMaturitiesOfBankLoans",
    ]
    .iter()
    .map(|e| fixture_value(&format!("idx-cor:{e}")))
    .sum::<f64>();
    let want = debt - fixture_value("idx-cor:CashAndCashEquivalents");
    let net_debt = metric(&v, "net_debt").expect("net_debt metric");
    let q1 = cell(net_debt, "2024Q1");
    assert_eq!(q1["value"].as_f64(), Some(want));
    let prov = q1["sources"].as_array().expect("Q1 net debt cites the PDF");
    assert!(prov
        .iter()
        .all(|p| p["kind"] == "pdf" && p["filing_id"] == "BUMI_2024_Q1"));
    let mut pages: Vec<(String, i64)> = prov
        .iter()
        .map(|p| {
            (
                p["element"].as_str().unwrap().to_string(),
                p["page"].as_i64().unwrap(),
            )
        })
        .collect();
    pages.sort();
    assert_eq!(
        pages,
        [
            ("idx-cor:CashAndCashEquivalents".to_string(), 5),
            ("idx-cor:CurrentMaturitiesOfBankLoans".to_string(), 6),
            ("idx-cor:LongTermBankLoans".to_string(), 6),
            ("idx-cor:ShortTermBankLoans".to_string(), 6),
        ]
    );

    // Equity at Q1 is populated; a cell that rests on XBRL only has no provenance.
    let equity = metric(&v, "total_equity").unwrap();
    assert_eq!(
        cell(equity, "2024Q1")["value"].as_f64(),
        Some(fixture_value("idx-cor:Equity"))
    );
    assert!(cell(equity, "2024Q2").get("sources").is_none());

    // Q2 standalone revenue = H1 - Q1, citing the Q1 operand's PDF page.
    let revenue = metric(&v, "revenue").unwrap();
    let q2 = cell(revenue, "2024Q2");
    assert_eq!(q2["value"].as_f64(), Some(284_825_145.0));
    let prov = q2["sources"].as_array().unwrap();
    assert_eq!(prov.len(), 1);
    assert_eq!(
        (prov[0]["filing_id"].as_str(), prov[0]["page"].as_i64()),
        (Some("BUMI_2024_Q1"), Some(7))
    );
}

#[tokio::test]
async fn health_counts_the_pdf_filings() {
    let dir = tempfile::tempdir().unwrap();
    let t = seeded_app(dir.path());
    let (s, v) = t.get("/api/health?deep=1").await;
    assert_eq!(s, StatusCode::OK);
    let e = &v["sources"]["research_filings"];
    assert_eq!(e["filings"], 3);
    assert_eq!(e["pdf_filings"], 1);
    assert!(e["detail"]
        .as_str()
        .unwrap()
        .contains("1 from original PDF"));
}

#[tokio::test]
async fn a_rejected_pdf_file_turns_health_red_and_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    let mut doc = fixture();
    doc["facts"][0]["page"] = json!(0);
    let pdf_dir = write_pdf_dir(dir.path(), &doc);
    let db = dir.path().join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        xbrl::ingest_new_with_pdf(&store, &xbrl_dir, Some(&pdf_dir), now(), None).unwrap();
    }
    let t = common::test_app_with(|cfg| cfg.research_db_path = db.clone());
    let (_, v) = t.get("/api/health?deep=1").await;
    let e = &v["sources"]["research_filings"];
    assert_eq!(e["ok"], false);
    assert_eq!(e["unparsed"], 1);
    assert_eq!(e["pdf_filings"], 0);
    let detail = e["detail"].as_str().unwrap();
    assert!(
        detail.contains("BUMI_2024_Q1.json") && detail.contains("page 0"),
        "{detail}"
    );
    assert!(
        !detail.contains("original PDF"),
        "no PDF filing, so the plain wording: {detail}"
    );
}

// ---- migration 012 ----

#[test]
fn migration_012_keeps_existing_rows_as_xbrl_and_can_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    {
        let old = Store::open_at_version(&db, 11).unwrap();
        assert_eq!(old.user_version().unwrap(), 11);
        old.with_conn(|c| {
            // Rows as a v11 build wrote them: the old column lists.
            c.execute_batch(
                "INSERT INTO filings (id, ticker, fy, period, taxonomy, industry_code, period_start, period_end,
                   audited, rounding, currency, filed_at, available_from, available_source, source_path, file_hash, ingested_at)
                 VALUES ('BUMI_2024_Q2', 'BUMI', 2024, 'Q2', 'general', 'A121', '2024-01-01', '2024-06-30',
                   0, 'Satuan Penuh', 'USD', NULL, '2024-08-29', 'estimated', 'old.xbrl', 'h', '2026-10-01T00:00:00Z');
                 INSERT INTO facts (filing_id, element, ctx_kind, start, end_, dims, value, unit, decimals)
                 VALUES ('BUMI_2024_Q2', 'idx-cor:Equity', 'instant', NULL, '2024-06-30', '', 1.0, 'USD', 0);",
            )?;
            Ok(())
        })
        .unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    let filings = store.filings(Some("BUMI")).unwrap();
    assert_eq!(filings.len(), 1);
    assert_eq!(filings[0].filing.source_kind, SourceKind::Xbrl);
    let fact = store
        .fact(
            "BUMI_2024_Q2",
            "idx-cor:Equity",
            CtxKind::Instant,
            None,
            "2024-06-30",
            "",
        )
        .unwrap()
        .unwrap();
    assert!(fact.cite.is_none());
    // The constraints hold on the new columns.
    store
        .with_conn(|c| {
            assert!(c
                .execute("UPDATE filings SET source_kind = 'scan'", [])
                .is_err());
            assert!(c.execute("UPDATE facts SET page = 0", []).is_err());
            Ok(())
        })
        .unwrap();
    drop(store);

    // A store rolled back by setting user_version (deploy/RUNBOOK.md) still has the columns:
    // the next start must not fail on "duplicate column name".
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("PRAGMA user_version = 11").unwrap();
    }
    let again = Store::open(&db).unwrap();
    assert_eq!(again.user_version().unwrap(), latest_version());
    assert_eq!(again.filings(Some("BUMI")).unwrap(), filings);
}

#[test]
fn migration_012_keeps_existing_filings_as_xbrl() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    {
        let old = Store::open_at_version(&db, 11).unwrap();
        assert_eq!(old.user_version().unwrap(), 11);
    }
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('filings')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(!cols.contains(&"source_kind".to_string()));
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), 12);
    let xbrl_dir = dir.path().join("XBRL");
    write_bumi_xbrl(&xbrl_dir);
    xbrl::ingest_new(&store, &xbrl_dir, now()).unwrap();
    let before = store.filings(Some("BUMI")).unwrap();
    drop(store);
    let again = Store::open(&db).unwrap();
    assert_eq!(again.user_version().unwrap(), 12);
    assert_eq!(again.filings(Some("BUMI")).unwrap(), before);
    assert!(before
        .iter()
        .all(|f| f.filing.source_kind == SourceKind::Xbrl));
    assert_eq!(again.pdf_filing_count().unwrap(), 0);
}
