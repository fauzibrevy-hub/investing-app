//! Original filings read from a PDF (owner decision after the BUMI 2024 Q1 gap): IDX has no
//! XBRL instance for some interim reports, but the owner has the PDF. The app stays
//! model-free: the reading happens outside it, and the app only ingests a JSON file that was
//! checked against the PDF. One file per filing, `<TICKER>_<FY>_<PERIOD>.json`:
//!
//! ```json
//! { "format": "pdf-filing/1", "ticker": "BUMI", "fy": 2024, "period": "Q1",
//!   "period_start": "2024-01-01", "period_end": "2024-03-31", "currency": "USD",
//!   "rounding": "Satuan Penuh", "audited": false, "taxonomy": "general",
//!   "industry_code": "A121", "document_date": "2024-04-23",
//!   "source": { "file": "BUMI_2024_Q1.pdf", "sha256": "…" },
//!   "facts": [ { "element": "idx-cor:SalesAndRevenue", "ctx_kind": "duration",
//!                "start": "2024-01-01", "end": "2024-03-31", "value": 311015873,
//!                "page": 7, "label": "Pendapatan / Revenues" } ] }
//! ```
//!
//! Values are in full currency units (a filing printed in thousands is typed times 1000, and
//! a value that is not a multiple of the rounding is refused), signs as the XBRL convention of that element (the
//! packs read the same names from the XBRL filings). Most facts are one printed line. Where
//! IDX's tag is not one line (profit before tax after profit sharing, bank loans without the
//! non-bank lenders) the fact lists `components`, each a printed line with its own page and
//! value, and `value` must be their sum; an optional `note` says why (audit text, not stored). Only current-period contexts: a fact's
//! dates must be the filing's own period, so a comparative column cannot get in. Every fact
//! names the 1-based PDF page it was read from. The file is a trust boundary: anything the
//! app cannot place is an error for the whole file (unknown element, no page, a value that
//! is not a finite number, dates off the period, a repeated fact), never a silent skip.
//!
//! A PDF filing is a filing like any other for the derivation (`SourceKind::Pdf` only
//! changes what the screens say about it); an XBRL filing of the same id always wins
//! (`ingest_pdf_files` in the parent module).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::NaiveDate;
use serde::Deserialize;

use super::dei::{AvailableSource, Period};
use super::parse::{Cite, CtxKind, Fact};
use super::ratio::PERCENT_RATIOS;
use super::{filing_id, sha256_hex, Filing, Parsed, SourceKind};
use crate::research::packs::builtin::embedded_packs;
use crate::research::packs::Binding;

/// The only `format` this reader accepts.
pub const FORMAT: &str = "pdf-filing/1";

