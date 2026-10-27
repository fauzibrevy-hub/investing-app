//! v4 → canon store migration (research plan §4.5, PRD A1.8) on a synthetic v4 store built
//! from migrations 001 and 002 plus invented rows. Checks the backup, the one-transaction
//! upgrade, equal owner-data counts, journal entries with original timestamps and the close
//! on that date, idempotency (re-open), and the v8 drop step (005, registered in R5-E1) on a
//! store that was already migrated by R1: owner tables unchanged, v4 tables gone, a pre-R5
//! log replaying into the canon tables.

use std::path::Path;

use investing_app::research::store::{latest_version, Store, LOG_FILE};
use investing_app::research::universe;
use rusqlite::{params, Connection, OpenFlags};

const NOTE_TEXT: &str = "Catatan sintetis: margin membaik dua kuartal berturut-turut.";

/// A v4 store (schema v2) with the owner's kinds of data, all invented.
fn build_v4(dir: &Path) {
    let db = dir.join("research.sqlite");
    let store = Store::open_at_version(&db, 2).unwrap();
    assert_eq!(store.user_version().unwrap(), 2);
    store
        .with_conn(|c| {
            c.execute_batch(
                r#"
            INSERT INTO universe (ticker, role, origins, synced_at) VALUES
              ('AAAA', 'held', '["portfolio"]', '2026-10-01T09:00:00+00:00'),
              ('BBBB', 'watch', '["manual"]', '2026-10-01T09:00:00+00:00');
            INSERT INTO universe_overrides (ticker, action, at) VALUES
              ('BBBB', 'add', '2026-09-15T10:00:00+00:00'),
              ('CCCC', 'add', '2026-09-20T10:00:00+00:00'),
              ('DDDD', 'remove', '2026-09-21T10:00:00+00:00');
            INSERT INTO prices_daily (ticker, date, close, adj_close) VALUES
              ('AAAA', '2026-10-01', 1000, 1000), ('AAAA', '2026-10-02', 1010, 1010),
              ('AAAA', '2026-10-05', 1030, 1030), ('AAAA', '2026-10-06', 1040, 1040),
              ('BBBB', '2026-10-01', 250, 250), ('BBBB', '2026-10-02', 255, 255);
            INSERT INTO notes (id, ticker, created_at, text) VALUES
              ('n1', 'AAAA', '2026-10-02T13:00:00+00:00', 'Catatan satu'),
              ('n2', 'AAAA', '2026-10-04T08:00:00+00:00', 'Catatan di akhir pekan'),
              ('n3', 'BBBB', '2026-09-30T08:00:00+00:00', 'Sebelum ada harga'),
              ('n5', 'AAAA', '2026-10-06T03:00:00+00:00', 'Catatan pagi, bursa belum tutup');
            INSERT INTO targets (ticker, entry_price, set_at) VALUES ('AAAA', 950, '2026-10-05T12:00:00+00:00');
            INSERT INTO shares_manual (ticker, shares, as_of, source_note, entered_at) VALUES
              ('AAAA', 1234567, '2026-06-30', 'prospektus sintetis', '2026-10-01T00:00:00+00:00'),
              ('CCCC', 7654321, '2026-06-30', 'keterbukaan sintetis', '2026-10-01T00:00:00+00:00');
            INSERT INTO shares (ticker, as_of, shares, source) VALUES
              ('AAAA', '2026-06-30', 1300000, 'xbrl_current'),
              ('AAAA', '2026-06-30', 1250000, 'xbrl_period'),
              ('BBBB', '2025-12-31', 500000, 'xbrl_period'),
              ('BBBB', '2026-06-30', 510000, 'xbrl_current');
            INSERT INTO seen (ticker, seen_at, eligibility, zone) VALUES ('AAAA', '2026-10-06T00:00:00+00:00', 'x', 'y');
            INSERT INTO snapshots (ticker, taken_at, rules_version, eligibility, zone, content_hash, payload)
              VALUES ('AAAA', '2026-10-06T00:00:00+00:00', '2026.10-2', 'x', 'y', 'h', '{}');
            INSERT INTO fundamentals_q (ticker, fy, q, metric, ytd, standalone, ttm, available_from, available_source)
              VALUES ('AAAA', 2026, 2, 'revenue', 10, 5, 20, '2026-08-29', 'estimated');
            INSERT INTO held_positions (ticker, avg_price) VALUES ('AAAA', 900);
            "#,
            )?;
            Ok(())
        })
        .unwrap();
    // A v4 log line, as the running v4 app would have written it.
    std::fs::write(
        dir.join(LOG_FILE),
        r#"{"op":"note_add","id":"n1","ticker":"AAAA","created_at":"2026-10-02T13:00:00+00:00","text":"Catatan satu"}"#,
    )
    .unwrap();
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// `(id, ticker, at, kind, text, price_at, level)`.
type JournalRow = (
    String,
    String,
    String,
    String,
    String,
    Option<f64>,
    Option<f64>,
);

fn journal(store: &Store) -> Vec<JournalRow> {
    store
        .with_conn(|c| {
            let mut st = c.prepare(
                "SELECT id, ticker, at, kind, text, price_at, level FROM journal ORDER BY id",
            )?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn today_suffix() -> String {
    format!("v4-{}", chrono::Utc::now().format("%Y-%m-%d"))
}

#[test]
fn v4_store_is_backed_up_migrated_and_owner_data_carried_over() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let db = dir.path().join("research.sqlite");

    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    // R3.1 (007): adj_close is gone and every v4 bar survived the table rebuild.
    store
        .with_conn(|c| {
            let mut st = c.prepare("PRAGMA table_info(prices_daily)")?;
            let cols: Vec<String> = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            assert!(!cols.contains(&"adj_close".to_string()), "{cols:?}");
            assert_eq!(count(c, "SELECT COUNT(*) FROM prices_daily"), 6);
            Ok(())
        })
        .unwrap();

    // Backup: a byte-for-byte usable v4 database next to the live one.
    let backup = dir
        .path()
        .join(format!("research.sqlite.{}.bak", today_suffix()));
    assert!(backup.exists(), "backup missing");
    let b = Connection::open_with_flags(&backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(count(&b, "PRAGMA user_version"), 2);
    assert_eq!(count(&b, "SELECT COUNT(*) FROM notes"), 4);
    // The v4 log is kept under the same suffix; the new one is seeded with the current
    // owner data (3 overrides, 2 manual counts, 1 target, 4 notes) so `rebuild` stays whole.
    assert!(dir
        .path()
        .join(format!("{LOG_FILE}.{}.bak", today_suffix()))
        .exists());
    let new_log = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
    assert_eq!(
        new_log.lines().count(),
        12,
        "10 seeded + 2 carried v4 share snapshots"
    );
    assert!(new_log.contains("\"op\":\"note_add\"") && new_log.contains("\"op\":\"target_set\""));

    // R3.2 (D13 option a): the v4 `shares` snapshot becomes manual counts with a source
    // note, one per (ticker, as_of), the owner's own entry on the same date untouched,
    // and a second open carries nothing twice.
    let carried = |s: &Store| -> Vec<(String, String, f64, String)> {
        s.with_conn(|c| {
            let mut st = c.prepare(
                "SELECT ticker, as_of, shares, source_note FROM shares_manual ORDER BY ticker, as_of",
            )?;
            let v = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap()
    };
    let rows = carried(&store);
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(rows[0].3, "prospektus sintetis");
    assert_eq!(
        rows[0].2, 1_234_567.0,
        "the owner's entry wins over the snapshot"
    );
    assert_eq!(
        (
            rows[1].0.as_str(),
            rows[1].1.as_str(),
            rows[1].2,
            rows[1].3.as_str()
        ),
        ("BBBB", "2025-12-31", 500_000.0, "v4 scraper snapshot")
    );
    assert_eq!((rows[2].1.as_str(), rows[2].2), ("2026-06-30", 510_000.0));
    assert_eq!(rows[3].3, "keterbukaan sintetis");
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(carried(&store), rows);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(LOG_FILE))
            .unwrap()
            .lines()
            .count(),
        12
    );
    // The carry is one-time (review R3): a carried count the owner deletes stays deleted
    // on the next open, even though the v4 `shares` table still holds it.
    store
        .delete_shares_manual("BBBB", "2026-10-24T00:00:00+00:00")
        .unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    assert!(
        carried(&store).iter().all(|r| r.0 != "BBBB"),
        "{:?}",
        carried(&store)
    );

    // Notes → journal `catatan` with the original timestamp and the close known then.
    let j = journal(&store);
    let notes: Vec<_> = j.iter().filter(|e| e.3 == "catatan").collect();
    assert_eq!(notes.len(), 4);
    let n5 = notes.iter().find(|e| e.0 == "v4note:n5").unwrap();
    assert_eq!(
        n5.5,
        Some(1030.0),
        "written before the close → the previous close"
    );
    let n1 = notes.iter().find(|e| e.0 == "v4note:n1").unwrap();
    assert_eq!(
        (n1.1.as_str(), n1.2.as_str(), n1.4.as_str()),
        ("AAAA", "2026-10-02T13:00:00+00:00", "Catatan satu")
    );
    assert_eq!(n1.5, Some(1010.0));
    let n2 = notes.iter().find(|e| e.0 == "v4note:n2").unwrap();
    assert_eq!(
        n2.5,
        Some(1010.0),
        "a weekend note takes the last close before it"
    );
    let n3 = notes.iter().find(|e| e.0 == "v4note:n3").unwrap();
    assert_eq!(
        n3.5, None,
        "no close on or before the date → null, never a guess"
    );

    // Targets → journal `level` with the entry price and the close that day.
    let levels: Vec<_> = j.iter().filter(|e| e.3 == "level").collect();
    assert_eq!(levels.len(), 1);
    let l = levels[0];
    assert_eq!(l.0, "v4target:AAAA:2026-10-05T12:00:00+00:00");
    assert_eq!(
        (l.2.as_str(), l.4.as_str(), l.5, l.6),
        (
            "2026-10-05T12:00:00+00:00",
            "Harga incaran v4",
            Some(1030.0),
            Some(950.0)
        )
    );

    // Manual shares and manual universe additions are present.
    assert_eq!(
        store.shares_manual("AAAA").unwrap().unwrap().shares,
        1234567.0
    );
    assert_eq!(
        store.shares_manual("CCCC").unwrap().unwrap().shares,
        7654321.0
    );
    store
        .with_conn(|c| {
            let role = |t: &str| -> String {
                c.query_row("SELECT role FROM universe WHERE ticker = ?1", [t], |r| {
                    r.get(0)
                })
                .unwrap()
            };
            assert_eq!(role("AAAA"), "held");
            assert_eq!(role("BBBB"), "manual", "watch becomes manual");
            assert_eq!(
                role("CCCC"),
                "manual",
                "an add without a universe row becomes manual"
            );
            let added: String = c
                .query_row(
                    "SELECT added_at FROM universe WHERE ticker = 'CCCC'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(added, "2026-09-20T10:00:00+00:00");
            let added_of = |t: &str| -> String {
                c.query_row(
                    "SELECT added_at FROM universe WHERE ticker = ?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap()
            };
            assert_eq!(
                added_of("BBBB"),
                "2026-09-15T10:00:00+00:00",
                "an existing manual row takes its override time"
            );
            assert_eq!(
                added_of("AAAA"),
                "2026-10-01T09:00:00+00:00",
                "a held row falls back to its sync time"
            );
            assert_eq!(
                count(c, "SELECT COUNT(*) FROM universe WHERE ticker = 'DDDD'"),
                0
            );
            // The v4 tables are gone (005 is registered); the owner data is in the canon tables.
            for gone in [
                "notes",
                "targets",
                "snapshots",
                "fundamentals_q",
                "shares",
                "universe_overrides",
            ] {
                assert_eq!(
                    count(
                        c,
                        &format!("SELECT COUNT(*) FROM sqlite_master WHERE name = '{gone}'")
                    ),
                    0,
                    "{gone}"
                );
            }
            assert_eq!(count(c, "SELECT COUNT(*) FROM seen"), 1);
            assert_eq!(count(c, "SELECT COUNT(*) FROM held_positions"), 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(store.universe().unwrap().len(), 3);
}

#[test]
fn universe_sync_keeps_canon_columns_peers_and_manual_role() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    store
        .with_conn(|c| {
            c.execute(
                "UPDATE universe SET business_note = 'Distributor sintetis', pack_override = 'it_distribution' WHERE ticker = 'BBBB'",
                [],
            )?;
            c.execute(
                "INSERT INTO universe (ticker, role, peer_of, added_at, synced_at)
                 VALUES ('EEEE', 'peer', 'BBBB', '2026-10-24T00:00:00+00:00', '2026-10-24T00:00:00+00:00')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let before_added: String = store
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT added_at FROM universe WHERE ticker = 'BBBB'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();

    // The scheduler's portfolio sync, as it runs every ten minutes.
    let held = vec![universe::HeldPosition {
        ticker: "AAAA".into(),
        avg_price: 1000.0,
    }];
    universe::sync(&store, &held, "2026-10-24T09:10:00+00:00").unwrap();
    store
        .with_conn(|c| {
            let row = |t: &str| -> (String, Option<String>, Option<String>, String) {
                c.query_row(
                    "SELECT role, business_note, pack_override, added_at FROM universe WHERE ticker = ?1",
                    [t],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap()
            };
            let bbbb = row("BBBB");
            assert_eq!(bbbb.0, "manual", "the sync does not demote manual back to watch");
            assert_eq!(bbbb.1.as_deref(), Some("Distributor sintetis"));
            assert_eq!(bbbb.2.as_deref(), Some("it_distribution"));
            assert_eq!(bbbb.3, before_added);
            assert_eq!(row("EEEE").0, "peer", "peers are not the v4 sync's to delete");
            assert_eq!(row("AAAA").0, "held");
            assert_eq!(count(c, "SELECT COUNT(*) FROM universe"), 4);
            Ok(())
        })
        .unwrap();
    // Removing the migrated manual add (the canon universe owns it now): the row goes and
    // the held and peer rows stay.
    universe::remove(&store, "CCCC", "2026-10-24T09:20:00+00:00").unwrap();
    universe::sync(&store, &held, "2026-10-24T09:20:00+00:00").unwrap();
    assert_eq!(
        store
            .universe()
            .unwrap()
            .iter()
            .filter(|e| e.ticker == "CCCC")
            .count(),
        0
    );
    assert_eq!(store.universe().unwrap().len(), 3);

    // R3.2: the composite key keeps one count per (ticker, as_of) as a point-in-time
    // series; the v4 reader still answers with the latest date. A PUT on the same date
    // replaces, a dated delete removes one row, the v4 delete clears the ticker.
    for (as_of, shares) in [
        ("2026-06-30", 10.0),
        ("2025-12-31", 9.0),
        ("2026-06-30", 11.0),
    ] {
        store
            .set_shares_manual(&investing_app::research::store::ManualShares {
                ticker: "AAAA".into(),
                shares,
                as_of: as_of.into(),
                source_note: "x".into(),
                entered_at: "2026-10-24T00:00:00+00:00".into(),
            })
            .unwrap();
    }
    let all = store.shares_manual_all("AAAA").unwrap();
    assert_eq!(all.len(), 2, "{all:?}");
    assert_eq!(
        store.shares_manual("AAAA").unwrap().unwrap().shares,
        11.0,
        "the latest as_of, as last written"
    );
    store
        .delete_shares_manual_at("AAAA", "2026-06-30", "2026-10-24T00:00:00+00:00")
        .unwrap();
    assert_eq!(store.shares_manual("AAAA").unwrap().unwrap().shares, 9.0);
    store
        .delete_shares_manual("AAAA", "2026-10-24T00:00:00+00:00")
        .unwrap();
    assert!(store.shares_manual_all("AAAA").unwrap().is_empty());
}

#[test]
fn a_count_mismatch_rolls_everything_back_and_keeps_the_backup() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let db = dir.path().join("research.sqlite");
    {
        // A manual add that the canon `universe` CHECK rejects: INSERT OR IGNORE would
        // drop it silently, so the count assertion must refuse the migration.
        let v4 = Store::open_at_version(&db, 2).unwrap();
        v4.with_conn(|c| {
            c.execute(
                "INSERT INTO universe_overrides (ticker, action, at) VALUES ('ab1', 'add', '2026-09-22T10:00:00+00:00')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    }
    let err = match Store::open(&db) {
        Ok(_) => panic!("the migration must refuse a lost manual addition"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("owner data counts differ"), "{err}");
    let c = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(count(&c, "PRAGMA user_version"), 2, "rolled back to v4");
    assert_eq!(count(&c, "SELECT COUNT(*) FROM notes"), 4);
    assert_eq!(
        count(
            &c,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'journal'"
        ),
        0
    );
    assert!(dir
        .path()
        .join(format!("research.sqlite.{}.bak", today_suffix()))
        .exists());
    assert!(
        dir.path().join(LOG_FILE).exists(),
        "the v4 log is untouched"
    );
    assert!(!dir
        .path()
        .join(format!("{LOG_FILE}.{}.bak", today_suffix()))
        .exists());
}

#[test]
fn migration_is_idempotent_and_a_pre_r5_log_replays_into_the_canon_tables() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let db = dir.path().join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        assert_eq!(journal(&store).len(), 5);
    }
    // Re-opening does not migrate again or duplicate anything; no further backup appears.
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    assert_eq!(journal(&store).len(), 5);
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
        .count();
    assert_eq!(
        backups, 2,
        "v4 database and v4 log; the upgrade runs the drop itself"
    );

    // Lines the pre-R5 app wrote: a note, a `seen` with the v4 columns, a manual add and a
    // removal override. They replay into the canon tables; the old fields are ignored.
    let log = dir.path().join(LOG_FILE);
    let mut text = std::fs::read_to_string(&log).unwrap();
    for line in [
        format!(r#"{{"op":"note_add","id":"n4","ticker":"BBBB","created_at":"2026-10-02T09:00:00+00:00","text":"{NOTE_TEXT}"}}"#),
        r#"{"op":"seen_set","ticker":"AAAA","seen_at":"2026-10-07T00:00:00+00:00","eligibility":"x","zone":"y"}"#.to_string(),
        r#"{"op":"note_add","id":"n9","ticker":"BBBB","created_at":"2026-10-03T09:00:00+00:00","text":"akan dihapus"}"#.to_string(),
        r#"{"op":"note_delete","id":"n9","at":"2026-10-04T00:00:00+00:00"}"#.to_string(),
        r#"{"op":"override","ticker":"FFFF","action":"add","at":"2026-10-08T00:00:00+00:00"}"#.to_string(),
        r#"{"op":"override","ticker":"CCCC","action":"remove","at":"2026-10-09T00:00:00+00:00"}"#.to_string(),
    ] {
        text.push_str(&line);
        text.push('\n');
    }
    std::fs::write(&log, text).unwrap();
    store.rebuild().unwrap();
    store.rebuild().unwrap();
    let j = journal(&store);
    assert_eq!(j.len(), 6);
    let n4 = j.iter().find(|e| e.0 == "v4note:n4").unwrap();
    assert_eq!(
        (n4.1.as_str(), n4.4.as_str(), n4.5),
        ("BBBB", NOTE_TEXT, Some(255.0))
    );
    assert!(
        !j.iter().any(|e| e.0 == "v4note:n9"),
        "a note deleted later in the log does not come back"
    );
    assert_eq!(j.iter().filter(|e| e.3 == "level").count(), 1);
    assert_eq!(
        store.seen("AAAA").unwrap().unwrap().seen_at,
        "2026-10-07T00:00:00+00:00"
    );
    let tickers: Vec<String> = store
        .universe()
        .unwrap()
        .into_iter()
        .map(|e| e.ticker)
        .collect();
    assert!(tickers.contains(&"FFFF".to_string()), "{tickers:?}");
    assert!(
        tickers.contains(&"CCCC".to_string()),
        "a v4 `remove` override replays as nothing: {tickers:?}"
    );
}

#[test]
fn the_drop_step_on_an_r1_store_keeps_the_owner_data_and_removes_the_v4_tables() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let db = dir.path().join("research.sqlite");
    // What R1 to R4 left on the owner's disk: migrated to v7, the v4 tables still there.
    {
        let store = Store::open_at_version(&db, 7).unwrap();
        assert_eq!(store.user_version().unwrap(), 7);
        store
            .with_conn(|c| {
                c.execute_batch(
                    r#"
                -- written through the v4 API after 004 first ran
                INSERT INTO notes (id, ticker, created_at, text)
                  VALUES ('n6', 'AAAA', '2026-10-07T13:00:00+00:00', 'Catatan setelah migrasi');
                INSERT INTO targets (ticker, entry_price, set_at) VALUES ('BBBB', 300, '2026-10-07T13:00:00+00:00');
                "#,
                )?;
                Ok(())
            })
            .unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());

    let tables = store.table_names().unwrap();
    for gone in [
        "snapshots",
        "fundamentals_q",
        "shares",
        "universe_overrides",
        "targets",
        "notes",
    ] {
        assert!(
            !tables.iter().any(|t| t == gone),
            "{gone} still exists: {tables:?}"
        );
    }
    for kept in [
        "filings",
        "facts",
        "derived",
        "journal",
        "shares_manual",
        "universe",
        "seen",
        "prices_daily",
        "held_positions",
    ] {
        assert!(
            tables.iter().any(|t| t == kept),
            "{kept} missing: {tables:?}"
        );
    }
    store
        .with_conn(|c| {
            let cols = |table: &str| -> Vec<String> {
                let mut st = c.prepare(&format!("PRAGMA table_info({table})")).unwrap();
                st.query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap()
            };
            assert_eq!(cols("seen"), ["ticker", "seen_at"]);
            assert!(!cols("prices_daily").contains(&"adj_close".to_string()));
            assert!(!cols("universe").contains(&"origins".to_string()));
            // Counts of the owner tables: 4 notes + the later one, 1 + the later target.
            assert_eq!(
                count(c, "SELECT COUNT(*) FROM journal WHERE kind = 'catatan'"),
                5
            );
            assert_eq!(
                count(c, "SELECT COUNT(*) FROM journal WHERE kind = 'level'"),
                2
            );
            assert_eq!(count(c, "SELECT COUNT(*) FROM shares_manual"), 4);
            assert_eq!(
                count(c, "SELECT COUNT(*) FROM universe WHERE role = 'manual'"),
                2
            );
            assert_eq!(count(c, "SELECT COUNT(*) FROM seen"), 1);
            assert_eq!(count(c, "SELECT COUNT(*) FROM held_positions"), 1);
            Ok(())
        })
        .unwrap();
    assert!(dir
        .path()
        .join(format!(
            "research.sqlite.pre-005-{}.bak",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .exists());
    drop(store);

    // Idempotent: a second open changes nothing and makes no second backup.
    let again = Store::open(&db).unwrap();
    assert_eq!(again.user_version().unwrap(), latest_version());
    assert_eq!(journal(&again).len(), 7);
    let pre = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("pre-005"))
        .count();
    assert_eq!(pre, 1);
}

#[test]
fn a_deleted_v4_note_does_not_block_the_drop_and_stays_in_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    build_v4(dir.path());
    let db = dir.path().join("research.sqlite");
    {
        let store = Store::open_at_version(&db, 7).unwrap();
        store
            .with_conn(|c| {
                // 004 carried n3 into the journal; the owner then deleted it in v4.
                c.execute_batch(
                    "INSERT OR IGNORE INTO journal (id, ticker, at, kind, text) SELECT 'v4note:' || id, ticker, created_at, 'catatan', text FROM notes;
                     DELETE FROM notes WHERE id = 'n3';",
                )?;
                Ok(())
            })
            .unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    assert!(journal(&store).iter().any(|e| e.0 == "v4note:n3"));
}

#[test]
fn a_fresh_store_starts_at_the_latest_version_with_an_empty_journal() {
    let store = Store::open_memory().unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    assert!(journal(&store).is_empty());
    assert!(store.filings(None).unwrap().is_empty());
    // Manual shares keep one row per (ticker, as_of); the v4 reader sees the latest.
    store
        .with_conn(|c| {
            c.execute(
                "INSERT INTO shares_manual (ticker, as_of, shares, source_note, entered_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params!["AAAA", "2025-12-31", 100.0, "a", "2026-01-01T00:00:00+00:00"],
            )?;
            c.execute(
                "INSERT INTO shares_manual (ticker, as_of, shares, source_note, entered_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params!["AAAA", "2026-06-30", 120.0, "b", "2026-07-01T00:00:00+00:00"],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(store.shares_manual("AAAA").unwrap().unwrap().shares, 120.0);
    assert_eq!(store.shares_manual_all("AAAA").unwrap().len(), 2);
}
