//! A rules-version bump reaches data already in the store (research plan §4.4): tickers
//! whose derived cells were computed under an older `RULES_VERSION` are re-derived at
//! startup and by `research-ingest`, rewriting only `derived` and `restatements`.
//! Synthetic filings only; the "old" stamp is written straight into the table.

#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::path::Path;

use chrono::{TimeZone, Utc};
use finance_app::research::derive::RULES_VERSION;
use finance_app::research::store::Store;
use finance_app::research::xbrl;
use rusqlite::Connection;
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
const OLD: &str = "2026.10-r1";
const OWNER_TABLES: [&str; 8] = [
    "journal",
    "held_positions",
    "universe",
    "shares_manual",
    "prices_daily",
    "filings",
    "facts",
    "ingest_errors",
];

/// A store with two tickers ingested (AAAA with a 2025 restatement of 2024 Q2) and some
/// owner rows. Returns the store path.
fn seeded(dir: &Path) -> std::path::PathBuf {
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
    general_year(&xbrl_dir, "BBBB", &Y2024, None, &[], None);
    let db = dir.join("research.sqlite");
    let store = Store::open(&db).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap();
    let report = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
    assert!(report.unparsed.is_empty(), "{:?}", report.unparsed);
    drop(store);
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "INSERT INTO held_positions (ticker, avg_price) VALUES ('AAAA', 1000.0);
         INSERT INTO universe (ticker, role, added_at, synced_at) VALUES ('AAAA', 'held', '2026-10-01', '2026-10-01');
         INSERT INTO shares_manual (ticker, shares, as_of, source_note, entered_at) VALUES ('AAAA', 5000.0, '2026-06-30', 'synthetic', '2026-10-01');
         INSERT INTO prices_daily (ticker, date, close) VALUES ('AAAA', '2026-10-23', 1010.0);
         INSERT INTO journal (id, ticker, at, kind, text) VALUES ('j1', 'AAAA', '2026-10-02', 'catatan', 'synthetic note');",
    )
    .unwrap();
    db
}

fn stamp(db: &Path, ticker: &str, version: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "UPDATE derived SET rules_version = ?2 WHERE ticker = ?1",
        [ticker, version],
    )
    .unwrap();
}

/// Every row of the owner tables (and the ingested filings), as text, for before/after.
fn dump(db: &Path) -> Vec<(String, Vec<String>)> {
    let conn = Connection::open(db).unwrap();
    OWNER_TABLES
        .iter()
        .map(|t| {
            let mut st = conn
                .prepare(&format!("SELECT * FROM {t} ORDER BY rowid"))
                .unwrap();
            let n = st.column_count();
            let rows = st
                .query_map([], |r| {
                    Ok((0..n)
                        .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                        .collect::<Vec<_>>()
                        .join("|"))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            (t.to_string(), rows)
        })
        .collect()
}

fn versions(db: &Path, ticker: &str) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut st = conn
        .prepare("SELECT DISTINCT rules_version FROM derived WHERE ticker = ?1")
        .unwrap();
    st.query_map([ticker], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn computed_at(db: &Path, ticker: &str) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut st = conn
        .prepare("SELECT DISTINCT computed_at FROM derived WHERE ticker = ?1")
        .unwrap();
    st.query_map([ticker], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

const LATER: &str = "2026-10-25T09:00:00+00:00";

#[test]
fn cells_stamped_under_an_older_version_are_rederived_and_restamped() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    assert_eq!(versions(&db, "AAAA"), [RULES_VERSION]);
    stamp(&db, "AAAA", OLD);
    stamp(&db, "BBBB", OLD);
    let store = Store::open(&db).unwrap();
    assert_eq!(store.tickers_with_stale_rules().unwrap(), ["AAAA", "BBBB"]);
    let owner_before = dump(&db);
    let restated_before = store.restated_years().unwrap();
    // A wrong cell value and a stale restatement row, as an older rule set could have left.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE derived SET standalone = -1.0 WHERE ticker = 'BBBB' AND q = 1",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO restatements (ticker, element, start, end_, dims, original, original_filing, restated, restated_filing, detected_at)
             VALUES ('BBBB', 'idx-cor:Revenue', '2020-01-01', '2020-12-31', '', 1.0, 'BBBB_2020_FY', 2.0, 'BBBB_2021_FY', 'old')",
            [],
        )
        .unwrap();
    }

    let report = xbrl::rederive_stale(&store, LATER).unwrap();

    assert_eq!(report.tickers, ["AAAA", "BBBB"]);
    assert_eq!(versions(&db, "AAAA"), [RULES_VERSION]);
    assert_eq!(versions(&db, "BBBB"), [RULES_VERSION]);
    assert_eq!(computed_at(&db, "AAAA"), [LATER]);
    assert!(store.tickers_with_stale_rules().unwrap().is_empty());
    assert_eq!(store.restated_years().unwrap(), restated_before);
    let conn = Connection::open(&db).unwrap();
    let wrong: i64 = conn
        .query_row(
            "SELECT count(*) FROM derived WHERE standalone = -1.0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wrong, 0, "cell values are recomputed, not just restamped");
    let stale_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM restatements WHERE detected_at = 'old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stale_rows, 0, "restatements are rewritten with the cells");
    assert_eq!(
        dump(&db),
        owner_before,
        "owner and filing tables are untouched"
    );
}

#[test]
fn a_second_pass_rederives_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    stamp(&db, "AAAA", OLD);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        xbrl::rederive_stale(&store, LATER).unwrap().tickers,
        ["AAAA"]
    );
    let after_first = computed_at(&db, "AAAA");

    let second = xbrl::rederive_stale(&store, "2026-10-26T09:00:00+00:00").unwrap();

    assert!(second.tickers.is_empty());
    assert!(second.reconciliation_failures.is_empty());
    assert_eq!(
        computed_at(&db, "AAAA"),
        after_first,
        "cells were not rewritten"
    );
}

