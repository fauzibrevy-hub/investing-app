//! App-owned research store: `INVESTING_DATA_DIR/research.sqlite` (research plan §4.1, D16).
//!
//! Schema migrations run through `PRAGMA user_version`; SQL lives in `migrations/NNN_*.sql`
//! and is embedded at build time. User-authored data (universe rows, manual shares, KPIs,
//! scenarios, claims, journal, calendar) is also appended to `research-log.jsonl` next to
//! the database, and `rebuild` replays it. Everything else is derived and can be refetched.
//!
//! A v4 database (schema v1 or v2) is upgraded in place on open (plan §4.5): the file is
//! backed up first, migrations 003 to 008 run in one transaction, owner-data counts are
//! checked before and after, and the old JSONL log is renamed so a fresh one starts. The
//! step that drops the v4 tables (schema v8, `005_drop_v4.sql`) first re-runs the
//! owner-data migration and the one-time shares carry, and takes its own backup.
//!
//! The connection is synchronous behind a mutex; async callers wrap calls in `spawn_blocking`.

mod facts;
mod r4;
mod r8;
mod structure;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub(crate) use facts::RESTATED_YEARS_SQL;
pub use facts::{DerivedRow, FactRow, FilingRow, IngestErrorRow, RestatementRow};
pub use r4::{
    AnnotationRow, CalendarRow, ClaimEvalRow, ClaimRow, DocumentRow, EventRow, EvidenceRow,
    JournalRow,
};
pub use r8::ProposalRow;
pub use structure::{NodeSource, StructureMap, StructureNode};

use super::kpi::KpiValue;
use super::universe::{Role, UniverseEntry};

pub const LOG_FILE: &str = "research-log.jsonl";

/// `(user_version, sql)` in order; a migration never changes once released. File numbers
/// are not versions: `005_drop_v4.sql` was held back until R5, so `006_indexes.sql` is v5,
/// `007_prices_drop_adj_close.sql` (R3.1) is v6, `008_r4_indexes.sql` (R4) is v7 and
/// `005_drop_v4.sql` (R5-E1) is v8. The v8 step runs through `drop_v4_step`.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("migrations/001_init.sql")),
    (2, include_str!("migrations/002_held_positions.sql")),
    (3, include_str!("migrations/003_facts.sql")),
    (4, include_str!("migrations/004_migrate_owner_data.sql")),
    (5, include_str!("migrations/006_indexes.sql")),
    (6, include_str!("migrations/007_prices_drop_adj_close.sql")),
    (7, include_str!("migrations/008_r4_indexes.sql")),
    (DROP_V4_VERSION, DROP_V4_SQL),
    (9, include_str!("migrations/009_extraction_proposals.sql")),
    (10, include_str!("migrations/010_proposals_superseded.sql")),
    (11, include_str!("migrations/011_structure_maps.sql")),
    (12, include_str!("migrations/012_pdf_filings.sql")),
];

/// Schema version that drops the v4 tables.
const DROP_V4_VERSION: i64 = 8;

/// Owner-data migration, idempotent: the drop step re-runs it to pick up notes and targets
/// written since it first ran.
const OWNER_DATA_SQL: &str = include_str!("migrations/004_migrate_owner_data.sql");

/// Written in R1.7, registered as v8 in R5-E1 (plan §4.5 ASUMSI).
const DROP_V4_SQL: &str = include_str!("migrations/005_drop_v4.sql");

/// Highest schema version written by research plan v4.
const V4_MAX_VERSION: i64 = 2;

/// Source note of the manual counts carried over from the v4 `shares` table (R3.2).
const V4_SHARES_NOTE: &str = "v4 scraper snapshot";
/// `source_runs.source` that marks the one-time carry as done.
pub(crate) const V4_SHARES_CARRY: &str = "v4_shares_carry";

