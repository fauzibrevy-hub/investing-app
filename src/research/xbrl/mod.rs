//! Raw IDX XBRL filings (research plan §3.1, §4.2): `parse` turns one `.xbrl` file into
//! contexts, units and every numeric fact, `dei` reads the `idx-dei` header, `labels` maps
//! element names to display labels. `parse_filing` combines them into the `Filing` row the
//! store keeps plus its facts; ingestion into the store and the directory watch build on it.

pub mod dei;
pub mod labels;
pub mod parse;
pub mod pdf;
pub mod ratio;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use sha2::{Digest, Sha256};

use super::derive;
use super::store::{SourceRun, Store, StoreError};

pub use dei::{AvailableSource, Header, Period};
pub use parse::{Cite, CtxKind, Document, Fact};

/// `"<TICKER>_<FY>_<Q1|Q2|Q3|FY>"`.
pub type FilingId = String;

pub fn filing_id(ticker: &str, fy: i32, period: Period) -> FilingId {
    format!("{ticker}_{fy}_{}", period.as_str())
}

/// Where a filing's facts were read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// An IDX XBRL instance document.
    Xbrl,
    /// An original PDF, read outside the app and checked into a JSON file (`pdf`).
    Pdf,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xbrl => "xbrl",
            Self::Pdf => "pdf",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "xbrl" => Some(Self::Xbrl),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// One row of `filings`.
#[derive(Clone, Debug, PartialEq)]
pub struct Filing {
    pub id: FilingId,
    pub ticker: String,
    pub fy: i32,
    pub period: Period,
    pub taxonomy: String,
    pub industry_code: Option<String>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub audited: bool,
    pub rounding: String,
    pub currency: String,
    pub filed_at: Option<NaiveDate>,
    pub available_from: NaiveDate,
    pub available_source: AvailableSource,
    pub source_path: String,
    pub file_hash: String,
    pub source_kind: SourceKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub filing: Filing,
    pub facts: Vec<Fact>,
    pub entity_name: Option<String>,
    pub skipped_non_numeric: usize,
    pub skipped_nil: usize,
    pub conflicts: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum FilingError {
    #[error("read: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] parse::ParseError),
    #[error(transparent)]
    Header(#[from] dei::DeiError),
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Ticker from a scraper-style file name `TICKER_YYYY_Qn.xbrl`, used only when the header
/// has no `EntityCode`.
fn ticker_from_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    stem.split('_').next().map(str::to_string)
}

fn first_currency_unit(doc: &Document) -> Option<&str> {
    doc.units
        .iter()
        .map(|u| u.measure.as_str())
        .find(|m| m.len() == 3 && m.bytes().all(|b| b.is_ascii_uppercase()))
}

/// Builds the filing row and facts from a document already in memory.
pub fn from_document(
    mut doc: Document,
    source_path: &Path,
    file_hash: String,
) -> Result<Parsed, FilingError> {
    ratio::normalise(&mut doc.facts);
    let name_ticker = ticker_from_name(source_path);
    let header = dei::header(&doc.dei, name_ticker.as_deref(), first_currency_unit(&doc))?;
    let (available_from, available_source) = dei::available_from(header.period_end, header.period);
    let filing = Filing {
        id: filing_id(&header.ticker, header.fy, header.period),
        ticker: header.ticker,
        fy: header.fy,
        period: header.period,
        taxonomy: header.taxonomy,
        industry_code: header.industry_code,
        period_start: header.period_start,
        period_end: header.period_end,
        audited: header.audited,
        rounding: header.rounding,
        currency: header.currency,
        filed_at: None,
        available_from,
        available_source,
        source_path: source_path.to_string_lossy().into_owned(),
        file_hash,
        source_kind: SourceKind::Xbrl,
    };
    Ok(Parsed {
        filing,
        facts: doc.facts,
        entity_name: header.entity_name,
        skipped_non_numeric: doc.skipped_non_numeric,
        skipped_nil: doc.skipped_nil,
        conflicts: doc.conflicts,
    })
}

/// Reads and parses one `.xbrl` file.
pub fn parse_filing(path: &Path) -> Result<Parsed, FilingError> {
    let bytes = std::fs::read(path)?;
    let hash = sha256_hex(&bytes);
    let text = String::from_utf8_lossy(&bytes);
    let doc = parse::parse_str(&text)?;
    from_document(doc, path, hash)
}

// ---- ingestion into the store (plan §4.2) ----

/// `source_runs.source` of a directory ingest run.
pub const SOURCE: &str = "xbrl_dir";

/// What one `ingest_new` pass did.
#[derive(Debug, Default)]
pub struct IngestReport {
    pub scanned: usize,
    pub unchanged: usize,
    pub ingested: Vec<FilingId>,
    /// `(path, error)` of files that could not be read or parsed; also stored for health.
    pub unparsed: Vec<(String, String)>,
    /// `(filing id, warning)` for filings ingested with a caveat (non-calendar period).
    pub warnings: Vec<(String, String)>,
    pub derived_tickers: Vec<String>,
    pub reconciliation_failures: Vec<(String, derive::ReconciliationFailure)>,
    /// Ids of PDF filings an XBRL filing replaced or kept out this pass (the XBRL one wins).
    pub pdf_superseded: Vec<FilingId>,
}

/// Size and mtime per file at the last pass (the scheduler keeps one across ticks).
pub type FileMeta = std::collections::HashMap<PathBuf, (u64, std::time::SystemTime)>;

/// Every `.xbrl` file under `dir`, recursively, in path order, plus `(directory, error)` for
/// every directory that could not be listed. Symbolic links are not followed, so a link
/// cycle cannot loop.
pub fn list_files_checked(dir: &Path) -> (Vec<PathBuf>, Vec<(String, String)>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) => {
                errors.push((d.to_string_lossy().into_owned(), format!("read_dir: {e}")));
                continue;
            }
        };
        for e in entries {
            let Ok(e) = e else {
                continue;
            };
            let Ok(kind) = e.file_type() else {
                continue;
            };
            let p = e.path();
            if kind.is_dir() {
                stack.push(p);
            } else if kind.is_file()
                && p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("xbrl"))
            {
                files.push(p);
            }
        }
    }
    files.sort();
    (files, errors)
}