#[test]
fn tickers_already_on_the_current_version_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    stamp(&db, "AAAA", OLD);
    let store = Store::open(&db).unwrap();
    let bbbb_before = computed_at(&db, "BBBB");

    let report = xbrl::rederive_stale(&store, LATER).unwrap();

    assert_eq!(report.tickers, ["AAAA"]);
    assert_eq!(computed_at(&db, "BBBB"), bbbb_before);
}

#[test]
fn a_ticker_with_facts_but_no_derived_rows_is_derived() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    Connection::open(&db)
        .unwrap()
        .execute("DELETE FROM derived WHERE ticker = 'BBBB'", [])
        .unwrap();
    let store = Store::open(&db).unwrap();
    assert_eq!(store.tickers_with_stale_rules().unwrap(), ["BBBB"]);

    let report = xbrl::rederive_stale(&store, LATER).unwrap();

    assert_eq!(report.tickers, ["BBBB"]);
    assert_eq!(versions(&db, "BBBB"), [RULES_VERSION]);
}

#[test]
fn a_ticker_whose_facts_are_all_dimensioned_is_not_selected() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    let conn = Connection::open(&db).unwrap();
    conn.execute("DELETE FROM derived WHERE ticker = 'BBBB'", [])
        .unwrap();
    conn.execute(
        "UPDATE facts SET dims = 'seg=x' WHERE filing_id IN (SELECT id FROM filings WHERE ticker = 'BBBB')",
        [],
    )
    .unwrap();
    let store = Store::open(&db).unwrap();

    assert!(store.tickers_with_stale_rules().unwrap().is_empty());
    assert!(xbrl::rederive_stale(&store, LATER)
        .unwrap()
        .tickers
        .is_empty());
}

#[test]
fn one_ticker_that_cannot_be_derived_does_not_stop_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    stamp(&db, "AAAA", OLD);
    stamp(&db, "BBBB", OLD);
    Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE facts SET end_ = 'not-a-date' WHERE rowid = (SELECT min(x.rowid) FROM facts x JOIN filings f ON f.id = x.filing_id WHERE f.ticker = 'AAAA' AND x.dims = '')",
            [],
        )
        .unwrap();
    let store = Store::open(&db).unwrap();

    let report = xbrl::rederive_stale(&store, LATER).unwrap();

    assert_eq!(report.tickers, ["BBBB"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "AAAA");
    assert_eq!(versions(&db, "BBBB"), [RULES_VERSION]);
    assert_eq!(
        versions(&db, "AAAA"),
        [OLD],
        "a failed ticker keeps its old cells"
    );
}