/// `005_drop_v4.sql` was written before `007` existed and also drops
/// `prices_daily.adj_close`; once `007` has run that statement would fail, so it is left
/// out. The file itself stays as written.
fn drop_v4_sql_for(conn: &Connection) -> rusqlite::Result<String> {
    let has_adj_close = conn
        .prepare("PRAGMA table_info(prices_daily)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|c| c == "adj_close");
    Ok(DROP_V4_SQL
        .lines()
        .filter(|l| has_adj_close || l.trim() != "ALTER TABLE prices_daily DROP COLUMN adj_close;")
        .collect::<Vec<_>>()
        .join("\n"))
}

/// `research.sqlite` copied to `research.sqlite.pre-005-<date>.bak` before the v4 tables go.
fn backup_before_drop(conn: &Connection) -> Result<()> {
    let Some(path) = conn
        .path()
        .filter(|p| !p.is_empty() && *p != ":memory:")
        .map(PathBuf::from)
    else {
        return Ok(());
    };
    let suffix = format!("pre-005-{}", chrono::Utc::now().format("%Y-%m-%d"));
    let backup = backup_path(&path, &suffix);
    conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
    tracing::info!("research store: backup before 005 at {}", backup.display());
    Ok(())
}

/// The v8 step (plan §4.5 step 3): bring the canon tables up to date with the v4 tables,
/// refuse when an owner row would be lost, then drop the v4 tables. Returns the log lines
/// of the one-time shares carry for the caller to append.
fn drop_v4_step(tx: &Connection, now: &str) -> Result<Vec<LogEntry>> {
    tx.execute_batch(OWNER_DATA_SQL)?;
    let carried = carry_v4_shares(tx, now)?;
    let v4 = OwnerCounts::read(tx)?;
    let canon = OwnerCounts::read_migrated(tx)?;
    // A note or target the owner deleted in v4 since 004 first ran stays in the append-only
    // journal, so the canon side may hold more; it must never hold fewer.
    // `shares_manual` is one table on both sides (the carry wrote into it), so it is not
    // compared.
    if canon.notes < v4.notes
        || canon.targets < v4.targets
        || canon.manual_universe < v4.manual_universe
    {
        return Err(StoreError::Migration(format!(
            "cannot drop v4 tables: v4 {v4:?} vs canon {canon:?}"
        )));
    }
    tx.execute_batch(&drop_v4_sql_for(tx)?)?;
    Ok(carried)
}

/// Carries the v4 `shares` snapshot into `shares_manual` as entries with source note
/// `v4 scraper snapshot` (D13 option a, R3.2), **once**: a `source_runs` row under
/// `V4_SHARES_CARRY` marks it done, so a count the owner deletes stays deleted. Period-dated
/// rows are carried unless the owner entered that date; "current" rows (dated by the
/// scraper run) only for tickers without any entry of their own. The returned log lines
/// keep the rows through a `rebuild` once `shares` is dropped.
fn carry_v4_shares(conn: &Connection, now: &str) -> Result<Vec<LogEntry>> {
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM source_runs WHERE source = ?1 AND ok = 1",
        [V4_SHARES_CARRY],
        |r| r.get(0),
    )?;
    let has_shares: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'shares'",
        [],
        |r| r.get(0),
    )?;
    if done > 0 || has_shares == 0 {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String, f64)> = {
        let mut st = conn.prepare(
            "SELECT s.ticker, s.as_of, s.shares FROM shares s
             WHERE s.shares > 0
               AND s.source = (SELECT x.source FROM shares x WHERE x.ticker = s.ticker AND x.as_of = s.as_of
                               ORDER BY x.source = 'xbrl_period' DESC LIMIT 1)
               AND NOT EXISTS (SELECT 1 FROM shares_manual m WHERE m.ticker = s.ticker AND m.as_of = s.as_of)
               AND (s.source = 'xbrl_period'
                    OR NOT EXISTS (SELECT 1 FROM shares_manual m WHERE m.ticker = s.ticker))
             ORDER BY s.ticker, s.as_of",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let entries: Vec<LogEntry> = rows
        .into_iter()
        .map(|(ticker, as_of, shares)| LogEntry::SharesManualSet {
            ticker,
            shares,
            as_of,
            source_note: V4_SHARES_NOTE.into(),
            entered_at: now.into(),
            keyed: true,
        })
        .collect();
    for entry in &entries {
        apply(conn, entry)?;
    }
    conn.execute(
        "INSERT INTO source_runs (source, ticker, started_at, finished_at, ok, rows, error)
         VALUES (?1, NULL, ?2, ?2, 1, ?3, NULL)",
        params![V4_SHARES_CARRY, now, entries.len() as i64],
    )?;
    if !entries.is_empty() {
        tracing::info!(
            "research store: {} v4 share snapshot row(s) carried into shares_manual",
            entries.len()
        );
    }
    Ok(entries)
}

/// Runs one migration script. A script with `ADD COLUMN` statements skips a column the table
/// already has: SQLite has no `ADD COLUMN IF NOT EXISTS`, and a store rolled back by setting
/// `user_version` (deploy/RUNBOOK.md) still holds the columns the migration added.
fn run_migration(tx: &rusqlite::Transaction<'_>, sql: &str) -> Result<()> {
    if !sql.contains("ADD COLUMN") {
        return Ok(tx.execute_batch(sql)?);
    }
    let script: String = sql
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    for statement in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let words: Vec<&str> = statement.split_whitespace().collect();
        if let ["ALTER", "TABLE", table, "ADD", "COLUMN", column, ..] = words.as_slice() {
            let present: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                [table, column],
                |r| r.get(0),
            )?;
            if present {
                continue;
            }
        }
        tx.execute_batch(statement)?;
    }
    Ok(())
}

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.0)
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("research store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("research store io: {0}")]
    Io(#[from] std::io::Error),
    #[error("research log: {0}")]
    Json(#[from] serde_json::Error),
    #[error("research.sqlite is schema v{found}, this build knows up to v{known}")]
    TooNew { found: i64, known: i64 },
    #[error("research.sqlite is schema v{found}, this build needs at least v{known}")]
    TooOld { found: i64, known: i64 },
    #[error("research store migration refused: {0}")]
    Migration(String),
}

/// Owner-authored rows that the v4 upgrade must carry over without loss (plan §4.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerCounts {
    pub notes: i64,
    pub targets: i64,
    pub shares_manual: i64,
    /// Distinct tickers the owner added by hand (`universe_overrides` with `action = 'add'`).
    pub manual_universe: i64,
}

fn count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |r| r.get(0))
}

impl OwnerCounts {
    /// Counts on the v4 tables, which exist before and after the upgrade (until 005).
    pub fn read(conn: &Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            notes: count(conn, "SELECT COUNT(*) FROM notes")?,
            targets: count(conn, "SELECT COUNT(*) FROM targets")?,
            shares_manual: count(conn, "SELECT COUNT(*) FROM shares_manual")?,
            manual_universe: count(
                conn,
                "SELECT COUNT(DISTINCT ticker) FROM universe_overrides WHERE action = 'add'",
            )?,
        })
    }

    /// The same quantities as the canon tables see them after 004.
    pub fn read_migrated(conn: &Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            notes: count(conn, "SELECT COUNT(*) FROM journal WHERE id LIKE 'v4note:%'")?,
            targets: count(conn, "SELECT COUNT(*) FROM journal WHERE id LIKE 'v4target:%'")?,
            shares_manual: count(conn, "SELECT COUNT(*) FROM shares_manual")?,
            manual_universe: count(
                conn,
                "SELECT COUNT(*) FROM universe WHERE ticker IN (SELECT ticker FROM universe_overrides WHERE action = 'add')",
            )?,
        })
    }
}