/// Every `.xbrl` file under `dir`, recursively, in path order.
pub fn list_files(dir: &Path) -> Vec<PathBuf> {
    list_files_checked(dir).0
}

/// True when the filing's period is a calendar year-to-date, which the derivation assumes.
fn calendar_period(f: &Filing) -> bool {
    use chrono::Datelike;
    f.period_start.month() == 1
        && f.period_start.day() == 1
        && f.period_start.year() == f.period_end.year()
        && derive::quarter_end(f.period_end) == Some(f.period.quarter())
}

/// Re-derives every undimensioned element of `ticker` from the stored facts.
pub fn rederive(store: &Store, ticker: &str, now: &str) -> Result<derive::Derived, StoreError> {
    let facts = store.facts_for_derive(ticker)?;
    let derived = derive::derive(&facts);
    store.replace_derived(ticker, &derived.cells, &derived.restatements, now)?;
    Ok(derived)
}

/// What a stale-rules pass did: the tickers re-derived and the unexplained reconciliation
/// failures their new cells carry; a ticker that could not be derived is listed in `failed`
/// and does not stop the others.
#[derive(Debug, Default)]
pub struct RederiveReport {
    pub tickers: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub reconciliation_failures: Vec<(String, derive::ReconciliationFailure)>,
}

