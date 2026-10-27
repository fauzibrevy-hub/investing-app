//! Repositories for the R4 owner tables (research plan §4.1): `claims`, `claim_evals`,
//! `journal`, `journal_annotations`, `documents`, `evidence`, `events`, `calendar_manual`.
//! Every write goes through the JSONL log (D16); `apply` is shared with `rebuild`.
//! There is deliberately no update or delete for `journal` rows (D9): the only ways a
//! journal row changes are `rebuild`, which replays the log, and the v4 migration.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{LogEntry, Result, Store};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimRow {
    pub id: String,
    pub ticker: String,
    pub statement: String,
    pub metric: String,
    pub comparator: String,
    pub threshold: f64,
    pub threshold_hi: Option<f64>,
    pub window: String,
    pub due: String,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimEvalRow {
    pub claim_id: String,
    pub evaluated_at: String,
    pub status: String,
    pub value: Option<f64>,
    /// JSON text of the observations the evaluation used.
    pub inputs: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalRow {
    pub id: String,
    pub ticker: String,
    pub at: String,
    pub kind: String,
    pub text: String,
    pub price_at: Option<f64>,
    pub level: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnotationRow {
    pub id: String,
    pub journal_id: String,
    pub at: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentRow {
    pub id: String,
    pub ticker: String,
    pub kind: String,
    pub url: Option<String>,
    pub text: Option<String>,
    pub doc_date: Option<String>,
    pub added_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRow {
    pub claim_id: Option<String>,
    pub kpi_key: Option<String>,
    pub document_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventRow {
    pub id: String,
    pub at: String,
    pub ticker: Option<String>,
    pub kind: String,
    /// JSON text.
    pub payload: String,
    pub dismissed_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarRow {
    pub ticker: String,
    pub date: String,
    pub kind: String,
    pub note: Option<String>,
}

const CLAIM_COLUMNS: &str = "id, ticker, statement, metric, comparator, threshold, threshold_hi, \"window\", due, status, created_at, closed_at";
const EVAL_COLUMNS: &str = "claim_id, evaluated_at, status, value, inputs";
const JOURNAL_COLUMNS: &str = "id, ticker, at, kind, text, price_at, level";
const DOCUMENT_COLUMNS: &str = "id, ticker, kind, url, text, doc_date, added_at";
const EVENT_COLUMNS: &str = "id, at, ticker, kind, payload, dismissed_at";

fn claim_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ClaimRow> {
    Ok(ClaimRow {
        id: r.get(0)?,
        ticker: r.get(1)?,
        statement: r.get(2)?,
        metric: r.get(3)?,
        comparator: r.get(4)?,
        threshold: r.get(5)?,
        threshold_hi: r.get(6)?,
        window: r.get(7)?,
        due: r.get(8)?,
        status: r.get(9)?,
        created_at: r.get(10)?,
        closed_at: r.get(11)?,
    })
}

fn eval_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ClaimEvalRow> {
    Ok(ClaimEvalRow {
        claim_id: r.get(0)?,
        evaluated_at: r.get(1)?,
        status: r.get(2)?,
        value: r.get(3)?,
        inputs: r.get(4)?,
    })
}

fn journal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JournalRow> {
    Ok(JournalRow {
        id: r.get(0)?,
        ticker: r.get(1)?,
        at: r.get(2)?,
        kind: r.get(3)?,
        text: r.get(4)?,
        price_at: r.get(5)?,
        level: r.get(6)?,
    })
}

fn document_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentRow> {
    Ok(DocumentRow {
        id: r.get(0)?,
        ticker: r.get(1)?,
        kind: r.get(2)?,
        url: r.get(3)?,
        text: r.get(4)?,
        doc_date: r.get(5)?,
        added_at: r.get(6)?,
    })
}

fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        at: r.get(1)?,
        ticker: r.get(2)?,
        kind: r.get(3)?,
        payload: r.get(4)?,
        dismissed_at: r.get(5)?,
    })
}

impl Store {
    /// Applies one R4 entry and logs it only when it changed a row: an `OR IGNORE` that
    /// swallowed a duplicate or a CHECK violation never reaches the log. `true` when a row
    /// changed.
    pub(super) fn write(&self, entry: &LogEntry) -> Result<bool> {
        let changed = {
            let conn = self.conn();
            apply(&conn, entry)? > 0
        };
        if changed {
            self.bump();
            self.append_log(entry)?;
        }
        Ok(changed)
    }

    /// For entries that must end up stored: a row that was not written and is not already
    /// there was refused by the schema (a CHECK), which is an error, not a no-op.
    pub(super) fn write_stored(
        &self,
        entry: &LogEntry,
        exists: impl FnOnce() -> Result<bool>,
    ) -> Result<()> {
        if !self.write(entry)? && !exists()? {
            return Err(super::StoreError::Migration(
                "row refused by the schema".into(),
            ));
        }
        Ok(())
    }

    // ---- claims ----

    /// A claim id that exists is left as it is (ids come from idempotency keys, so a retry
    /// must not reopen a closed claim or reset its status).
    pub fn insert_claim(&self, c: &ClaimRow) -> Result<()> {
        self.write_stored(&LogEntry::ClaimCreate { claim: c.clone() }, || {
            Ok(self.claim(&c.id)?.is_some())
        })
    }

    /// Stores an evaluation and moves the claim's current status to it. `(claim, time)` is
    /// the key, so an evaluation landing on the instant of an earlier one is moved on by a
    /// millisecond instead of replacing it: the history only ever grows. The check, the
    /// insert and the status update share one lock and one transaction.
    pub fn record_eval(&self, e: &ClaimEvalRow) -> Result<()> {
        let mut eval = e.clone();
        let entry = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            loop {
                let taken: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM claim_evals WHERE claim_id = ?1 AND evaluated_at = ?2)",
                    params![eval.claim_id, eval.evaluated_at],
                    |r| r.get(0),
                )?;
                let at = chrono::DateTime::parse_from_rfc3339(&eval.evaluated_at);
                match (taken, at) {
                    (true, Ok(at)) => {
                        eval.evaluated_at = (at + chrono::Duration::milliseconds(1)).to_rfc3339();
                    }
                    _ => break,
                }
            }
            let entry = LogEntry::ClaimEval { eval };
            apply(&tx, &entry)?;
            tx.commit()?;
            entry
        };
        self.bump();
        self.append_log(&entry)
    }

    /// Closing keeps the last status and every evaluation; a closed claim is not re-evaluated.
    pub fn close_claim(&self, id: &str, at: &str) -> Result<()> {
        self.write(&LogEntry::ClaimClose {
            id: id.into(),
            closed_at: at.into(),
        })
        .map(|_| ())
    }

    pub fn claim(&self, id: &str) -> Result<Option<ClaimRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CLAIM_COLUMNS} FROM claims WHERE id = ?1"),
                [id],
                claim_row,
            )
            .optional()?)
    }

    /// Claims of one ticker, or of every ticker, oldest first.
    pub fn claims(&self, ticker: Option<&str>) -> Result<Vec<ClaimRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {CLAIM_COLUMNS} FROM claims {} ORDER BY created_at, id",
            if ticker.is_some() {
                "WHERE ticker = ?1"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let rows = match ticker {
            Some(t) => st.query_map([t], claim_row)?,
            None => st.query_map([], claim_row)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Evaluations of a claim, oldest first.
    pub fn claim_evals(&self, claim_id: &str) -> Result<Vec<ClaimEvalRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {EVAL_COLUMNS} FROM claim_evals WHERE claim_id = ?1 ORDER BY evaluated_at"
        ))?;
        let rows = st.query_map([claim_id], eval_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn last_eval(&self, claim_id: &str) -> Result<Option<ClaimEvalRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {EVAL_COLUMNS} FROM claim_evals WHERE claim_id = ?1
                     ORDER BY evaluated_at DESC LIMIT 1"
                ),
                [claim_id],
                eval_row,
            )
            .optional()?)
    }

    // ---- journal (append-only) ----

    pub fn add_journal(&self, j: &JournalRow) -> Result<()> {
        self.write_stored(&LogEntry::JournalAdd { entry: j.clone() }, || {
            Ok(self.journal_entry(&j.id)?.is_some())
        })
    }

    pub fn journal_entry(&self, id: &str) -> Result<Option<JournalRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {JOURNAL_COLUMNS} FROM journal WHERE id = ?1"),
                [id],
                journal_row,
            )
            .optional()?)
    }

    /// Entries of one ticker or of all, newest first.
    pub fn journal(&self, ticker: Option<&str>) -> Result<Vec<JournalRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {JOURNAL_COLUMNS} FROM journal {} ORDER BY at DESC, id DESC",
            if ticker.is_some() {
                "WHERE ticker = ?1"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let rows = match ticker {
            Some(t) => st.query_map([t], journal_row)?,
            None => st.query_map([], journal_row)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The `level` entries of a ticker, oldest first (the chart and the crossing producer).
    pub fn journal_levels(&self, ticker: &str) -> Result<Vec<JournalRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {JOURNAL_COLUMNS} FROM journal WHERE ticker = ?1 AND kind = 'level' AND level IS NOT NULL
             ORDER BY at, id"
        ))?;
        let rows = st.query_map([ticker], journal_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_annotation(&self, a: &AnnotationRow) -> Result<()> {
        let entry = LogEntry::JournalAnnotate {
            annotation: a.clone(),
        };
        self.write_stored(&entry, || {
            Ok(self.annotations(None)?.iter().any(|x| x.id == a.id))
        })
    }

    /// Annotations of every entry of `ticker` (or of all), oldest first.
    pub fn annotations(&self, ticker: Option<&str>) -> Result<Vec<AnnotationRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT a.id, a.journal_id, a.at, a.text FROM journal_annotations a
             JOIN journal j ON j.id = a.journal_id {} ORDER BY a.at, a.id",
            if ticker.is_some() {
                "WHERE j.ticker = ?1"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row<'_>| {
            Ok(AnnotationRow {
                id: r.get(0)?,
                journal_id: r.get(1)?,
                at: r.get(2)?,
                text: r.get(3)?,
            })
        };
        let rows = match ticker {
            Some(t) => st.query_map([t], map)?,
            None => st.query_map([], map)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- documents and evidence ----

    pub fn add_document(&self, d: &DocumentRow) -> Result<()> {
        let entry = LogEntry::DocumentAdd {
            document: d.clone(),
        };
        self.write_stored(&entry, || Ok(self.document(&d.id)?.is_some()))
    }

    pub fn document(&self, id: &str) -> Result<Option<DocumentRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE id = ?1"),
                [id],
                document_row,
            )
            .optional()?)
    }

    /// Newest first.
    pub fn documents(&self, ticker: &str) -> Result<Vec<DocumentRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE ticker = ?1
             ORDER BY COALESCE(doc_date, substr(added_at, 1, 10)) DESC, added_at DESC, id"
        ))?;
        let rows = st.query_map([ticker], document_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn link_evidence(&self, e: &EvidenceRow) -> Result<()> {
        let entry = LogEntry::EvidenceLink {
            evidence: e.clone(),
        };
        self.write_stored(&entry, || {
            let ticker = self.document(&e.document_id)?.map(|d| d.ticker);
            Ok(match ticker {
                Some(t) => self.evidence(&t)?.contains(e),
                None => false,
            })
        })
    }

    /// Links of every document of `ticker`.
    pub fn evidence(&self, ticker: &str) -> Result<Vec<EvidenceRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT e.claim_id, e.kpi_key, e.document_id FROM evidence e
             JOIN documents d ON d.id = e.document_id WHERE d.ticker = ?1
             ORDER BY e.document_id, COALESCE(e.claim_id, ''), COALESCE(e.kpi_key, '')",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok(EvidenceRow {
                claim_id: r.get(0)?,
                kpi_key: r.get(1)?,
                document_id: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Evidence links per claim id.
    pub fn evidence_counts(&self, ticker: &str) -> Result<HashMap<String, usize>> {
        let mut out = HashMap::new();
        for e in self.evidence(ticker)? {
            if let Some(c) = e.claim_id {
                *out.entry(c).or_insert(0) += 1;
            }
        }
        Ok(out)
    }

    // ---- events (inbox) ----

    /// Inserts the event unless its id exists (producers are idempotent by id); `true` when
    /// it was new. A dismissed event stays dismissed.
    pub fn add_event(&self, e: &EventRow) -> Result<bool> {
        if self.event(&e.id)?.is_some() {
            return Ok(false);
        }
        // The log line goes first: if the append fails nothing was stored and the next pass
        // tries again; if the insert fails afterwards the replay skips the line.
        let entry = LogEntry::EventAdd { event: e.clone() };
        self.append_log(&entry)?;
        let inserted = {
            let conn = self.conn();
            apply(&conn, &entry)? > 0
        };
        if inserted {
            self.bump();
        }
        Ok(inserted)
    }

    /// `false` when the event does not exist or is already dismissed.
    pub fn dismiss_event(&self, id: &str, at: &str) -> Result<bool> {
        let open = self.event(id)?.is_some_and(|e| e.dismissed_at.is_none());
        if open {
            self.write(&LogEntry::EventDismiss {
                id: id.into(),
                at: at.into(),
            })?;
        }
        Ok(open)
    }

    /// Replaces the payload of an event that is still open; `false` when nothing changed.
    pub fn update_event_payload(&self, id: &str, payload: &str) -> Result<bool> {
        self.write(&LogEntry::EventUpdate {
            id: id.into(),
            payload: payload.into(),
        })
    }

    pub fn event(&self, id: &str) -> Result<Option<EventRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = ?1"),
                [id],
                event_row,
            )
            .optional()?)
    }

    /// Every event, newest first; `open_only` hides the dismissed ones.
    pub fn events_list(&self, open_only: bool) -> Result<Vec<EventRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {EVENT_COLUMNS} FROM events {} ORDER BY at DESC, id DESC",
            if open_only {
                "WHERE dismissed_at IS NULL"
            } else {
                ""
            }
        ))?;
        let rows = st.query_map([], event_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Restatements detected on or after `since` (a date or timestamp), for the inbox.
    pub fn restatements_since(&self, since: &str) -> Result<Vec<super::RestatementRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, element, start, end_, dims, original, original_filing, restated, restated_filing, detected_at
             FROM restatements WHERE detected_at >= ?1 ORDER BY detected_at, ticker, element",
        )?;
        let rows = st.query_map([since], |r| {
            Ok(super::RestatementRow {
                ticker: r.get(0)?,
                element: r.get(1)?,
                start: r.get(2)?,
                end: r.get(3)?,
                dims: r.get(4)?,
                original: r.get(5)?,
                original_filing: r.get(6)?,
                restated: r.get(7)?,
                restated_filing: r.get(8)?,
                detected_at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- calendar_manual ----

    pub fn set_calendar(&self, c: &CalendarRow) -> Result<()> {
        self.write(&LogEntry::CalendarSet { entry: c.clone() })
            .map(|_| ())
    }

    pub fn delete_calendar(&self, ticker: &str, date: &str, kind: &str, at: &str) -> Result<()> {
        self.write(&LogEntry::CalendarDelete {
            ticker: ticker.into(),
            date: date.into(),
            kind: kind.into(),
            at: at.into(),
        })
        .map(|_| ())
    }

    pub fn calendar_manual(&self, ticker: Option<&str>) -> Result<Vec<CalendarRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT ticker, date, kind, note FROM calendar_manual {} ORDER BY date, ticker, kind",
            if ticker.is_some() {
                "WHERE ticker = ?1"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row<'_>| {
            Ok(CalendarRow {
                ticker: r.get(0)?,
                date: r.get(1)?,
                kind: r.get(2)?,
                note: r.get(3)?,
            })
        };
        let rows = match ticker {
            Some(t) => st.query_map([t], map)?,
            None => st.query_map([], map)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Every ticker's last "seen" time (`TERAKHIR DITINJAU`).
    pub fn seen_all(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn();
        let mut st = conn.prepare("SELECT ticker, seen_at FROM seen")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// Applies one R4 log entry and returns the rows changed (`add_event` reads it to tell a
/// new event from a repeat). Shared by live writes and `rebuild`.
pub(super) fn apply(conn: &Connection, entry: &LogEntry) -> rusqlite::Result<usize> {
    match entry {
        LogEntry::ClaimCreate { claim: c } => conn.execute(
            "INSERT OR IGNORE INTO claims (id, ticker, statement, metric, comparator, threshold, threshold_hi, \"window\", due, status, created_at, closed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                c.id, c.ticker, c.statement, c.metric, c.comparator, c.threshold, c.threshold_hi,
                c.window, c.due, c.status, c.created_at, c.closed_at
            ],
        ),
        LogEntry::ClaimEval { eval: e } => {
            conn.execute(
                "INSERT OR REPLACE INTO claim_evals (claim_id, evaluated_at, status, value, inputs)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.claim_id, e.evaluated_at, e.status, e.value, e.inputs],
            )?;
            conn.execute(
                "UPDATE claims SET status = ?2 WHERE id = ?1 AND closed_at IS NULL",
                params![e.claim_id, e.status],
            )
        }
        LogEntry::ClaimClose { id, closed_at } => conn.execute(
            "UPDATE claims SET closed_at = ?2 WHERE id = ?1 AND closed_at IS NULL",
            params![id, closed_at],
        ),
        LogEntry::JournalAdd { entry: j } => conn.execute(
            // Never REPLACE: an entry that exists stays as it was written.
            "INSERT OR IGNORE INTO journal (id, ticker, at, kind, text, price_at, level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![j.id, j.ticker, j.at, j.kind, j.text, j.price_at, j.level],
        ),
        LogEntry::JournalAnnotate { annotation: a } => conn.execute(
            "INSERT OR IGNORE INTO journal_annotations (id, journal_id, at, text)
             VALUES (?1, ?2, ?3, ?4)",
            params![a.id, a.journal_id, a.at, a.text],
        ),
        LogEntry::DocumentAdd { document: d } => conn.execute(
            "INSERT OR IGNORE INTO documents (id, ticker, kind, url, text, doc_date, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![d.id, d.ticker, d.kind, d.url, d.text, d.doc_date, d.added_at],
        ),
        LogEntry::EvidenceLink { evidence: e } => conn.execute(
            "INSERT OR IGNORE INTO evidence (claim_id, kpi_key, document_id) VALUES (?1, ?2, ?3)",
            params![e.claim_id, e.kpi_key, e.document_id],
        ),
        LogEntry::EventAdd { event: e } => conn.execute(
            "INSERT OR IGNORE INTO events (id, at, ticker, kind, payload, dismissed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![e.id, e.at, e.ticker, e.kind, e.payload, e.dismissed_at],
        ),
        LogEntry::EventDismiss { id, at } => conn.execute(
            "UPDATE events SET dismissed_at = ?2 WHERE id = ?1 AND dismissed_at IS NULL",
            params![id, at],
        ),
        LogEntry::EventUpdate { id, payload } => conn.execute(
            "UPDATE events SET payload = ?2 WHERE id = ?1 AND dismissed_at IS NULL AND payload <> ?2",
            params![id, payload],
        ),
        LogEntry::CalendarSet { entry: c } => conn.execute(
            "INSERT OR REPLACE INTO calendar_manual (ticker, date, kind, note) VALUES (?1, ?2, ?3, ?4)",
            params![c.ticker, c.date, c.kind, c.note],
        ),
        LogEntry::CalendarDelete {
            ticker, date, kind, ..
        } => conn.execute(
            "DELETE FROM calendar_manual WHERE ticker = ?1 AND date = ?2 AND kind = ?3",
            params![ticker, date, kind],
        ),
        LogEntry::SeenSet { ticker, seen_at } => conn.execute(
            "INSERT OR REPLACE INTO seen (ticker, seen_at) VALUES (?1, ?2)",
            params![ticker, seen_at],
        ),
        LogEntry::ProposalAdd { .. } | LogEntry::ProposalDecide { .. } => {
            super::r8::apply(conn, entry)
        }
        LogEntry::StructureSet { .. } => super::structure::apply(conn, entry),
        // The legacy variants have their own arms in `store::apply`; nothing else routes here.
        LogEntry::Override { .. }
        | LogEntry::SharesManualSet { .. }
        | LogEntry::SharesManualDelete { .. }
        | LogEntry::TargetSet { .. }
        | LogEntry::TargetDelete { .. }
        | LogEntry::NoteAdd { .. }
        | LogEntry::NoteDelete { .. }
        | LogEntry::UniverseUpsert { .. }
        | LogEntry::UniverseDelete { .. }
        | LogEntry::KpiSet { .. }
        | LogEntry::KpiDelete { .. }
        | LogEntry::ScenarioSet { .. }
        | LogEntry::ScenarioDelete { .. } => Ok(0),
    }
}
