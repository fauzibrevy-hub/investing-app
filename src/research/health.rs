//! `research_*` entries of `GET /api/health?deep=1`: cheap local probes only, read-only.

use std::fmt::Write as _;
use std::path::Path;
use std::time::SystemTime;

use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Map, Value};

use super::packs::Packs;
use super::prices::drivers;
use super::prices::yahoo::STALE_DAYS;
use super::store::latest_version;
use super::store::RESTATED_YEARS_SQL;
use crate::config::Config;

const SCRAPER_MAX_AGE_DAYS: u64 = 14;

fn entry(ok: bool, detail: impl Into<String>) -> Value {
    json!({"ok": ok, "detail": detail.into()})
}

fn open_ro(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

fn age_days(path: &Path) -> Option<u64> {
    let m = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(SystemTime::now().duration_since(m).ok()?.as_secs() / 86_400)
}

/// Newest successful `finished_at` of `source`, as an RFC 3339 string.
fn last_ok(conn: &Connection, source: &str) -> Option<String> {
    conn.query_row(
        "SELECT MAX(finished_at) FROM source_runs WHERE source = ?1 AND ok = 1",
        [source],
        |r| r.get(0),
    )
    .ok()
    .flatten()
}

fn run_age_days(finished: &str) -> Option<i64> {
    let t = chrono::DateTime::parse_from_rfc3339(finished).ok()?;
    Some(
        (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
            .num_days()
            .max(0),
    )
}

/// Adds `last_at` (RFC 3339 or null) and `age_days` (whole days or null) to a health entry
/// (A7.2: every research source reports its status and its age as data, not only in `detail`).
fn with_age(mut v: Value, last_at: Option<&str>) -> Value {
    v["last_at"] = last_at.map_or(Value::Null, |t| json!(t));
    v["age_days"] = last_at
        .and_then(run_age_days)
        .map_or(Value::Null, |d| json!(d));
    v
}

/// File the scraper job writes beside the watchlist after every run (`deploy/scraper.sh`).
pub const SCRAPER_STATUS_FILE: &str = "scraper-status.json";
/// A run that started but never wrote `finished_at` is "running" for this long, then "did not finish".
const SCRAPER_RUNNING_HOURS: i64 = 6;

/// Schedule in `deploy/com.menganto.investing.scraper.plist`: Weekday 0 (Sunday), 03:00 local.
pub const SCRAPER_SCHEDULE_WEEKDAY: chrono::Weekday = chrono::Weekday::Sun;
pub const SCRAPER_SCHEDULE_HOUR: u32 = 3;
/// A run may start this late after its slot (Mac asleep at 03:00, runs at wake) before it is `missed`.
const SCRAPER_GRACE_HOURS: i64 = 6;
/// Timestamps further ahead than this are a clock problem, not a run.
const SKEW_TOLERANCE_MINUTES: i64 = 5;

/// Most recent scheduled slot (Sunday 03:00 local) at or before `now`.
pub fn last_scheduled_run(now: chrono::DateTime<chrono::Local>) -> chrono::DateTime<chrono::Local> {
    use chrono::{Datelike, Duration, TimeZone};
    let mut day = now.date_naive();
    for _ in 0..8 {
        if day.weekday() == SCRAPER_SCHEDULE_WEEKDAY {
            let at = day
                .and_hms_opt(SCRAPER_SCHEDULE_HOUR, 0, 0)
                .expect("valid time");
            if let Some(t) = chrono::Local.from_local_datetime(&at).earliest() {
                if t <= now {
                    return t;
                }
            }
        }
        day -= Duration::days(1);
    }
    now
}

fn parse_utc(t: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    Some(
        chrono::DateTime::parse_from_rfc3339(t)
            .ok()?
            .with_timezone(&chrono::Utc),
    )
}

/// Whole hours since `t`, never negative.
fn age_hours(t: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    Some((now - parse_utc(t)?).num_hours().max(0))
}

/// The scraper job: last run time, exit status and step from `scraper-status.json` (R7.1, A7.1).
/// Without that file (job never ran since the status write existed) the old probe stays: the
/// watchlist exists and `idx.sqlite` is younger than 14 days.
fn scraper_job_entry(cfg: &Config) -> Value {
    scraper_job_entry_at(cfg, chrono::Local::now())
}

/// `scraper_job_entry` with an explicit clock (tests pin it to a weekday).
pub fn scraper_job_entry_at(cfg: &Config, now_local: chrono::DateTime<chrono::Local>) -> Value {
    let status_path = cfg
        .research_scraper_watchlist
        .parent()
        .map(|p| p.join(SCRAPER_STATUS_FILE));
    let idx_age = age_days(&cfg.idx_db_path);
    let raw = status_path.as_deref().filter(|p| p.is_file());
    let Some(path) = raw else {
        let mut v = match (cfg.research_scraper_watchlist.is_file(), idx_age) {
            (false, _) => entry(false, "watchlist.csv not written yet"),
            (true, None) => entry(false, "idx.sqlite not found"),
            (true, Some(d)) if d <= SCRAPER_MAX_AGE_DAYS => entry(
                true,
                format!("idx.sqlite {d} d old; no run status file yet"),
            ),
            (true, Some(d)) => entry(
                false,
                format!("idx.sqlite {d} d old; scraper job not running"),
            ),
        };
        v["state"] = json!("no_status");
        v["exit_code"] = Value::Null;
        v["idx_age_days"] = idx_age.map_or(Value::Null, |d| json!(d));
        return with_age(v, None);
    };
    let parsed: Option<Value> = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let Some(st) = parsed.filter(Value::is_object) else {
        let mut v = entry(false, "scraper-status.json unreadable");
        v["state"] = json!("unreadable");
        v["exit_code"] = Value::Null;
        return with_age(v, None);
    };
    let started = st["started_at"].as_str();
    let finished = st["finished_at"].as_str();
    let code = st["exit_code"].as_i64();
    let step = st["step"].as_str().unwrap_or("?");
    let new_files = st["new_files"].as_i64().filter(|n| *n >= 0);
    let now = now_local.with_timezone(&chrono::Utc);
    let ahead = |t: Option<&str>| {
        t.and_then(parse_utc)
            .is_some_and(|t| t - now > chrono::Duration::minutes(SKEW_TOLERANCE_MINUTES))
    };
    let slot = last_scheduled_run(now_local);
    let slot_utc = slot.with_timezone(&chrono::Utc);
    let grace = chrono::Duration::hours(SCRAPER_GRACE_HOURS);
    let files = new_files.map_or(String::new(), |n| {
        format!(", {n} new file{}", if n == 1 { "" } else { "s" })
    });
    let (ok, state, detail, last_at) = if ahead(started) || ahead(finished) {
        (
            false,
            "clock_skew",
            format!(
                "status timestamps are in the future (started {}, finished {}); check the Mac's clock",
                started.unwrap_or("-"),
                finished.unwrap_or("-")
            ),
            None,
        )
    } else {
        match (finished, code) {
            (Some(f), Some(0)) => match parse_utc(f) {
                Some(t) if now - t > chrono::Duration::days(SCRAPER_MAX_AGE_DAYS as i64) => (
                    false,
                    "stale",
                    format!(
                        "last run {f} exit 0, {} d ago; scraper job not running",
                        (now - t).num_days()
                    ),
                    Some(f),
                ),
                Some(t) => {
                    let began = started.and_then(parse_utc).unwrap_or(t);
                    if began < slot_utc && now - slot_utc > grace {
                        (
                            false,
                            "missed",
                            format!(
                                "last run {f} exit 0; the run due {} did not record a status (job not firing, or the status could not be written)",
                                slot.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
                            ),
                            Some(f),
                        )
                    } else {
                        (
                            true,
                            "ok",
                            format!(
                                "last run {f} exit 0{files}, {} d ago",
                                (now - t).num_days().max(0)
                            ),
                            Some(f),
                        )
                    }
                }
                None => (
                    false,
                    "unreadable",
                    format!("finished_at {f} unreadable"),
                    None,
                ),
            },
            (Some(f), c) => (
                false,
                "failed",
                format!(
                    "last run {f} failed: exit {} at step {step}",
                    c.map_or("?".into(), |c| c.to_string())
                ),
                Some(f),
            ),
            (None, _) => match started.and_then(|t| age_hours(t, now)) {
                Some(h) if h < SCRAPER_RUNNING_HOURS => (
                    true,
                    "running",
                    format!(
                        "running since {} ({h} h) at step {step}",
                        started.unwrap_or("?")
                    ),
                    started,
                ),
                _ => (
                    false,
                    "unfinished",
                    format!(
                        "run started {} did not finish (killed or crashed at step {step})",
                        started.unwrap_or("?")
                    ),
                    started,
                ),
            },
        }
    };
    let mut v = entry(ok, detail);
    v["state"] = json!(state);
    v["exit_code"] = code.map_or(Value::Null, |c| json!(c));
    v["step"] = json!(step);
    v["new_files"] = new_files.map_or(Value::Null, |n| json!(n));
    v["last_scheduled_at"] = json!(slot.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
    v["started_at"] = started.map_or(Value::Null, |t| json!(t));
    v["finished_at"] = finished.map_or(Value::Null, |t| json!(t));
    v["age_hours"] = last_at
        .and_then(|t| age_hours(t, now))
        .map_or(Value::Null, |h| json!(h));
    v["idx_age_days"] = idx_age.map_or(Value::Null, |d| json!(d));
    with_age(v, last_at)
}

fn count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |r| r.get(0))
}

/// Filings on file, newest period, and every file the parser could not read (never hidden).
fn filings_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let filings = count(c, "SELECT COUNT(*) FROM filings")?;
        let tickers = count(c, "SELECT COUNT(DISTINCT ticker) FROM filings")?;
        let newest: Option<String> = c
            .query_row(
                "SELECT ticker || ' ' || fy || ' ' || period FROM filings ORDER BY period_end DESC, ticker LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();
        let mut st =
            c.prepare("SELECT source_path, error FROM ingest_errors ORDER BY source_path")?;
        let unparsed: Vec<(String, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let pdf = count(c, "SELECT COUNT(*) FROM filings WHERE source_kind = 'pdf'")?;
        let mut detail = format!("{filings} filings for {tickers} tickers");
        if pdf > 0 {
            detail.push_str(&format!(" ({pdf} from original PDF)"));
        }
        if let Some(n) = newest {
            detail.push_str(&format!(", newest {n}"));
        }
        if !unparsed.is_empty() {
            detail.push_str(&format!(
                "; {} unreadable: {}",
                unparsed.len(),
                unparsed
                    .iter()
                    .map(|(p, e)| format!("{p} ({e})"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let newest_end: Option<String> = c
            .query_row("SELECT MAX(period_end) FROM filings", [], |r| r.get(0))
            .ok()
            .flatten();
        let mut v = json!({
            "ok": filings > 0 && unparsed.is_empty(),
            "detail": detail,
            "filings": filings,
            "pdf_filings": pdf,
            "unparsed": unparsed.len(),
            "newest_period_end": newest_end,
        });
        v = with_age(v, last_ok(c, super::xbrl::SOURCE).as_deref());
        Ok(v)
    };
    probe().unwrap_or_else(|e| entry(false, format!("filings: {e}")))
}

/// Operating KPI values entered, how many are "unknown", and the newest entry's age.
fn kpi_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let (values, unknown, tickers, newest): (i64, i64, i64, Option<String>) = c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(unknown), 0), COUNT(DISTINCT ticker), MAX(entered_at) FROM kpi_values",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let age = newest.as_deref().and_then(run_age_days);
        let detail = match age {
            Some(d) => format!(
                "{values} values ({unknown} unknown) for {tickers} tickers; last entry {d} d ago"
            ),
            None => "no KPI entered yet".to_string(),
        };
        Ok(with_age(
            json!({"ok": true, "detail": detail, "values": values, "unknown": unknown, "tickers": tickers}),
            newest.as_deref(),
        ))
    };
    probe().unwrap_or_else(|e| entry(false, format!("kpi: {e}")))
}

/// Packs loaded from the directory or the embedded copy; any invalid file makes it not-ok.
fn packs_entry(cfg: &Config) -> Value {
    let s = Packs::load(&cfg.research_packs_dir, &chrono::Utc::now().to_rfc3339()).status();
    let mut detail = format!("{} packs from {}", s.set.len(), s.source);
    if !s.embedded_ids.is_empty() && s.source != "embedded" {
        detail.push_str(&format!("; embedded: {}", s.embedded_ids.join(", ")));
    }
    if !s.invalid.is_empty() {
        detail.push_str(&format!(
            "; {} invalid: {}",
            s.invalid.len(),
            s.invalid
                .iter()
                .map(|(f, e)| format!("{f} ({e})"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    json!({"ok": s.ok(), "detail": detail, "loaded": s.set.len(), "invalid": s.invalid.len()})
}

/// Driver series stored per sourced catalogue entry, the last fetch age, and which sourced
/// drivers have nothing yet; TBD drivers are listed, never counted as failures.
fn drivers_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let mut st =
            c.prepare("SELECT series, COUNT(*), MAX(date) FROM drivers GROUP BY series")?;
        let stored: std::collections::HashMap<String, (i64, String)> = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<rusqlite::Result<_>>()?;
        let missing: Vec<&str> = drivers::sourced()
            .filter(|d| !stored.contains_key(d.id))
            .map(|d| d.id)
            .collect();
        let tbd: Vec<&str> = drivers::CATALOGUE
            .iter()
            .filter(|d| !d.has_source())
            .map(|d| d.id)
            .collect();
        let last = last_ok(c, drivers::SOURCE);
        let age = last.as_deref().and_then(run_age_days);
        // Per driver: the newest successful run of that id; one healthy driver never hides
        // another that has failed for weeks (review R3).
        let mut by_id = c.prepare(
            "SELECT MAX(finished_at) FROM source_runs WHERE source = ?1 AND ticker = ?2 AND ok = 1",
        )?;
        let mut stale: Vec<&str> = Vec::new();
        for d in drivers::sourced().filter(|d| stored.contains_key(d.id)) {
            let last: Option<String> = by_id
                .query_row(params![drivers::SOURCE, d.id], |r| r.get(0))
                .ok()
                .flatten();
            let fresh = last
                .as_deref()
                .and_then(run_age_days)
                .is_some_and(|a| a <= drivers::STALE_DAYS);
            if !fresh {
                stale.push(d.id);
            }
        }
        let mut detail = format!(
            "{} of {} sourced series stored",
            drivers::sourced().count() - missing.len(),
            drivers::sourced().count()
        );
        match age {
            Some(d) => detail.push_str(&format!("; last fetch {d} d ago")),
            None => detail.push_str("; never fetched"),
        }
        if !missing.is_empty() {
            detail.push_str(&format!("; missing: {}", missing.join(", ")));
        }
        if !stale.is_empty() {
            detail.push_str(&format!("; stale: {}", stale.join(", ")));
        }
        if !tbd.is_empty() {
            detail.push_str(&format!("; TBD (no free source): {}", tbd.join(", ")));
        }
        let ok = missing.is_empty() && stale.is_empty() && age.is_some();
        Ok(with_age(
            json!({
                "ok": ok, "detail": detail,
                "stored": stored.len(), "missing": missing, "stale": stale, "tbd": tbd,
            }),
            last.as_deref(),
        ))
    };
    probe().unwrap_or_else(|e| entry(false, format!("drivers: {e}")))
}

fn facts_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let facts = count(c, "SELECT COUNT(*) FROM facts")?;
        let derived = count(c, "SELECT COUNT(*) FROM derived")?;
        let last = last_ok(c, super::xbrl::SOURCE);
        let age = last.as_deref().and_then(run_age_days);
        let detail = match age {
            Some(d) => format!("{facts} facts, {derived} derived cells; last ingest {d} d ago"),
            None => format!("{facts} facts, {derived} derived cells; never ingested"),
        };
        Ok(with_age(
            json!({"ok": facts > 0, "detail": detail, "facts": facts, "derived": derived}),
            last.as_deref(),
        ))
    };
    probe().unwrap_or_else(|e| entry(false, format!("facts: {e}")))
}

/// Years whose standalone quarters do not add up to the FY as last reported and that no
/// recorded restatement explains (PRD S1, A1.3; ADR 0001). `failures` is that unexplained
/// count (expected 0); restated years are listed apart and never turn the entry red.
fn reconciliation_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let mut st = c.prepare(
            "SELECT ticker || ' ' || element || ' FY' || fy FROM derived
             WHERE q = 4 AND (',' || flags || ',') LIKE '%,reconcile_failed,%' ORDER BY ticker, element, fy",
        )?;
        let failures: Vec<String> = st
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut st = c.prepare(RESTATED_YEARS_SQL)?;
        let restated: Vec<(String, i32)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut detail = if failures.is_empty() {
            "all derived years reconcile with their FY or are recorded restatements".to_string()
        } else {
            let shown: Vec<&str> = failures.iter().take(10).map(String::as_str).collect();
            format!(
                "{} year(s) do not reconcile: {}{}",
                failures.len(),
                shown.join(", "),
                if failures.len() > 10 { ", ..." } else { "" }
            )
        };
        if !restated.is_empty() {
            let shown: Vec<String> = restated
                .iter()
                .take(10)
                .map(|(t, fy)| format!("{t} FY{fy}"))
                .collect();
            let _ = write!(
                detail,
                "; {} restated ticker-year(s): {}{}",
                restated.len(),
                shown.join(", "),
                if restated.len() > 10 { ", ..." } else { "" }
            );
        }
        Ok(json!({
            "ok": failures.is_empty(),
            "detail": detail,
            "failures": failures.len(),
            "restated_years": restated.len(),
            "restated": restated.iter().map(|(t, fy)| json!({"ticker": t, "fy": fy})).collect::<Vec<_>>(),
        }))
    };
    probe().unwrap_or_else(|e| entry(false, format!("reconciliation: {e}")))
}

/// R8 evidence storage: the uploads folder (created on the first upload, so absent is fine
/// until then; present but read-only or not a directory is not), stored files and document rows.
fn documents_entry(cfg: &Config, c: Option<&Connection>) -> Value {
    let dir = &cfg.research_documents_dir;
    // Read-only probe: only the permission bits are checked, not the service user's access.
    let folder: Result<(String, Option<usize>), String> = match std::fs::metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(("folder not created yet (no upload so far)".into(), None))
        }
        Err(e) => Err(format!("documents folder: {e}")),
        Ok(m) if !m.is_dir() => Err("documents path is not a directory".into()),
        Ok(m) if m.permissions().readonly() => {
            Err("documents folder has no write permission bits".into())
        }
        Ok(_) => match std::fs::read_dir(dir) {
            Err(e) => Err(format!("documents folder: {e}")),
            Ok(d) => {
                let n = d
                    .filter_map(Result::ok)
                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                    .count();
                Ok((format!("{n} files"), Some(n)))
            }
        },
    };
    let rows: Result<i64, String> = match c {
        None => Err("documents: no store".into()),
        Some(c) => c
            .query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))
            .map_err(|e| format!("documents: {e}")),
    };
    let files = folder.as_ref().ok().and_then(|f| f.1);
    let documents = rows.as_ref().ok().copied();
    let ok = folder.is_ok() && rows.is_ok();
    let detail = [
        folder.map(|f| f.0).unwrap_or_else(|e| e),
        rows.map(|n| format!("{n} document rows"))
            .unwrap_or_else(|e| e),
    ]
    .join("; ");
    json!({"ok": ok, "detail": detail, "files": files, "documents": documents})
}