/// Re-derives every ticker whose derived cells carry another rules version than
/// `derive::RULES_VERSION` (or that has facts and no cells), so a rules bump reaches data
/// whose filings did not change. Only `derived` and `restatements` are rewritten (via
/// `rederive`); once stamped with the current version a ticker is not selected again.
pub fn rederive_stale(store: &Store, now: &str) -> Result<RederiveReport, StoreError> {
    let mut report = RederiveReport::default();
    for ticker in store.tickers_with_stale_rules()? {
        let d = match rederive(store, &ticker, now) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("research rederive {ticker}: {e}");
                report.failed.push((ticker, e.to_string()));
                continue;
            }
        };
        tracing::info!(
            "research rederive {ticker}: {} cells under rules {}, {} restatement(s), {} reconciliation failure(s)",
            d.cells.len(),
            derive::RULES_VERSION,
            d.restatements.len(),
            d.reconciliation_failures.len()
        );
        for f in &d.reconciliation_failures {
            tracing::warn!("research reconciliation {ticker}: {}", f.explain());
        }
        report.reconciliation_failures.extend(
            d.reconciliation_failures
                .into_iter()
                .map(|f| (ticker.clone(), f)),
        );
        report.tickers.push(ticker);
    }
    Ok(report)
}

/// Ingests every file under `dir` whose hash the store has not seen (new or changed), then
/// re-derives the tickers touched. Unreadable files are recorded, never skipped silently.
/// Contract for the scheduler: the ids of the filings ingested this pass.
pub fn ingest_new(
    store: &Store,
    dir: &Path,
    now: DateTime<Utc>,
) -> Result<IngestReport, StoreError> {
    ingest_new_with(store, dir, now, None)
}

/// `ingest_new` with an optional size/mtime cache: a file whose size and mtime are unchanged
/// since the last pass is counted as unchanged without being read or hashed.
pub fn ingest_new_with(
    store: &Store,
    dir: &Path,
    now: DateTime<Utc>,
    cache: Option<&mut FileMeta>,
) -> Result<IngestReport, StoreError> {
    ingest_pass(store, dir, None, now, cache, false)
}

/// `ingest_new_with` that also reads the original-filing JSON files of `pdf_dir` (`pdf`):
/// XBRL files first, then the PDF ones, one re-derivation for both. A `pdf_dir` that does
/// not exist is an empty source, not an error: the directory is optional.
pub fn ingest_new_with_pdf(
    store: &Store,
    dir: &Path,
    pdf_dir: Option<&Path>,
    now: DateTime<Utc>,
    cache: Option<&mut FileMeta>,
) -> Result<IngestReport, StoreError> {
    ingest_pass(store, dir, pdf_dir, now, cache, false)
}

/// Re-parses every file under `dir` even when its hash is already stored, replacing the facts
/// of each filing and re-deriving the tickers. For after a parser change (the plain pass sees
/// an unchanged file and skips it).
pub fn reingest_all(
    store: &Store,
    dir: &Path,
    now: DateTime<Utc>,
) -> Result<IngestReport, StoreError> {
    ingest_pass(store, dir, None, now, None, true)
}

/// `reingest_all` that also re-reads the PDF filings of `pdf_dir`.
pub fn reingest_all_with_pdf(
    store: &Store,
    dir: &Path,
    pdf_dir: Option<&Path>,
    now: DateTime<Utc>,
) -> Result<IngestReport, StoreError> {
    ingest_pass(store, dir, pdf_dir, now, None, true)
}

/// A file the pass has to parse: its path text, bytes and sha256.
struct Loaded {
    path: String,
    bytes: Vec<u8>,
    hash: String,
}

/// Reads `path` unless it is unchanged. `None` means nothing to parse: unchanged (counted)
/// or unreadable (recorded for health, never skipped silently).
fn load_changed(
    store: &Store,
    path: &Path,
    started: &str,
    force: bool,
    known: &HashMap<String, String>,
    cache: Option<&mut FileMeta>,
    report: &mut IngestReport,
) -> Result<Option<Loaded>, StoreError> {
    let path_str = path.to_string_lossy().into_owned();
    let meta = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (m.len(), t)));
    if let (Some(cache), Some(meta)) = (cache.as_deref(), meta) {
        if !force && cache.get(path) == Some(&meta) && known.contains_key(&path_str) {
            report.unchanged += 1;
            return Ok(None);
        }
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("read: {e}");
            store.record_ingest_error(&path_str, "", &msg, started)?;
            report.unparsed.push((path_str, msg));
            return Ok(None);
        }
    };
    let hash = sha256_hex(&bytes);
    if let (Some(cache), Some(meta)) = (cache, meta) {
        cache.insert(path.to_path_buf(), meta);
    }
    if !force && known.get(&path_str) == Some(&hash) {
        report.unchanged += 1;
        return Ok(None);
    }
    Ok(Some(Loaded {
        path: path_str,
        bytes,
        hash,
    }))
}

