//! Store guarantees of the R4 tables (research plan §4.1, D9, D16): ids that exist are never
//! overwritten, a closed claim stays closed, an evaluation never replaces another, a row
//! the schema refuses is an error and never reaches the log, and a store rebuilt from the
//! log next to nothing keeps annotations on the entries migrated from v4 notes.

use finance_app::research::store::{
    AnnotationRow, ClaimEvalRow, ClaimRow, JournalRow, Store, LOG_FILE,
};

fn claim(id: &str) -> ClaimRow {
    ClaimRow {
        id: id.into(),
        ticker: "COAL".into(),
        statement: "Harga di atas seribu".into(),
        metric: "price.close".into(),
        comparator: ">=".into(),
        threshold: 1_000.0,
        threshold_hi: None,
        window: "latest".into(),
        due: "2026-12-31".into(),
        status: "not_testable".into(),
        created_at: "2026-10-24T09:00:00+00:00".into(),
        closed_at: None,
    }
}

fn eval(id: &str, at: &str, status: &str, value: f64) -> ClaimEvalRow {
    ClaimEvalRow {
        claim_id: id.into(),
        evaluated_at: at.into(),
        status: status.into(),
        value: Some(value),
        inputs: "{}".into(),
    }
}

fn log_lines(dir: &std::path::Path) -> usize {
    std::fs::read_to_string(dir.join(LOG_FILE))
        .unwrap_or_default()
        .lines()
        .count()
}

#[test]
fn a_claim_that_exists_is_not_overwritten_and_a_closed_one_stays_closed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    store.insert_claim(&claim("c-1")).unwrap();
    store
        .record_eval(&eval(
            "c-1",
            "2026-10-24T09:00:00+00:00",
            "holding",
            1_100.0,
        ))
        .unwrap();
    store
        .close_claim("c-1", "2026-10-25T09:00:00+00:00")
        .unwrap();
    let closed = store.claim("c-1").unwrap().unwrap();
    assert_eq!(closed.status, "holding");

    // The same id inserted again (a retry) leaves the closed claim as it was.
    store.insert_claim(&claim("c-1")).unwrap();
    assert_eq!(store.claim("c-1").unwrap().unwrap(), closed);
    // An evaluation already under way when the claim closed cannot move its status.
    store
        .record_eval(&eval("c-1", "2026-10-26T09:00:00+00:00", "broken", 900.0))
        .unwrap();
    assert_eq!(store.claim("c-1").unwrap().unwrap().status, "holding");
    // Closing twice changes nothing and logs nothing new.
    let lines = log_lines(dir.path());
    store
        .close_claim("c-1", "2026-10-27T09:00:00+00:00")
        .unwrap();
    assert_eq!(log_lines(dir.path()), lines);
    assert_eq!(
        store.claim("c-1").unwrap().unwrap().closed_at.as_deref(),
        Some("2026-10-25T09:00:00+00:00")
    );
}

#[test]
fn evaluations_on_the_same_instant_are_both_kept_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    store.insert_claim(&claim("c-1")).unwrap();
    let at = "2026-10-24T09:00:00+00:00";
    for (status, value) in [("not_testable", 0.0), ("holding", 1.0), ("broken", 2.0)] {
        store.record_eval(&eval("c-1", at, status, value)).unwrap();
    }
    let rows = store.claim_evals("c-1").unwrap();
    let got: Vec<(&str, f64)> = rows
        .iter()
        .map(|r| (r.status.as_str(), r.value.unwrap()))
        .collect();
    assert_eq!(
        got,
        [("not_testable", 0.0), ("holding", 1.0), ("broken", 2.0)]
    );
    assert_eq!(store.claim("c-1").unwrap().unwrap().status, "broken");
    // The log carries the moved timestamps, so a rebuild gives the same history.
    store.rebuild().unwrap();
    assert_eq!(store.claim_evals("c-1").unwrap(), rows);
}

#[test]
fn a_row_the_schema_refuses_is_an_error_and_never_logged() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let bad = JournalRow {
        id: "j-bad".into(),
        ticker: "COAL".into(),
        at: "2026-10-24T09:00:00+00:00".into(),
        kind: "catatan".into(),
        text: String::new(), // CHECK (length(text) BETWEEN 1 AND 2000)
        price_at: None,
        level: None,
    };
    assert!(store.add_journal(&bad).is_err());
    assert!(store.journal_entry("j-bad").unwrap().is_none());
    let log = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap_or_default();
    assert!(!log.contains("j-bad"), "a refused row must not be logged");
    // A good row twice is one row and one log line.
    let ok = JournalRow {
        text: "ok".into(),
        id: "j-ok".into(),
        ..bad
    };
    store.add_journal(&ok).unwrap();
    store.add_journal(&ok).unwrap();
    assert_eq!(store.journal(Some("COAL")).unwrap().len(), 1);
    let log = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
    assert_eq!(log.matches("j-ok").count(), 1);
}

#[test]
fn an_annotation_on_a_migrated_v4_note_survives_a_rebuild_on_a_fresh_store() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    let annotation;
    {
        // A pre-R5 log line: the v4 note replays straight into the journal.
        std::fs::write(
            dir.path().join(LOG_FILE),
            "{\"op\":\"note_add\",\"id\":\"n-1\",\"ticker\":\"COAL\",\"created_at\":\"2026-09-01T01:00:00+00:00\",\"text\":\"catatan lama\"}\n",
        )
        .unwrap();
        let store = Store::open(&db).unwrap();
        store.rebuild().unwrap();
        let entry = store.journal_entry("v4note:n-1").unwrap().unwrap();
        annotation = AnnotationRow {
            id: "a-1".into(),
            journal_id: entry.id,
            at: "2026-10-24T09:00:00+00:00".into(),
            text: "ditinjau lagi".into(),
        };
        store.add_annotation(&annotation).unwrap();
    }
    // The database file is lost; the log remains. Opening rebuilds from it.
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
    }
    let store = Store::open(&db).unwrap();
    assert!(store.journal_entry("v4note:n-1").unwrap().is_some());
    assert_eq!(store.annotations(Some("COAL")).unwrap(), [annotation]);
}