/// `research.sqlite` → `research.sqlite.<suffix>.bak`, never overwriting an earlier backup.
fn backup_path(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "research.sqlite".into());
    let dir = path.parent().unwrap_or(Path::new(""));
    let mut candidate = dir.join(format!("{name}.{suffix}.bak"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{name}.{suffix}-{n}.bak"));
        n += 1;
    }
    candidate
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone, Debug, PartialEq)]
pub struct ManualShares {
    pub ticker: String,
    pub shares: f64,
    pub as_of: String,
    pub source_note: String,
    pub entered_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PriceBar {
    pub date: String,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: f64,
    pub volume: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Dividend,
    Split,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CorpEventRow {
    pub ticker: String,
    pub date: String,
    pub kind: EventKind,
    pub amount: Option<f64>,
    pub ratio_num: Option<f64>,
    pub ratio_den: Option<f64>,
}

/// One line of `research-log.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LogEntry {
    Override {
        ticker: String,
        action: String,
        at: String,
    },
    /// `keyed` (R3.2) means one row per `(ticker, as_of)`; lines written before R3.2
    /// (no flag) meant "replace the ticker's single count" and replay that way.
    SharesManualSet {
        ticker: String,
        shares: f64,
        as_of: String,
        source_note: String,
        entered_at: String,
        #[serde(default)]
        keyed: bool,
    },
    /// `as_of = None` removes every count of the ticker (the v4 contract); `Some` one row.
    SharesManualDelete {
        ticker: String,
        at: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<String>,
    },
    TargetSet {
        ticker: String,
        entry_price: f64,
        set_at: String,
    },
    TargetDelete {
        ticker: String,
        at: String,
    },
    NoteAdd {
        id: String,
        ticker: String,
        created_at: String,
        text: String,
    },
    NoteDelete {
        id: String,
        at: String,
    },
    /// Canon universe row (R2.2): manual, peer, or the owner's columns on a held row.
    UniverseUpsert {
        ticker: String,
        role: String,
        peer_of: Option<String>,
        pack_override: Option<String>,
        business_note: Option<String>,
        added_at: String,
        synced_at: String,
    },
    UniverseDelete {
        ticker: String,
        at: String,
    },
    /// Operating KPI with provenance (R2.3); `unknown` is distinct from not entered.
    KpiSet {
        ticker: String,
        metric: String,
        fy: i32,
        q: u32,
        value: Option<f64>,
        unknown: bool,
        unit: String,
        document: String,
        document_date: String,
        entered_at: String,
    },
    KpiDelete {
        ticker: String,
        metric: String,
        fy: i32,
        q: u32,
        at: String,
    },
    /// Owner scenario row (R3.4): bear, base or bull; partial inputs allowed.
    ScenarioSet {
        ticker: String,
        name: String,
        growth: Option<f64>,
        margin: Option<f64>,
        exit_multiple: Option<f64>,
        years: Option<u32>,
        updated_at: String,
    },
    ScenarioDelete {
        ticker: String,
        name: String,
        at: String,
    },
    /// R4: a claim, its evaluations and its closing (research plan §4.1).
    ClaimCreate {
        claim: ClaimRow,
    },
    ClaimEval {
        eval: ClaimEvalRow,
    },
    ClaimClose {
        id: String,
        closed_at: String,
    },
    /// R4: journal entries and annotations are only ever added (D9).
    JournalAdd {
        entry: JournalRow,
    },
    JournalAnnotate {
        annotation: AnnotationRow,
    },
    DocumentAdd {
        document: DocumentRow,
    },
    EvidenceLink {
        evidence: EvidenceRow,
    },
    /// R4: inbox events are logged with their dismissals so a rebuild restores the inbox.
    EventAdd {
        event: EventRow,
    },
    EventDismiss {
        id: String,
        at: String,
    },
    /// An open event's payload refreshed by its producer (KPI cells answered since).
    EventUpdate {
        id: String,
        payload: String,
    },
    CalendarSet {
        entry: CalendarRow,
    },
    CalendarDelete {
        ticker: String,
        date: String,
        kind: String,
        at: String,
    },
    /// The owner opened a ticker. Lines written before R5 also carry the v4 `eligibility`
    /// and `zone`; serde ignores those two fields on replay.
    SeenSet {
        ticker: String,
        seen_at: String,
    },
    /// R8: a KPI value the rule-based extractor proposes from an uploaded document. The
    /// document itself is a `DocumentAdd` line.
    ProposalAdd {
        proposal: ProposalRow,
    },
    /// R8: the owner's decision on a proposal (`accepted` or `rejected`). An accept also
    /// writes `KpiSet` and `EvidenceLink` lines; a reject writes nothing else.
    ProposalDecide {
        id: String,
        status: String,
        decided_at: String,
    },
    /// Riset redesign: the whole structure map of a ticker (every edit, accept, reject and
    /// draft import writes the full node list, so the last line per ticker wins on replay).
    StructureSet {
        ticker: String,
        nodes: Vec<StructureNode>,
        at: String,
    },
}

/// One row of `scenarios` (research plan §4.1).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScenarioRow {
    pub ticker: String,
    pub name: String,
    pub growth: Option<f64>,
    pub margin: Option<f64>,
    pub exit_multiple: Option<f64>,
    pub years: Option<u32>,
    pub updated_at: String,
}

pub struct Store {
    conn: Mutex<Connection>,
    log_path: Option<PathBuf>,
    /// Bumped by every write that can change an evaluation, so caches know when to drop.
    generation: AtomicU64,
}