/// Extraction proposals waiting for a decision and the newest extraction (R8). Pending ones are
/// work for the owner, never a failure.
fn proposals_entry(c: &Connection) -> Value {
    let probe = || -> rusqlite::Result<Value> {
        let (pending, total, newest): (i64, i64, Option<String>) = c.query_row(
            "SELECT COALESCE(SUM(status = 'pending'), 0), COUNT(*), MAX(created_at) FROM extraction_proposals",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let detail = match (total, newest.as_deref().and_then(run_age_days)) {
            (0, _) => "no extraction yet".to_string(),
            (_, Some(d)) => format!("{pending} pending of {total}; last extraction {d} d ago"),
            (_, None) => format!("{pending} pending of {total}; last extraction time unreadable"),
        };
        Ok(with_age(
            json!({"ok": true, "detail": detail, "pending": pending, "total": total}),
            newest.as_deref(),
        ))
    };
    probe().unwrap_or_else(|e| entry(false, format!("proposals: {e}")))
}

pub fn sources(cfg: &Config) -> Map<String, Value> {
    let mut m = Map::new();
    let conn = cfg
        .research_db_path
        .is_file()
        .then(|| open_ro(&cfg.research_db_path))
        .flatten();

    let store = match &conn {
        None => with_age(entry(false, "research.sqlite not found"), None),
        Some(c) => {
            // Age of the store = its newest recorded source run (the file mtime lags in WAL mode).
            let newest: Option<String> = c
                .query_row(
                    "SELECT MAX(finished_at) FROM source_runs WHERE source <> ?1",
                    [super::store::V4_SHARES_CARRY],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            let v = match c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)) {
                Ok(v) if v == latest_version() => entry(true, format!("schema v{v}")),
                Ok(v) => entry(
                    false,
                    format!("schema v{v}, expected v{}", latest_version()),
                ),
                Err(e) => entry(false, e.to_string()),
            };
            with_age(v, newest.as_deref())
        }
    };
    m.insert("research_store".into(), store);

    let run_status = |source: &str, what: &str, max_age: i64| match conn.as_ref() {
        None => with_age(entry(false, format!("{what}: no store")), None),
        Some(c) => {
            let last = last_ok(c, source);
            let v = match last.as_deref().and_then(run_age_days) {
                Some(age) if age <= max_age => entry(true, format!("{what} fetched {age} d ago")),
                Some(age) => entry(false, format!("{what} stale: {age} d")),
                None => entry(false, format!("{what}: never fetched")),
            };
            with_age(v, last.as_deref())
        }
    };
    m.insert(
        "research_prices".into(),
        run_status("yahoo", "prices", STALE_DAYS),
    );
    m.insert(
        "research_xbrl".into(),
        run_status(
            super::xbrl::SOURCE,
            "xbrl directory",
            SCRAPER_MAX_AGE_DAYS as i64,
        ),
    );

    // Canon v1 tables (research plan §5 health row): filings, facts, reconciliation.
    match conn.as_ref() {
        None => {
            for k in [
                "research_filings",
                "research_facts",
                "research_reconciliation",
                "research_kpi",
                "research_drivers",
            ] {
                m.insert(k.into(), with_age(entry(false, "no store"), None));
            }
        }
        Some(c) => {
            m.insert("research_filings".into(), filings_entry(c));
            m.insert("research_facts".into(), facts_entry(c));
            m.insert("research_reconciliation".into(), reconciliation_entry(c));
            m.insert("research_kpi".into(), kpi_entry(c));
            m.insert("research_drivers".into(), drivers_entry(c));
        }
    }
    m.insert(
        "research_documents".into(),
        documents_entry(cfg, conn.as_ref()),
    );
    m.insert(
        "research_proposals".into(),
        match conn.as_ref() {
            None => with_age(entry(false, "no store"), None),
            Some(c) => proposals_entry(c),
        },
    );
    m.insert("research_packs".into(), packs_entry(cfg));

    m.insert(
        "research_emitens".into(),
        if cfg.idx_emitens_csv.is_file() {
            entry(true, "emitens.csv")
        } else {
            entry(false, "emitens.csv not found; company names are missing")
        },
    );
    m.insert("research_scraper_job".into(), scraper_job_entry(cfg));
    m
}