/// Errors listed per file before the rest are only counted.
const MAX_LISTED: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum PdfFilingError {
    #[error("read: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a pdf-filing JSON file: {0}")]
    Json(String),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDoc {
    format: String,
    ticker: String,
    fy: i32,
    period: String,
    period_start: String,
    period_end: String,
    currency: String,
    rounding: String,
    audited: bool,
    taxonomy: String,
    #[serde(default)]
    industry_code: Option<String>,
    document_date: String,
    source: SourceDoc,
    facts: Vec<FactDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDoc {
    file: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactDoc {
    element: String,
    ctx_kind: String,
    #[serde(default)]
    start: Option<String>,
    end: String,
    /// A number; `null` or a missing key is reported, not defaulted.
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    label: Option<String>,
    /// Printed lines whose sum is `value`; empty when `value` is one printed line.
    #[serde(default)]
    components: Vec<ComponentDoc>,
    /// Why the fact is a sum; for the reader of the file, not read by the app.
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentDoc {
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    value: Option<f64>,
}

/// Elements the packs bind (the embedded canon packs): the only ones the app reads from a
/// filing, so the only ones a PDF file may carry.
fn known_elements() -> &'static BTreeSet<String> {
    static KNOWN: OnceLock<BTreeSet<String>> = OnceLock::new();
    KNOWN.get_or_init(|| {
        let mut out = BTreeSet::new();
        for pack in embedded_packs() {
            for metric in pack.metrics() {
                let Some(formula) = &metric.formula else {
                    continue;
                };
                for alt in formula.bindings.values().flatten() {
                    if let Binding::Element { element, .. } = alt {
                        out.insert(element.clone());
                    }
                }
            }
        }
        out
    })
}

fn date(field: &str, v: &str, errors: &mut Vec<String>) -> Option<NaiveDate> {
    match NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d") {
        Ok(d) => Some(d),
        Err(_) => {
            errors.push(format!(
                "{field}: `{}` is not a YYYY-MM-DD date",
                v.escape_debug()
            ));
            None
        }
    }
}

/// `decimals` of a declared rounding level (`Satuan Penuh`, `Ribuan`, `Jutaan`).
fn decimals_of(rounding: &str) -> Option<i32> {
    let r = rounding.to_ascii_lowercase();
    if r.contains("penuh") || r.contains("full") {
        Some(0)
    } else if r.contains("ribu") || r.contains("thousand") {
        Some(-3)
    } else if r.contains("juta") || r.contains("million") {
        Some(-6)
    } else {
        None
    }
}

fn quarter_end(fy: i32, period: Period) -> Option<NaiveDate> {
    let (m, d) = match period {
        Period::Q1 => (3, 31),
        Period::Q2 => (6, 30),
        Period::Q3 => (9, 30),
        Period::FY => (12, 31),
    };
    NaiveDate::from_ymd_opt(fy, m, d)
}

/// Reads and validates one PDF-filing JSON file.
pub fn parse_pdf_filing(path: &Path) -> Result<Parsed, PdfFilingError> {
    let bytes = std::fs::read(path)?;
    let hash = sha256_hex(&bytes);
    parse_pdf_filing_bytes(&bytes, path, hash)
}

/// Validates a file already in memory.
pub fn parse_pdf_filing_bytes(
    bytes: &[u8],
    source_path: &Path,
    file_hash: String,
) -> Result<Parsed, PdfFilingError> {
    let doc: FileDoc =
        serde_json::from_slice(bytes).map_err(|e| PdfFilingError::Json(e.to_string()))?;
    let mut errors: Vec<String> = Vec::new();

    if doc.format != FORMAT {
        errors.push(format!(
            "format: `{}` is not `{FORMAT}`",
            doc.format.escape_debug()
        ));
    }
    let ticker = doc.ticker.trim().to_string();
    if ticker.len() != 4 || !ticker.bytes().all(|b| b.is_ascii_uppercase()) {
        errors.push(format!(
            "ticker: `{}` is not a four-letter ticker",
            ticker.escape_debug()
        ));
    }
    let period = Period::parse(&doc.period);
    if period.is_none() {
        errors.push(format!(
            "period: `{}` is not Q1, Q2, Q3 or FY",
            doc.period.escape_debug()
        ));
    }
    let start = date("period_start", &doc.period_start, &mut errors);
    let end = date("period_end", &doc.period_end, &mut errors);
    let document_date = date("document_date", &doc.document_date, &mut errors);
    if let (Some(period), Some(start), Some(end)) = (period, start, end) {
        // The derivation reads calendar years to date; a filing that is not one would only
        // ever show as gaps, so it is refused here instead of ingested as a dead filing.
        if NaiveDate::from_ymd_opt(doc.fy, 1, 1) != Some(start) {
            errors.push(format!(
                "period_start {start} is not 1 January of fy {}",
                doc.fy
            ));
        }
        if quarter_end(doc.fy, period) != Some(end) {
            errors.push(format!(
                "period_end {end} is not the end of {} {}",
                doc.fy,
                period.as_str()
            ));
        }
    }
    if let (Some(d), Some(end)) = (document_date, end) {
        if d < end {
            errors.push(format!("document_date {d} is before the period end {end}"));
        }
    }
    let currency = doc.currency.trim().to_string();
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        errors.push(format!(
            "currency: `{}` is not a three-letter code",
            currency.escape_debug()
        ));
    }
    let decimals = decimals_of(&doc.rounding);
    if decimals.is_none() {
        errors.push(format!(
            "rounding: `{}` is not Satuan Penuh, Ribuan or Jutaan",
            doc.rounding.escape_debug()
        ));
    }
    if doc.taxonomy.trim().is_empty() {
        errors.push("taxonomy is empty".into());
    }
    if doc.source.file.trim().is_empty() {
        errors.push("source.file is empty".into());
    }
    let sha = &doc.source.sha256;
    if sha.len() != 64 || !sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        errors.push("source.sha256 is not 64 lowercase hex digits".into());
    }
    if doc.facts.is_empty() {
        errors.push("facts is empty".into());
    }

    let known = known_elements();
    let mut seen: BTreeSet<(String, CtxKind, Option<String>, String)> = BTreeSet::new();
    let mut facts = Vec::with_capacity(doc.facts.len());
    for (i, f) in doc.facts.iter().enumerate() {
        let at = format!("facts[{i}] {}", f.element.escape_debug());
        let before = errors.len();
        if !known.contains(&f.element) {
            errors.push(format!(
                "{at}: unknown element (the app reads only the {} elements its packs bind)",
                known.len()
            ));
        } else if PERCENT_RATIOS.contains(&f.element.as_str()) || f.element.contains("PerShare") {
            errors.push(format!("{at}: not a monetary element"));
        }
        let kind = CtxKind::parse(&f.ctx_kind);
        if kind.is_none() {
            errors.push(format!(
                "{at}: ctx_kind `{}` is not duration or instant",
                f.ctx_kind.escape_debug()
            ));
        }
        let value = match f.value {
            Some(v) if v.is_finite() => Some(v),
            Some(_) => {
                errors.push(format!("{at}: value is not a finite number"));
                None
            }
            None => {
                errors.push(format!("{at}: value is missing"));
                None
            }
        };
        let page = match f.page {
            Some(p) if p >= 1 => match u32::try_from(p) {
                Ok(page) => Some(page),
                Err(_) => {
                    errors.push(format!("{at}: page {p} is too large for a PDF page"));
                    None
                }
            },
            Some(p) => {
                errors.push(format!("{at}: page {p} is not a PDF page (1 or more)"));
                None
            }
            None => {
                errors.push(format!("{at}: page is missing"));
                None
            }
        };
        // Full currency units: when the filing rounds to thousands or millions, a value typed
        // as printed (311,015 for 311,015,000) is not a multiple of the rounding and is refused.
        if let (Some(v), Some(d)) = (value, decimals) {
            let unit = 10f64.powi(-d.min(0));
            if d < 0 && (v / unit - (v / unit).round()).abs() > 1e-6 {
                errors.push(format!(
                    "{at}: value {v} is not a multiple of {unit}; values are in full {currency}, not as printed in {}",
                    doc.rounding.trim().escape_debug()
                ));
            }
        }
        let label = f.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
        if label.is_none() {
            errors.push(format!("{at}: label as printed is missing"));
        }
        if !f.components.is_empty() {
            let mut sum = 0.0;
            let mut complete = true;
            for (j, c) in f.components.iter().enumerate() {
                let cat = format!("{at} components[{j}]");
                match c.value {
                    Some(v) if v.is_finite() => sum += v,
                    _ => {
                        errors.push(format!("{cat}: value is missing or not a finite number"));
                        complete = false;
                    }
                }
                if c.page.is_none_or(|p| p < 1) {
                    errors.push(format!(
                        "{cat}: page is missing or not a PDF page (1 or more)"
                    ));
                }
                if c.label.as_deref().is_none_or(|l| l.trim().is_empty()) {
                    errors.push(format!("{cat}: label as printed is missing"));
                }
            }
            if let (true, Some(v), Some(d)) = (complete, value, decimals) {
                if (sum - v).abs() > 0.5 * 10f64.powi(-d) {
                    errors.push(format!("{at}: components add up to {sum}, not {v}"));
                }
            }
        }
        // Current period only: the dates are the filing's own, so a comparative cannot pass.
        let fact_end = date(&format!("{at}: end"), &f.end, &mut errors);
        let fact_start = f
            .start
            .as_deref()
            .and_then(|s| date(&format!("{at}: start"), s, &mut errors));
        match kind {
            Some(CtxKind::Duration) => {
                if f.start.is_none() {
                    errors.push(format!("{at}: a duration fact needs start"));
                } else if fact_start != start || fact_end != end {
                    errors.push(format!(
                        "{at}: dates {}..{} are not the filing period {}..{}",
                        f.start.as_deref().unwrap_or("").escape_debug(),
                        f.end.escape_debug(),
                        doc.period_start.escape_debug(),
                        doc.period_end.escape_debug()
                    ));
                }
            }
            Some(CtxKind::Instant) => {
                if f.start.is_some() {
                    errors.push(format!("{at}: an instant fact has no start"));
                } else if fact_end != end {
                    errors.push(format!(
                        "{at}: instant {} is not the period end {}",
                        f.end.escape_debug(),
                        doc.period_end.escape_debug()
                    ));
                }
            }
            None => {}
        }
        if let (Some(kind), true) = (kind, errors.len() == before) {
            // Keyed on the parsed dates, so `2024-3-31` and `2024-03-31` are one period.
            let key = (
                f.element.clone(),
                kind,
                fact_start.map(|d| d.to_string()),
                fact_end.map(|d| d.to_string()).unwrap_or_default(),
            );
            if !seen.insert(key) {
                errors.push(format!("{at}: listed twice for the same period"));
            }
        }
        if let (Some(kind), Some(value), Some(page), Some(label), Some(fact_end), true) =
            (kind, value, page, label, fact_end, errors.len() == before)
        {
            // Stored as the canonical text of the parsed dates: the derivation and the cite
            // lookup compare these strings with `NaiveDate::to_string()`.
            facts.push(Fact {
                element: f.element.clone(),
                ctx_kind: kind,
                start: fact_start.map(|d| d.to_string()),
                end: fact_end.to_string(),
                dims: String::new(),
                value,
                unit: currency.clone(),
                decimals,
                cite: Some(Cite {
                    page,
                    label: label.to_string(),
                }),
            });
        }
    }

    // Every `Option` below was checked while the errors were collected.
    let (true, Some(period), Some(period_start), Some(period_end), Some(document_date)) =
        (errors.is_empty(), period, start, end, document_date)
    else {
        let more = errors.len().saturating_sub(MAX_LISTED);
        let mut text = errors
            .into_iter()
            .take(MAX_LISTED)
            .collect::<Vec<_>>()
            .join("; ");
        if more > 0 {
            text.push_str(&format!("; and {more} more"));
        }
        return Err(PdfFilingError::Invalid(text));
    };
    let filing = Filing {
        id: filing_id(&ticker, doc.fy, period),
        ticker,
        fy: doc.fy,
        period,
        taxonomy: doc.taxonomy.trim().to_string(),
        industry_code: doc
            .industry_code
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty()),
        period_start,
        period_end,
        audited: doc.audited,
        rounding: doc.rounding.trim().to_string(),
        currency,
        filed_at: Some(document_date),
        // The date on the document is the earliest it can have been public.
        available_from: document_date,
        available_source: AvailableSource::Declared,
        source_path: source_path.to_string_lossy().into_owned(),
        file_hash,
        source_kind: SourceKind::Pdf,
    };
    Ok(Parsed {
        filing,
        facts,
        entity_name: None,
        skipped_non_numeric: 0,
        skipped_nil: 0,
        conflicts: 0,
    })
}

/// Every `.json` file under `dir`, recursively, in path order, plus `(directory, error)` for
/// a directory that could not be listed. Symbolic links are not followed.
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
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            let p = e.path();
            if kind.is_dir() {
                stack.push(p);
            } else if kind.is_file()
                && p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("json"))
            {
                files.push(p);
            }
        }
    }
    files.sort();
    (files, errors)
}