fn path_texts(files: &[PathBuf]) -> HashSet<String> {
    files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Why the file at `path` must not replace `existing`, the stored filing of the same id, if
/// it must not. The same bytes under another path are the same filing whose file moved (a
/// moved data dir, a rename) once this pass no longer lists the stored path: the filing
/// follows its file. Anything else is a second file for one filing id, reported not merged.
fn duplicate_of(
    source: &str,
    existing: &Filing,
    path: &str,
    hash: &str,
    listed: &HashSet<String>,
) -> Option<String> {
    if existing.source_path == path {
        return None;
    }
    if existing.file_hash == hash && !listed.contains(&existing.source_path) {
        tracing::info!(
            "{source} {path}: filing {} moved from {}",
            existing.id,
            existing.source_path
        );
        return None;
    }
    Some(format!(
        "duplicate filing id {}: already ingested from {}",
        existing.id, existing.source_path
    ))
}

/// An ingest error recorded for the bytes of a stored filing is no broken file: it is a
/// moved file reported as a duplicate of its old path. Such a file is read again rather
/// than skipped as unchanged, so the move is recognised and the error cleared.
fn retry_moved(store: &Store, known: &mut HashMap<String, String>) -> Result<(), StoreError> {
    let errors = store.ingest_errors()?;
    if errors.is_empty() {
        return Ok(());
    }
    let stored: HashSet<String> = store
        .filings(None)?
        .into_iter()
        .map(|r| r.filing.file_hash)
        .collect();
    for e in errors {
        if stored.contains(&e.file_hash) {
            known.remove(&e.source_path);
        }
    }
    Ok(())
}

fn ingest_pass(
    store: &Store,
    dir: &Path,
    pdf_dir: Option<&Path>,
    now: DateTime<Utc>,
    mut cache: Option<&mut FileMeta>,
    force: bool,
) -> Result<IngestReport, StoreError> {
    let started = now.to_rfc3339();
    let mut known = store.known_file_hashes()?;
    retry_moved(store, &mut known)?;
    let mut report = IngestReport::default();
    let mut tickers = BTreeSet::new();
    let (files, dir_errors) = list_files_checked(dir);
    for (d, e) in dir_errors {
        tracing::warn!("research xbrl {d}: {e}");
        report.unparsed.push((d, e));
    }
    let listed = path_texts(&files);
    for path in files {
        report.scanned += 1;
        let Some(Loaded {
            path: path_str,
            bytes,
            hash,
        }) = load_changed(
            store,
            &path,
            &started,
            force,
            &known,
            cache.as_deref_mut(),
            &mut report,
        )?
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let parsed = parse::parse_str(&text)
            .map_err(FilingError::from)
            .and_then(|doc| from_document(doc, &path, hash.clone()));
        match parsed {
            Ok(p) => {
                if p.conflicts > 0 {
                    tracing::warn!(
                        "research xbrl {}: {} duplicate facts with different values, first kept",
                        p.filing.id,
                        p.conflicts
                    );
                }
                // Two files for the same filing id would replace each other every pass;
                // the one already stored wins and the newcomer is reported, not merged.
                // The exception is a filing read from a PDF: an XBRL filing of the same id
                // always replaces it.
                if let Some(existing) = store.filing(&p.filing.id)? {
                    if existing.filing.source_kind == SourceKind::Pdf {
                        tracing::info!(
                            "research xbrl {path_str}: replaces the PDF filing {}",
                            p.filing.id
                        );
                        report.pdf_superseded.push(p.filing.id.clone());
                    } else if let Some(msg) =
                        duplicate_of("research xbrl", &existing.filing, &path_str, &hash, &listed)
                    {
                        tracing::warn!("research xbrl {path_str}: {msg}");
                        store.record_ingest_error(&path_str, &hash, &msg, &started)?;
                        report.unparsed.push((path_str, msg));
                        continue;
                    }
                }
                if !calendar_period(&p.filing) {
                    report.warnings.push((
                        p.filing.id.clone(),
                        format!(
                            "period {} to {} is not a calendar year-to-date; its cells will show as gaps",
                            p.filing.period_start, p.filing.period_end
                        ),
                    ));
                }
                store.upsert_filing(&p.filing, &p.facts, &started)?;
                tickers.insert(p.filing.ticker.clone());
                report.ingested.push(p.filing.id);
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!("research xbrl {path_str}: {msg}");
                store.record_ingest_error(&path_str, &hash, &msg, &started)?;
                report.unparsed.push((path_str, msg));
            }
        }
    }
    if let Some(pdf_dir) = pdf_dir.filter(|d| d.is_dir()) {
        ingest_pdf_files(
            store,
            pdf_dir,
            &started,
            force,
            &known,
            cache,
            &mut report,
            &mut tickers,
        )?;
    }
    for ticker in tickers {
        let d = rederive(store, &ticker, &started)?;
        report.reconciliation_failures.extend(
            d.reconciliation_failures
                .into_iter()
                .map(|f| (ticker.clone(), f)),
        );
        report.derived_tickers.push(ticker);
    }
    let error = (!report.unparsed.is_empty()).then(|| {
        format!(
            "{} unreadable file(s): {}",
            report.unparsed.len(),
            report
                .unparsed
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    store.record_source_run(&SourceRun {
        source: SOURCE.into(),
        ticker: None,
        started_at: started,
        finished_at: Some(Utc::now().to_rfc3339()),
        ok: true,
        rows: Some(report.ingested.len() as i64),
        error,
    })?;
    Ok(report)
}

/// The PDF half of a pass: every `.json` file of `dir`, validated as a PDF filing (`pdf`).
/// Runs after the XBRL files, so a filing id that XBRL already holds keeps its XBRL facts.
#[allow(clippy::too_many_arguments)]
fn ingest_pdf_files(
    store: &Store,
    dir: &Path,
    started: &str,
    force: bool,
    known: &HashMap<String, String>,
    mut cache: Option<&mut FileMeta>,
    report: &mut IngestReport,
    tickers: &mut BTreeSet<String>,
) -> Result<(), StoreError> {
    let (files, dir_errors) = pdf::list_files_checked(dir);
    for (d, e) in dir_errors {
        tracing::warn!("research pdf filings {d}: {e}");
        report.unparsed.push((d, e));
    }
    let listed = path_texts(&files);
    for path in files {
        report.scanned += 1;
        let Some(Loaded {
            path: path_str,
            bytes,
            hash,
        }) = load_changed(
            store,
            &path,
            started,
            force,
            known,
            cache.as_deref_mut(),
            report,
        )?
        else {
            continue;
        };
        match pdf::parse_pdf_filing_bytes(&bytes, &path, hash.clone()) {
            Ok(p) => {
                if let Some(existing) = store.filing(&p.filing.id)? {
                    if existing.filing.source_kind == SourceKind::Xbrl {
                        tracing::info!(
                            "research pdf filings {path_str}: {} exists as XBRL, which wins",
                            p.filing.id
                        );
                        // Not an error: a stale error row of this file would only mislead.
                        store.clear_ingest_error(&path_str)?;
                        report.pdf_superseded.push(p.filing.id);
                        continue;
                    }
                    if let Some(msg) = duplicate_of(
                        "research pdf filings",
                        &existing.filing,
                        &path_str,
                        &hash,
                        &listed,
                    ) {
                        tracing::warn!("research pdf filings {path_str}: {msg}");
                        store.record_ingest_error(&path_str, &hash, &msg, started)?;
                        report.unparsed.push((path_str, msg));
                        continue;
                    }
                }
                store.upsert_filing(&p.filing, &p.facts, started)?;
                tickers.insert(p.filing.ticker.clone());
                report.ingested.push(p.filing.id);
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!("research pdf filings {path_str}: {msg}");
                store.record_ingest_error(&path_str, &hash, &msg, started)?;
                report.unparsed.push((path_str, msg));
            }
        }
    }
    Ok(())
}
