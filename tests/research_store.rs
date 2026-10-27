//! Research store: schema, idempotent migrations, JSONL log and rebuild (plan §6.1, M1 #1, #6).

use investing_app::research::store::{
    latest_version, ManualShares, PriceBar, Store, StoreError, LOG_FILE,
};
use investing_app::research::universe::{Role, UniverseEntry};

const AT: &str = "2026-10-22T09:00:00+07:00";

fn tables(store: &Store) -> Vec<String> {
    store
        .with_conn(|c| {
            let mut st = c.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

#[test]
fn creates_every_table_and_migrations_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");

    let store = Store::open(&path).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    let first = tables(&store);
    assert_eq!(
        first,
        [
            "calendar_manual",
            "claim_evals",
            "claims",
            "corp_events",
            "derived",
            "documents",
            "drivers",
            "events",
            "evidence",
            "extraction_proposals",
            "facts",
            "filings",
            "held_positions",
            "ingest_errors",
            "journal",
            "journal_annotations",
            "kpi_values",
            "prices_daily",
            "restatements",
            "scenarios",
            "seen",
            "shares_manual",
            "source_runs",
            "structure_maps",
            "universe",
        ]
    );

    store.migrate().unwrap();
    store.migrate().unwrap();
    drop(store);

    let again = Store::open(&path).unwrap();
    assert_eq!(again.user_version().unwrap(), latest_version());
    assert_eq!(tables(&again), first);
}

#[test]
fn uses_wal_and_refuses_a_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let store = Store::open(&path).unwrap();
    let mode: String = store
        .with_conn(|c| Ok(c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(mode, "wal");

    store
        .with_conn(|c| {
            c.execute_batch("PRAGMA user_version = 99")?;
            Ok(())
        })
        .unwrap();
    drop(store);
    match Store::open(&path) {
        Err(StoreError::TooNew { found: 99, .. }) => {}
        other => panic!("expected TooNew, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn check_constraints_reject_bad_rows() {
    let store = Store::open_memory().unwrap();
    let bad_shares = ManualShares {
        ticker: "AAAA".into(),
        shares: 1_000.0,
        as_of: "2026-09-30".into(),
        source_note: String::new(),
        entered_at: AT.into(),
    };
    assert!(store.set_shares_manual(&bad_shares).is_err());
    let bad_ticker = UniverseEntry {
        ticker: "aaa1".into(),
        role: Role::Manual,
        peer_of: None,
        pack_override: None,
        business_note: None,
        added_at: AT.into(),
        synced_at: AT.into(),
    };
    assert!(store.upsert_universe(&bad_ticker).is_err());
}

fn manual(ticker: &str) -> UniverseEntry {
    UniverseEntry {
        ticker: ticker.into(),
        role: Role::Manual,
        peer_of: None,
        pack_override: None,
        business_note: Some("catatan".into()),
        added_at: AT.into(),
        synced_at: AT.into(),
    }
}

fn seed_user_data(store: &Store) {
    store.upsert_universe(&manual("BBBB")).unwrap();
    store.upsert_universe(&manual("CCCC")).unwrap();
    store.delete_universe("CCCC", AT).unwrap();
    store
        .set_shares_manual(&ManualShares {
            ticker: "BBBB".into(),
            shares: 2_000_000.0,
            as_of: "2026-09-30".into(),
            source_note: "annual report p.3".into(),
            entered_at: AT.into(),
        })
        .unwrap();
    store.set_seen("BBBB", AT).unwrap();
}

fn assert_seeded(store: &Store) {
    let tickers: Vec<String> = store
        .universe()
        .unwrap()
        .into_iter()
        .map(|e| e.ticker)
        .collect();
    assert_eq!(tickers, ["BBBB"]);
    assert_eq!(
        store.shares_manual("BBBB").unwrap().unwrap().source_note,
        "annual report p.3"
    );
    assert_eq!(store.seen("BBBB").unwrap().unwrap().seen_at, AT);
}

#[test]
fn user_data_survives_losing_the_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let store = Store::open(&path).unwrap();
    seed_user_data(&store);
    assert_seeded(&store);
    drop(store);

    for f in [
        "research.sqlite",
        "research.sqlite-wal",
        "research.sqlite-shm",
    ] {
        let _ = std::fs::remove_file(dir.path().join(f));
    }
    assert!(dir.path().join(LOG_FILE).exists());

    let restored = Store::open(&path).unwrap();
    assert_seeded(&restored);
}

#[test]
fn rebuild_replays_the_log_and_skips_torn_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let store = Store::open(&path).unwrap();
    seed_user_data(&store);

    // Tamper with the live tables, then rebuild from the log.
    store
        .with_conn(|c| {
            c.execute_batch("DELETE FROM shares_manual; INSERT INTO seen VALUES ('ZZZZ','t')")?;
            Ok(())
        })
        .unwrap();
    // A torn final line (crash mid-append) must not stop the replay.
    let log = dir.path().join(LOG_FILE);
    let mut text = std::fs::read_to_string(&log).unwrap();
    text.push_str("{\"op\":\"shares_manual_set\",\"tick");
    std::fs::write(&log, text).unwrap();

    let applied = store.rebuild().unwrap();
    assert_eq!(applied, 5);
    assert_seeded(&store);
    assert_eq!(store.rebuild().unwrap(), 5);
    assert_seeded(&store);
}

#[test]
fn deletes_are_logged_and_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let store = Store::open(&path).unwrap();
    store.upsert_universe(&manual("AAAA")).unwrap();
    store.delete_universe("AAAA", AT).unwrap();
    store.delete_shares_manual("AAAA", AT).unwrap();
    drop(store);
    std::fs::remove_file(&path).unwrap();
    let restored = Store::open(&path).unwrap();
    assert!(restored.universe_entry("AAAA").unwrap().is_none());
}

#[test]
fn prices_are_upsert_only() {
    let store = Store::open_memory().unwrap();
    let bar = |date: &str, close: f64| PriceBar {
        date: date.into(),
        open: None,
        high: None,
        low: None,
        close,
        volume: Some(10),
    };
    store
        .upsert_prices(
            "AAAA",
            &[bar("2026-10-20", 100.0), bar("2026-10-21", 101.0)],
        )
        .unwrap();
    store.upsert_prices("AAAA", &[]).unwrap();
    store
        .upsert_prices("AAAA", &[bar("2026-10-21", 102.0)])
        .unwrap();
    assert_eq!(store.price_count("AAAA").unwrap(), 2);
}