impl Store {
    /// Opens (creating if needed) the database at `path`, migrates it, and keeps the JSONL
    /// log beside it. A brand-new database next to an existing log is rebuilt from that log.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let log_path = path.with_file_name(LOG_FILE);
        let store = Self {
            conn: Mutex::new(conn),
            log_path: Some(log_path.clone()),
            generation: AtomicU64::new(0),
        };
        let found = store.user_version()?;
        let fresh = found == 0;
        if found > 0 && found <= V4_MAX_VERSION {
            store.upgrade_from_v4(path, &log_path)?;
            // The upgrade stops at the drop step (v8); later migrations run now.
            store.migrate()?;
        } else {
            store.migrate()?;
        }
        if fresh && log_path.exists() {
            store.rebuild()?;
        }
        Ok(store)
    }

    /// Opens `path` and applies only the migrations up to `upto` (tests build a synthetic
    /// v4 store with `upto = 2`). Not used by the server.
    #[doc(hidden)]
    pub fn open_at_version(path: &Path, upto: i64) -> Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self {
            conn: Mutex::new(conn),
            log_path: Some(path.with_file_name(LOG_FILE)),
            generation: AtomicU64::new(0),
        };
        store.migrate_to(upto)?;
        Ok(store)
    }

    /// The v4 → canon upgrade (plan §4.5): backup via `VACUUM INTO`, every pending migration
    /// in one transaction, owner-data counts asserted equal, then the old log renamed.
    fn upgrade_from_v4(&self, path: &Path, log_path: &Path) -> Result<()> {
        let suffix = format!("v4-{}", chrono::Utc::now().format("%Y-%m-%d"));
        let backup = backup_path(path, &suffix);
        let mut conn = self.conn();
        conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
        tracing::info!(
            "research store: v4 backup written to {} before migrating",
            backup.display()
        );
        // An immediate transaction serialises concurrent openers (server and CLI); the
        // version is read inside it so the loser finds the work already done.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let found: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if found > V4_MAX_VERSION {
            drop(tx);
            drop(conn);
            return self.migrate();
        }
        let before = OwnerCounts::read(&tx)?;
        tracing::info!("research store: owner data before migration {before:?}");
        for (version, sql) in MIGRATIONS
            .iter()
            .filter(|(v, _)| *v > found && *v < DROP_V4_VERSION)
        {
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
        }
        let after = OwnerCounts::read_migrated(&tx)?;
        let still = OwnerCounts::read(&tx)?;
        tracing::info!("research store: owner data after migration {after:?}");
        if before != after || before != still {
            // Dropping `tx` rolls everything back; the backup stays on disk.
            return Err(StoreError::Migration(format!(
                "owner data counts differ: before {before:?}, migrated {after:?}, v4 tables {still:?}; backup at {}",
                backup.display()
            )));
        }
        // The v4 log is seeded from the v4 tables and the drop step (v8) runs in this same
        // transaction, so no state exists where the tables are gone and the log is not seeded.
        let mut lines = Self::seed_entries(&tx)?;
        lines.extend(drop_v4_step(&tx, &chrono::Utc::now().to_rfc3339())?);
        tx.execute_batch(&format!("PRAGMA user_version = {DROP_V4_VERSION}"))?;
        if log_path.exists() {
            let renamed = backup_path(log_path, &suffix);
            match std::fs::rename(log_path, &renamed) {
                Ok(()) => tracing::info!(
                    "research store: v4 log kept at {}; a new {LOG_FILE} starts",
                    renamed.display()
                ),
                // A stuck log file must not block the server.
                Err(e) => tracing::warn!(
                    "research store: could not rename {}: {e}; appending to it instead",
                    log_path.display()
                ),
            }
        }
        // Logged before the commit: a failed append rolls the upgrade back (the next open
        // starts over from the v4 state), and a failed commit leaves only lines that replay
        // as the same upserts.
        for entry in &lines {
            self.append_log(entry)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// One log line per owner-authored v4 row currently stored, so a log started after the
    /// upgrade still rebuilds every override, manual count, target and note.
    fn seed_entries(conn: &Connection) -> Result<Vec<LogEntry>> {
        let mut entries = Vec::new();
        let mut st = conn.prepare("SELECT ticker, action, at FROM universe_overrides")?;
        for row in st.query_map([], |r| {
            Ok(LogEntry::Override {
                ticker: r.get(0)?,
                action: r.get(1)?,
                at: r.get(2)?,
            })
        })? {
            entries.push(row?);
        }
        let mut st = conn
            .prepare("SELECT ticker, shares, as_of, source_note, entered_at FROM shares_manual")?;
        for row in st.query_map([], |r| {
            Ok(LogEntry::SharesManualSet {
                ticker: r.get(0)?,
                shares: r.get(1)?,
                as_of: r.get(2)?,
                source_note: r.get(3)?,
                entered_at: r.get(4)?,
                keyed: true,
            })
        })? {
            entries.push(row?);
        }
        let mut st = conn.prepare("SELECT ticker, entry_price, set_at FROM targets")?;
        for row in st.query_map([], |r| {
            Ok(LogEntry::TargetSet {
                ticker: r.get(0)?,
                entry_price: r.get(1)?,
                set_at: r.get(2)?,
            })
        })? {
            entries.push(row?);
        }
        let mut st = conn.prepare("SELECT id, ticker, created_at, text FROM notes")?;
        for row in st.query_map([], |r| {
            Ok(LogEntry::NoteAdd {
                id: r.get(0)?,
                ticker: r.get(1)?,
                created_at: r.get(2)?,
                text: r.get(3)?,
            })
        })? {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Table names in the schema, for tests that assert what the drop left behind.
    pub fn table_names(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// In-memory store without a log (tests).
    pub fn open_memory() -> Result<Self> {
        let store = Self {
            conn: Mutex::new(Connection::open_in_memory()?),
            log_path: None,
            generation: AtomicU64::new(0),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Changes whenever data feeding an evaluation was written.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn user_version(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// Applies every migration newer than `user_version`; running it again is a no-op.
    pub fn migrate(&self) -> Result<()> {
        self.migrate_to(i64::MAX)
    }

    /// Applies the migrations newer than `user_version` up to `upto`, one transaction each.
    fn migrate_to(&self, upto: i64) -> Result<()> {
        let mut conn = self.conn();
        let found: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if found > latest_version() {
            return Err(StoreError::TooNew {
                found,
                known: latest_version(),
            });
        }
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > found && *v <= upto) {
            if *version == DROP_V4_VERSION {
                // VACUUM INTO cannot run inside a transaction; a store that never held data
                // has nothing to protect.
                if found > 0 {
                    backup_before_drop(&conn)?;
                }
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                // Another opener may have finished the step while this one waited.
                let now: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
                if now >= *version {
                    continue;
                }
                let carried = drop_v4_step(&tx, &chrono::Utc::now().to_rfc3339())?;
                tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
                tx.commit()?;
                // Logged after the commit, so a failed commit leaves no carried line behind;
                // a failed append is loud, and the carry marker keeps it from running twice.
                for entry in &carried {
                    if let Err(e) = self.append_log(entry) {
                        tracing::error!("research store: carried share line not logged: {e}");
                    }
                }
                continue;
            }
            // Immediate, with the version read again inside: a second opener (the service and
            // the ingest CLI) that waited finds the step done instead of repeating it.
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let now: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if now >= *version {
                continue;
            }
            run_migration(&tx, sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(&mut self.conn())
    }

    // ---- universe (canon rows, R2.2) ----

    /// Portfolio sync: every held ticker gets (or keeps) role `held`; a held or legacy
    /// `watch` row that is no longer held becomes `manual`. Owner columns and manual or
    /// peer rows are untouched.
    pub fn sync_held(&self, held: &[String], synced_at: &str) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for t in held {
            tx.execute(
                "INSERT INTO universe (ticker, role, synced_at, added_at)
                 VALUES (?1, 'held', ?2, ?2)
                 ON CONFLICT(ticker) DO UPDATE SET role = 'held', synced_at = excluded.synced_at",
                params![t, synced_at],
            )?;
        }
        tx.execute(
            "UPDATE universe SET role = 'manual', synced_at = ?2
             WHERE role IN ('held', 'watch') AND ticker NOT IN (SELECT value FROM json_each(?1))",
            params![serde_json::to_string(held)?, synced_at],
        )?;
        Ok(tx.commit()?)
    }

    pub fn universe(&self) -> Result<Vec<UniverseEntry>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, role, peer_of, pack_override, business_note, added_at, synced_at
             FROM universe ORDER BY ticker",
        )?;
        let rows = st.query_map([], universe_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn universe_entry(&self, ticker: &str) -> Result<Option<UniverseEntry>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT ticker, role, peer_of, pack_override, business_note, added_at, synced_at
                 FROM universe WHERE ticker = ?1",
                [ticker],
                universe_row,
            )
            .optional()?)
    }

    /// Inserts or updates one row (logged). A row that is `held` stays held.
    pub fn upsert_universe(&self, e: &UniverseEntry) -> Result<()> {
        self.apply_and_log(&LogEntry::UniverseUpsert {
            ticker: e.ticker.clone(),
            role: e.role.as_str().into(),
            peer_of: e.peer_of.clone(),
            pack_override: e.pack_override.clone(),
            business_note: e.business_note.clone(),
            added_at: e.added_at.clone(),
            synced_at: e.synced_at.clone(),
        })
    }

    pub fn delete_universe(&self, ticker: &str, at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::UniverseDelete {
            ticker: ticker.into(),
            at: at.into(),
        })
    }

    // ---- operating KPIs (R2.3, logged) ----

    pub fn set_kpi(&self, k: &KpiValue) -> Result<()> {
        self.apply_and_log(&LogEntry::KpiSet {
            ticker: k.ticker.clone(),
            metric: k.metric.clone(),
            fy: k.fy,
            q: k.q,
            value: k.value,
            unknown: k.unknown,
            unit: k.unit.clone(),
            document: k.document.clone(),
            document_date: k.document_date.clone(),
            entered_at: k.entered_at.clone(),
        })
    }

    pub fn delete_kpi(&self, ticker: &str, metric: &str, fy: i32, q: u32, at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::KpiDelete {
            ticker: ticker.into(),
            metric: metric.into(),
            fy,
            q,
            at: at.into(),
        })
    }

    /// Every KPI value of `ticker`, oldest period first.
    pub fn kpis(&self, ticker: &str) -> Result<Vec<KpiValue>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, metric, fy, q, value, unknown, unit, document, document_date, entered_at
             FROM kpi_values WHERE ticker = ?1 ORDER BY metric, fy, q",
        )?;
        let rows = st.query_map([ticker], kpi_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn kpi(&self, ticker: &str, metric: &str, fy: i32, q: u32) -> Result<Option<KpiValue>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT ticker, metric, fy, q, value, unknown, unit, document, document_date, entered_at
                 FROM kpi_values WHERE ticker = ?1 AND metric = ?2 AND fy = ?3 AND q = ?4",
                params![ticker, metric, fy, q],
                kpi_row,
            )
            .optional()?)
    }

    /// `(values, unknown, tickers, newest entered_at)` for health.
    pub fn kpi_summary(&self) -> Result<(i64, i64, i64, Option<String>)> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*), COALESCE(SUM(unknown), 0), COUNT(DISTINCT ticker), MAX(entered_at) FROM kpi_values",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?)
    }

    // ---- user data (logged) ----

    pub fn set_shares_manual(&self, m: &ManualShares) -> Result<()> {
        self.apply_and_log(&LogEntry::SharesManualSet {
            ticker: m.ticker.clone(),
            shares: m.shares,
            as_of: m.as_of.clone(),
            source_note: m.source_note.clone(),
            entered_at: m.entered_at.clone(),
            keyed: true,
        })
    }

    pub fn delete_shares_manual(&self, ticker: &str, at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::SharesManualDelete {
            ticker: ticker.into(),
            at: at.into(),
            as_of: None,
        })
    }

    /// Removes one dated count (the canon composite key, R3.2).
    pub fn delete_shares_manual_at(&self, ticker: &str, as_of: &str, at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::SharesManualDelete {
            ticker: ticker.into(),
            at: at.into(),
            as_of: Some(as_of.into()),
        })
    }

    pub fn shares_manual(&self, ticker: &str) -> Result<Option<ManualShares>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT ticker, shares, as_of, source_note, entered_at FROM shares_manual
                 WHERE ticker = ?1 ORDER BY as_of DESC LIMIT 1",
                [ticker],
                |r| {
                    Ok(ManualShares {
                        ticker: r.get(0)?,
                        shares: r.get(1)?,
                        as_of: r.get(2)?,
                        source_note: r.get(3)?,
                        entered_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    fn apply_and_log(&self, entry: &LogEntry) -> Result<()> {
        self.bump();
        apply(&self.conn(), entry)?;
        self.append_log(entry)
    }

    fn append_log(&self, entry: &LogEntry) -> Result<()> {
        let Some(path) = &self.log_path else {
            return Ok(());
        };
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        f.write_all(line.as_bytes())?;
        Ok(f.flush()?)
    }

    /// Clears the user-data tables and replays the log. A torn or unreadable line is skipped.
    pub fn rebuild(&self) -> Result<usize> {
        let Some(path) = &self.log_path else {
            return Ok(0);
        };
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for table in [
            "shares_manual",
            "kpi_values",
            "scenarios",
            "claim_evals",
            "claims",
            "journal_annotations",
            "extraction_proposals",
            "structure_maps",
            "evidence",
            "documents",
            "events",
            "calendar_manual",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        // Entries migrated from the v4 notes and targets (004) are kept as they are (their
        // log lines replay as the same inserts); everything else in the journal replays from
        // the log. `seen` is only ever overwritten by the replay, so rows written before it
        // was logged survive.
        tx.execute(
            "DELETE FROM journal WHERE id NOT LIKE 'v4note:%' AND id NOT LIKE 'v4target:%'",
            [],
        )?;
        // Held rows come from the portfolio sync; everything the owner wrote replays.
        tx.execute(
            "DELETE FROM universe WHERE role IN ('manual', 'peer', 'watch')",
            [],
        )?;
        tx.execute(
            "UPDATE universe SET peer_of = NULL, pack_override = NULL, business_note = NULL",
            [],
        )?;
        // A v4 note or target deleted later in the log must not come back through its add.
        let entries: Vec<Option<LogEntry>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<LogEntry>(l).ok())
            .collect();
        let mut note_deleted_at = std::collections::HashMap::new();
        let mut target_deleted_at = std::collections::HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            match e {
                Some(LogEntry::NoteDelete { id, .. }) => {
                    note_deleted_at.insert(id.clone(), i);
                }
                Some(LogEntry::TargetDelete { ticker, .. }) => {
                    target_deleted_at.insert(ticker.clone(), i);
                }
                _ => {}
            }
        }
        let mut applied = 0;
        for (i, entry) in entries.iter().enumerate() {
            let removed_later = match entry {
                Some(LogEntry::NoteAdd { id, .. }) => {
                    note_deleted_at.get(id).is_some_and(|d| *d > i)
                }
                Some(LogEntry::TargetSet { ticker, .. }) => {
                    target_deleted_at.get(ticker).is_some_and(|d| *d > i)
                }
                _ => false,
            };
            if removed_later {
                continue;
            }
            match entry {
                None => tracing::warn!("research log: skipping unreadable line"),
                Some(entry) => {
                    // A row the CHECKs now reject must not abort the whole replay.
                    match apply(&tx, entry) {
                        Ok(()) => applied += 1,
                        Err(e) => tracing::warn!("research log: line skipped on replay: {e}"),
                    }
                }
            }
        }
        tx.commit()?;
        Ok(applied)
    }

    // ---- derived data (rebuildable from sources) ----

    /// Upsert only: a failed or partial fetch never deletes stored bars.
    pub fn upsert_prices(&self, ticker: &str, bars: &[PriceBar]) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for b in bars {
            tx.execute(
                "INSERT OR REPLACE INTO prices_daily (ticker, date, open, high, low, close, volume)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![ticker, b.date, b.open, b.high, b.low, b.close, b.volume],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub fn upsert_events(&self, events: &[CorpEventRow]) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for e in events {
            let kind = match e.kind {
                EventKind::Dividend => "dividend",
                EventKind::Split => "split",
            };
            tx.execute(
                "INSERT OR REPLACE INTO corp_events (ticker, date, kind, amount, ratio_num, ratio_den)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![e.ticker, e.date, kind, e.amount, e.ratio_num, e.ratio_den],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Oldest first.
    pub fn prices(&self, ticker: &str) -> Result<Vec<PriceBar>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT date, open, high, low, close, volume FROM prices_daily
             WHERE ticker = ?1 ORDER BY date",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok(PriceBar {
                date: r.get(0)?,
                open: r.get(1)?,
                high: r.get(2)?,
                low: r.get(3)?,
                close: r.get(4)?,
                volume: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The newest `n` closes as `(date, close)`, oldest first.
    pub fn last_closes(&self, ticker: &str, n: usize) -> Result<Vec<(String, f64)>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT date, close FROM prices_daily WHERE ticker = ?1 ORDER BY date DESC LIMIT ?2",
        )?;
        let rows = st.query_map(params![ticker, n as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out: Vec<(String, f64)> = rows.collect::<std::result::Result<_, _>>()?;
        out.reverse();
        Ok(out)
    }

    pub fn events(&self, ticker: &str) -> Result<Vec<CorpEventRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, date, kind, amount, ratio_num, ratio_den FROM corp_events
             WHERE ticker = ?1 ORDER BY date, kind",
        )?;
        let rows = st.query_map([ticker], |r| {
            let kind: String = r.get(2)?;
            Ok(CorpEventRow {
                ticker: r.get(0)?,
                date: r.get(1)?,
                kind: if kind == "split" {
                    EventKind::Split
                } else {
                    EventKind::Dividend
                },
                amount: r.get(3)?,
                ratio_num: r.get(4)?,
                ratio_den: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- held positions (average cost) ----

    /// Replaces the stored average costs with the portfolio's current positions.
    pub fn replace_held(&self, positions: &[(String, f64)]) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM held_positions", [])?;
        for (ticker, avg) in positions {
            tx.execute(
                "INSERT OR REPLACE INTO held_positions (ticker, avg_price) VALUES (?1, ?2)",
                params![ticker, avg],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub fn avg_price(&self, ticker: &str) -> Result<Option<f64>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT avg_price FROM held_positions WHERE ticker = ?1",
                [ticker],
                |r| r.get(0),
            )
            .optional()?)
    }

    // ---- snapshots and seen (derived; not in the user-data log) ----

    pub fn set_seen(&self, ticker: &str, seen_at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::SeenSet {
            ticker: ticker.into(),
            seen_at: seen_at.into(),
        })
    }

    pub fn seen(&self, ticker: &str) -> Result<Option<Seen>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT ticker, seen_at FROM seen WHERE ticker = ?1",
                [ticker],
                |r| {
                    Ok(Seen {
                        ticker: r.get(0)?,
                        seen_at: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    // ---- scenarios (R3.4, logged) ----

    pub fn set_scenario(&self, s: &ScenarioRow) -> Result<()> {
        self.apply_and_log(&LogEntry::ScenarioSet {
            ticker: s.ticker.clone(),
            name: s.name.clone(),
            growth: s.growth,
            margin: s.margin,
            exit_multiple: s.exit_multiple,
            years: s.years,
            updated_at: s.updated_at.clone(),
        })
    }

    pub fn delete_scenario(&self, ticker: &str, name: &str, at: &str) -> Result<()> {
        self.apply_and_log(&LogEntry::ScenarioDelete {
            ticker: ticker.into(),
            name: name.into(),
            at: at.into(),
        })
    }

    /// The owner's scenario rows of `ticker`, in name order (base, bear, bull).
    pub fn scenarios(&self, ticker: &str) -> Result<Vec<ScenarioRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, name, growth, margin, exit_multiple, years, updated_at
             FROM scenarios WHERE ticker = ?1 ORDER BY name",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok(ScenarioRow {
                ticker: r.get(0)?,
                name: r.get(1)?,
                growth: r.get(2)?,
                margin: r.get(3)?,
                exit_multiple: r.get(4)?,
                years: r.get::<_, Option<i64>>(5)?.map(|y| y as u32),
                updated_at: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- drivers (R3.3) ----

    /// Upsert only: a failed or partial fetch never deletes stored observations.
    pub fn upsert_drivers(
        &self,
        series: &str,
        source: &str,
        fetched_at: &str,
        points: &[(String, f64)],
    ) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (date, value) in points {
            tx.execute(
                "INSERT OR REPLACE INTO drivers (series, date, value, source, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![series, date, value, source, fetched_at],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub fn driver_last_date(&self, series: &str) -> Result<Option<String>> {
        Ok(self.conn().query_row(
            "SELECT MAX(date) FROM drivers WHERE series = ?1",
            [series],
            |r| r.get(0),
        )?)
    }

    /// `(date, value)` of one series, oldest first, from `from` (inclusive) when given.
    pub fn driver_series(&self, series: &str, from: Option<&str>) -> Result<Vec<(String, f64)>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT date, value FROM drivers WHERE series = ?1 AND (?2 IS NULL OR date >= ?2) ORDER BY date",
        )?;
        let rows = st.query_map(params![series, from], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One row per stored series: how much is there and how fresh it is.
    pub fn driver_summaries(&self) -> Result<Vec<DriverSummary>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT series, COUNT(*), MIN(date), MAX(date), MAX(fetched_at), MIN(source)
             FROM drivers GROUP BY series ORDER BY series",
        )?;
        let rows = st.query_map([], |r| {
            Ok(DriverSummary {
                series: r.get(0)?,
                points: r.get(1)?,
                first: r.get(2)?,
                last: r.get(3)?,
                fetched_at: r.get(4)?,
                source: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn price_count(&self, ticker: &str) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM prices_daily WHERE ticker = ?1",
            [ticker],
            |r| r.get(0),
        )?)
    }

    pub fn record_source_run(&self, run: &SourceRun) -> Result<()> {
        self.conn().execute(
            "INSERT INTO source_runs (source, ticker, started_at, finished_at, ok, rows, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.source,
                run.ticker,
                run.started_at,
                run.finished_at,
                run.ok,
                run.rows,
                run.error
            ],
        )?;
        Ok(())
    }

    /// `finished_at` of the newest successful run of `source` (for `ticker`, when given).
    pub fn last_ok_run(&self, source: &str, ticker: Option<&str>) -> Result<Option<String>> {
        Ok(self.conn().query_row(
            "SELECT MAX(finished_at) FROM source_runs WHERE source = ?1 AND ticker IS ?2 AND ok = 1",
            params![source, ticker],
            |r| r.get(0),
        )?)
    }

    /// Newest run of `source` over all tickers, successful or not.
    pub fn latest_run(&self, source: &str) -> Result<Option<SourceRun>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT source, ticker, started_at, finished_at, ok, rows, error FROM source_runs
                 WHERE source = ?1 ORDER BY started_at DESC, rowid DESC LIMIT 1",
                [source],
                |r| {
                    Ok(SourceRun {
                        source: r.get(0)?,
                        ticker: r.get(1)?,
                        started_at: r.get(2)?,
                        finished_at: r.get(3)?,
                        ok: r.get(4)?,
                        rows: r.get(5)?,
                        error: r.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn source_runs(&self, source: &str) -> Result<Vec<SourceRun>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT source, ticker, started_at, finished_at, ok, rows, error FROM source_runs
             WHERE source = ?1 ORDER BY rowid",
        )?;
        let rows = st.query_map([source], |r| {
            Ok(SourceRun {
                source: r.get(0)?,
                ticker: r.get(1)?,
                started_at: r.get(2)?,
                finished_at: r.get(3)?,
                ok: r.get(4)?,
                rows: r.get(5)?,
                error: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// When the user last opened the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub ticker: String,
    pub seen_at: String,
}

/// One adapter run, for the health card.
/// Stored extent of one driver series (R3.3).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DriverSummary {
    pub series: String,
    pub points: i64,
    pub first: String,
    pub last: String,
    pub fetched_at: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceRun {
    pub source: String,
    pub ticker: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub ok: bool,
    pub rows: Option<i64>,
    pub error: Option<String>,
}

fn universe_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UniverseEntry> {
    let role: String = r.get(1)?;
    Ok(UniverseEntry {
        ticker: r.get(0)?,
        role: Role::parse(&role).unwrap_or(Role::Manual),
        peer_of: r.get(2)?,
        pack_override: r.get(3)?,
        business_note: r.get(4)?,
        added_at: r.get(5)?,
        synced_at: r.get(6)?,
    })
}

fn kpi_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<KpiValue> {
    Ok(KpiValue {
        ticker: r.get(0)?,
        metric: r.get(1)?,
        fy: r.get(2)?,
        q: r.get(3)?,
        value: r.get(4)?,
        unknown: r.get::<_, i64>(5)? != 0,
        unit: r.get(6)?,
        document: r.get(7)?,
        document_date: r.get(8)?,
        entered_at: r.get(9)?,
    })
}

/// Applies one user-data change to the tables (shared by live writes and `rebuild`).
/// The close known when a v4 note or target was written (as in 004): the same day's close
/// only once the exchange has closed (16:00 WIB = 09:00 UTC), otherwise the last close before.
fn price_at_sql(ticker: &str, at: &str) -> String {
    format!(
        "(SELECT p.close FROM prices_daily p WHERE p.ticker = {ticker}
            AND p.date <= CASE WHEN substr({at}, 12, 5) >= '09:00'
                               THEN substr({at}, 1, 10)
                               ELSE date(substr({at}, 1, 10), '-1 day') END
          ORDER BY p.date DESC LIMIT 1)"
    )
}

fn apply(conn: &Connection, entry: &LogEntry) -> rusqlite::Result<()> {
    match entry {
        // v4 lines replay into the canon tables the drop step migrated them to: an `add`
        // override is a manual universe row, a note or target a journal entry (ids and
        // timestamps as in 004). Deletions are not replayed: the journal is append-only.
        LogEntry::Override { ticker, action, at } if action == "add" => conn.execute(
            "INSERT OR IGNORE INTO universe (ticker, role, synced_at, added_at)
             VALUES (?1, 'manual', ?2, ?2)",
            params![ticker, at],
        ),
        // A v4 `remove` hid a ticker from the automatic universe; 004 ignores it, so the
        // replay does too (a canon removal is a `UniverseDelete` line).
        LogEntry::Override { .. } => Ok(0),
        LogEntry::SharesManualSet {
            ticker,
            shares,
            as_of,
            source_note,
            entered_at,
            keyed,
        } => {
            if !keyed {
                // A pre-R3.2 line: "set" meant replace the ticker's single count.
                conn.execute("DELETE FROM shares_manual WHERE ticker = ?1", [ticker])?;
            }
            conn.execute(
            // Canon composite key (R3.2): one count per (ticker, as_of); the same date is
            // replaced, other dates stay as the point-in-time series.
            "INSERT INTO shares_manual (ticker, shares, as_of, source_note, entered_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(ticker, as_of) DO UPDATE SET
               shares = excluded.shares, source_note = excluded.source_note,
               entered_at = excluded.entered_at",
            params![ticker, shares, as_of, source_note, entered_at],
            )
        }
        LogEntry::SharesManualDelete {
            ticker,
            as_of: Some(as_of),
            ..
        } => conn.execute(
            "DELETE FROM shares_manual WHERE ticker = ?1 AND as_of = ?2",
            params![ticker, as_of],
        ),
        LogEntry::SharesManualDelete { ticker, .. } => {
            conn.execute("DELETE FROM shares_manual WHERE ticker = ?1", [ticker])
        }
        LogEntry::TargetSet {
            ticker,
            entry_price,
            set_at,
        } => conn.execute(
            &format!(
                "INSERT OR IGNORE INTO journal (id, ticker, at, kind, text, price_at, level)
                 VALUES ('v4target:' || ?1 || ':' || ?2, ?1, ?2, 'level', 'Harga incaran v4', {}, ?3)",
                price_at_sql("?1", "?2")
            ),
            params![ticker, set_at, entry_price],
        ),
        LogEntry::NoteAdd {
            id,
            ticker,
            created_at,
            text,
        } => conn.execute(
            &format!(
                "INSERT OR IGNORE INTO journal (id, ticker, at, kind, text, price_at, level)
                 VALUES ('v4note:' || ?1, ?2, ?3, 'catatan', ?4, {}, NULL)",
                price_at_sql("?2", "?3")
            ),
            params![id, ticker, created_at, text],
        ),
        LogEntry::TargetDelete { .. } | LogEntry::NoteDelete { .. } => Ok(0),
        LogEntry::UniverseUpsert {
            ticker,
            role,
            peer_of,
            pack_override,
            business_note,
            added_at,
            synced_at,
        } => conn.execute(
            // `held` is the portfolio sync's to grant: a logged row written while the ticker
            // was held replays as `manual` once it is not, so a rebuild cannot resurrect a
            // sold holding as held.
            "INSERT INTO universe (ticker, role, peer_of, pack_override, business_note, added_at, synced_at)
             VALUES (?1, CASE WHEN ?2 = 'held' THEN 'manual' ELSE ?2 END, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(ticker) DO UPDATE SET
               role = CASE WHEN universe.role = 'held' THEN 'held'
                           WHEN excluded.role = 'held' THEN universe.role
                           ELSE excluded.role END,
               peer_of = excluded.peer_of,
               pack_override = excluded.pack_override,
               business_note = excluded.business_note",
            params![ticker, role, peer_of, pack_override, business_note, added_at, synced_at],
        ),
        LogEntry::UniverseDelete { ticker, .. } => conn.execute(
            "DELETE FROM universe WHERE ticker = ?1 AND role <> 'held'",
            [ticker],
        ),
        LogEntry::KpiSet {
            ticker,
            metric,
            fy,
            q,
            value,
            unknown,
            unit,
            document,
            document_date,
            entered_at,
        } => conn.execute(
            "INSERT OR REPLACE INTO kpi_values (ticker, metric, fy, q, value, unknown, unit, document, document_date, entered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![ticker, metric, fy, q, value, *unknown as i64, unit, document, document_date, entered_at],
        ),
        LogEntry::KpiDelete {
            ticker,
            metric,
            fy,
            q,
            ..
        } => conn.execute(
            "DELETE FROM kpi_values WHERE ticker = ?1 AND metric = ?2 AND fy = ?3 AND q = ?4",
            params![ticker, metric, fy, q],
        ),
        LogEntry::ScenarioSet {
            ticker,
            name,
            growth,
            margin,
            exit_multiple,
            years,
            updated_at,
        } => conn.execute(
            "INSERT OR REPLACE INTO scenarios (ticker, name, growth, margin, exit_multiple, years, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![ticker, name, growth, margin, exit_multiple, years.map(i64::from), updated_at],
        ),
        LogEntry::ScenarioDelete { ticker, name, .. } => conn.execute(
            "DELETE FROM scenarios WHERE ticker = ?1 AND name = ?2",
            params![ticker, name],
        ),
        other => r4::apply(conn, other),
    }
    .map(|_| ())
}
