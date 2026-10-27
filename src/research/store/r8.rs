//! Repository for the R8 `extraction_proposals` table (research plan §4.1, R8). A proposal
//! is a suggested KPI value with its page and snippet; it moves `pending` to `accepted` or
//! `rejected` once, and only the API's accept handler writes the KPI. Adds and decisions go
//! through the JSONL log like every other owner table, and `apply` is shared with `rebuild`.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::{LogEntry, Result, Store};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposalRow {
    pub id: String,
    pub document_id: String,
    pub ticker: String,
    /// `<metric>:<FY>Q<q>`.
    pub kpi_key: String,
    pub value: f64,
    pub unit: String,
    pub page: u32,
    pub snippet: String,
    pub rank: u32,
    /// `pending`, `accepted`, `rejected` or `superseded`.
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

const COLUMNS: &str =
    "id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at, decided_at";

fn proposal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProposalRow> {
    Ok(ProposalRow {
        id: r.get(0)?,
        document_id: r.get(1)?,
        ticker: r.get(2)?,
        kpi_key: r.get(3)?,
        value: r.get(4)?,
        unit: r.get(5)?,
        page: r.get(6)?,
        snippet: r.get(7)?,
        rank: r.get(8)?,
        status: r.get(9)?,
        created_at: r.get(10)?,
        decided_at: r.get(11)?,
    })
}

impl Store {
    /// Stores the proposals of one extraction; an id that exists is left as it is, so a
    /// retry cannot reopen a decided proposal. Always `pending` on the way in.
    pub fn add_proposals(&self, rows: &[ProposalRow]) -> Result<()> {
        for p in rows {
            let mut row = p.clone();
            row.status = "pending".into();
            row.decided_at = None;
            let entry = LogEntry::ProposalAdd { proposal: row };
            self.write_stored(&entry, || Ok(self.proposal(&p.id)?.is_some()))?;
        }
        Ok(())
    }

    pub fn proposal(&self, id: &str) -> Result<Option<ProposalRow>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM extraction_proposals WHERE id = ?1"),
                [id],
                proposal_row,
            )
            .optional()?)
    }

    /// Proposals of `ticker`, grouped by document and KPI cell, best candidate first.
    pub fn proposals(
        &self,
        ticker: &str,
        document_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<ProposalRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM extraction_proposals
             WHERE ticker = ?1 AND (?2 IS NULL OR document_id = ?2) AND (?3 IS NULL OR status = ?3)
             ORDER BY document_id, kpi_key, rank, id"
        ))?;
        let rows = st.query_map(params![ticker, document_id, status], proposal_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The other candidates of the cell `p` was just accepted into, marked `superseded`:
    /// its pending siblings of the same document, and (when this accept replaced a value)
    /// proposals accepted earlier. Returns their ids. Proposals of another document that are
    /// still pending stay pending: the owner may yet compare them with `replace`.
    pub fn supersede_around(
        &self,
        p: &ProposalRow,
        replaced: bool,
        at: &str,
    ) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for o in self.proposals(&p.ticker, None, None)? {
            if o.id == p.id || o.kpi_key != p.kpi_key {
                continue;
            }
            let hit = (o.status == "pending" && o.document_id == p.document_id)
                || (o.status == "accepted" && replaced);
            if hit && self.decide_proposal(&o.id, "superseded", at)? {
                out.push(o.id);
            }
        }
        Ok(out)
    }

    /// `pending` to `accepted`, `rejected` or `superseded`, once (an accepted proposal can
    /// still become `superseded`). `true` when this call decided it.
    pub fn decide_proposal(&self, id: &str, status: &str, at: &str) -> Result<bool> {
        self.write(&LogEntry::ProposalDecide {
            id: id.into(),
            status: status.into(),
            decided_at: at.into(),
        })
    }
}

pub(super) fn apply(conn: &Connection, entry: &LogEntry) -> rusqlite::Result<usize> {
    match entry {
        LogEntry::ProposalAdd { proposal: p } => conn.execute(
            "INSERT OR IGNORE INTO extraction_proposals
               (id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                p.id, p.document_id, p.ticker, p.kpi_key, p.value, p.unit, p.page, p.snippet,
                p.rank, p.status, p.created_at, p.decided_at
            ],
        ),
        LogEntry::ProposalDecide {
            id,
            status,
            decided_at,
        } => conn.execute(
            "UPDATE extraction_proposals SET status = ?2, decided_at = ?3
             WHERE id = ?1 AND (status = 'pending' OR (status = 'accepted' AND ?2 = 'superseded'))",
            params![id, status, decided_at],
        ),
        _ => Ok(0),
    }
}
