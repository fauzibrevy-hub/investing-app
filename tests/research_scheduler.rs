//! Directory watch (research plan §4.2, R1.6): the scheduler ingests a filing when its file
//! hash is new, leaves unchanged files alone, re-ingests a changed file, records a
//! `source_runs` row per pass, and reports a missing directory instead of pretending.

#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use investing_app::research::packs::Packs;
use investing_app::research::prices::yahoo::YahooSource;
use investing_app::research::scheduler::{Clock, Scheduler};
use investing_app::research::sources::portfolio::HeldProvider;
use investing_app::research::sources::SourceError;
use investing_app::research::store::{ClaimRow, Store};
use investing_app::research::universe::{add, AddRequest, HeldPosition};
use investing_app::research::xbrl;
use xbrl_fixture::{general_year, Year};

struct NoHeld;

impl HeldProvider for NoHeld {
    async fn held(&self) -> Result<Vec<HeldPosition>, SourceError> {
        Ok(Vec::new())
    }
}

fn scheduler(dir: &Path) -> Scheduler<YahooSource, NoHeld> {
    Scheduler::new(
        Arc::new(Store::open(&dir.join("research.sqlite")).unwrap()),
        YahooSource::new(Some("http://127.0.0.1:9/never-called")),
        NoHeld,
        dir.join("XBRL"),
        dir.join("research/watchlist.csv"),
        Arc::new(Utc::now) as Clock,
    )
}

const Y2024: Year = Year {
    fy: 2024,
    revenue: [100.0, 230.0, 350.0, 500.0],
    profit: [10.0, 22.0, 33.0, 50.0],
    equity: [400.0, 410.0, 420.0, 440.0],
};

#[test]
fn new_and_changed_files_are_ingested_unchanged_ones_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    let paths = general_year(&xbrl_dir, "AAAA", &Y2024, None, &["FY"], None);
    let s = scheduler(dir.path());

    let first = s.ingest_new();
    assert_eq!(first, ["AAAA_2024_Q1", "AAAA_2024_Q2", "AAAA_2024_Q3"]);
    assert_eq!(s.store.filing_count().unwrap(), 3);
    assert!(s.store.derived("AAAA", None).unwrap().len() > 3);

    // Same files, same hashes: nothing happens, but the pass is still recorded.
    assert!(s.ingest_new().is_empty());
    let runs = s.store.source_runs(xbrl::SOURCE).unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r.ok));

    // The FY filing arrives: only it is ingested, and Q4 becomes derivable.
    general_year(&xbrl_dir, "AAAA", &Y2024, None, &["Q1", "Q2", "Q3"], None);
    assert_eq!(s.ingest_new(), ["AAAA_2024_FY"]);
    let q4 = s
        .store
        .derived("AAAA", Some("idx-cor:SalesAndRevenue"))
        .unwrap()
        .into_iter()
        .find(|c| c.fy == 2024 && c.q == 4)
        .unwrap();
    assert_eq!(q4.standalone, Some(150e6));

    // A changed file (new hash) replaces its facts; the filing id stays unique.
    let q1 = &paths[0];
    let text = std::fs::read_to_string(q1).unwrap();
    std::fs::write(q1, text.replace(">100000000<", ">101000000<")).unwrap();
    assert_eq!(s.ingest_new(), ["AAAA_2024_Q1"]);
    assert_eq!(s.store.filing_count().unwrap(), 4);
    let q1_cell = s
        .store
        .derived("AAAA", Some("idx-cor:SalesAndRevenue"))
        .unwrap()
        .into_iter()
        .find(|c| c.fy == 2024 && c.q == 1)
        .unwrap();
    assert_eq!(q1_cell.standalone, Some(101e6));
}

#[test]
fn a_missing_directory_is_recorded_not_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let s = scheduler(dir.path());
    assert!(s.ingest_new().is_empty());
    let runs = s.store.source_runs(xbrl::SOURCE).unwrap();
    assert_eq!(runs.len(), 1);
    assert!(!runs[0].ok);
    assert!(runs[0]
        .error
        .as_deref()
        .unwrap()
        .contains("xbrl dir not found"));
}

/// A4.1: a claim written before its data exists is re-evaluated after the scheduler ingests
/// the filing that carries it, without anyone opening the claim.
#[test]
fn ingest_then_react_re_evaluates_open_claims() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    let packs = Arc::new(Packs::embedded());
    let s = scheduler(dir.path()).with_packs(packs.clone());
    add(
        &s.store,
        &packs.set(),
        &AddRequest {
            ticker: "AAAA".into(),
            ..Default::default()
        },
        "2024-01-01T00:00:00+00:00",
    )
    .unwrap();
    s.store
        .insert_claim(&ClaimRow {
            id: "c-test".into(),
            ticker: "AAAA".into(),
            statement: "Pendapatan positif".into(),
            metric: "core.revenue".into(),
            comparator: ">".into(),
            threshold: 0.0,
            threshold_hi: None,
            window: "latest".into(),
            due: "2099-12-31".into(),
            status: "not_testable".into(),
            created_at: "2024-01-01T00:00:00+00:00".into(),
            closed_at: None,
        })
        .unwrap();
    // Nothing ingested yet: the first pass records "not testable" and the status holds.
    s.react();
    assert_eq!(s.store.claim_evals("c-test").unwrap().len(), 1);
    assert_eq!(
        s.store.claim("c-test").unwrap().unwrap().status,
        "not_testable"
    );

    general_year(&xbrl_dir, "AAAA", &Y2024, None, &["FY"], None);
    assert_eq!(s.ingest_new().len(), 3);
    s.react();
    let claim = s.store.claim("c-test").unwrap().unwrap();
    assert_eq!(claim.status, "holding");
    let evals = s.store.claim_evals("c-test").unwrap();
    assert_eq!(evals.len(), 2);
    assert!(evals[1].inputs.contains("operands"));
    // Another pass over the same data stores nothing.
    s.react();
    assert_eq!(s.store.claim_evals("c-test").unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_pass_rederives_a_store_stamped_under_an_older_rules_version() {
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("XBRL");
    general_year(&xbrl_dir, "AAAA", &Y2024, None, &[], None);
    let sched = scheduler(dir.path());
    sched.ingest_new();
    {
        let conn = rusqlite::Connection::open(dir.path().join("research.sqlite")).unwrap();
        conn.execute("UPDATE derived SET rules_version = '2026.10-r1'", [])
            .unwrap();
    }

    let first = sched.rederive_stale().await;
    let second = sched.rederive_stale().await;

    assert_eq!(first.tickers, ["AAAA"]);
    assert!(second.tickers.is_empty(), "a second start does nothing");
    let rows = sched.store.derived("AAAA", None).unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|r| r.rules_version == investing_app::research::derive::RULES_VERSION));
}
