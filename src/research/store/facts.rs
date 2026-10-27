//! Repositories for the canon fact tables (research plan §4.1): `filings`, `facts`,
//! `derived`, `restatements`, `ingest_errors`, plus the share-count readers. All dates are
//! ISO `YYYY-MM-DD` text and timestamps RFC 3339 text, as everywhere else in the store.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension};

use super::{Result, Store};
use crate::research::derive::{self, Cell, CellInputs, FactIn, Flag, Restatement, RULES_VERSION};
use crate::research::shares;
use crate::research::xbrl::{AvailableSource, Cite, CtxKind, Fact, Filing, Period, SourceKind};

/// Ticker-years whose FY of a non-per-share element is a recorded restatement: a Q4 cell
/// flagged `restated` with a restatements row for the calendar year that the reconciliation
/// check did not fail. The last condition makes this set and `reconcile_failed` a partition
/// of the same cells whatever flags the YTD Q3 input adds to the Q4 cell (a FY restated while
/// Q3 is a sign conflict is explained; a FY that is itself a sign conflict stays a failure).
/// Counted per ticker-year, not per element, so it is not comparable with the failure count.
/// Shared with the health entry so both read the same set.
pub(crate) const RESTATED_YEARS_SQL: &str = "SELECT DISTINCT d.ticker, d.fy FROM derived d
     WHERE d.q = 4 AND instr(',' || d.flags || ',', ',restated,') > 0
       AND instr(',' || d.flags || ',', ',reconcile_failed,') = 0
       AND d.element NOT LIKE '%PerShare%'
       AND EXISTS (SELECT 1 FROM restatements r
                   WHERE r.ticker = d.ticker AND r.element = d.element AND r.dims = ''
                     AND r.start = d.fy || '-01-01' AND r.end_ = d.fy || '-12-31')
     ORDER BY d.ticker, d.fy";

/// One `filings` row with its fact count.
#[derive(Clone, Debug, PartialEq)]
pub struct FilingRow {
    pub filing: Filing,
    pub ingested_at: String,
    pub fact_count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactRow {
    pub filing_id: String,
    pub element: String,
    pub ctx_kind: CtxKind,
    pub start: Option<String>,
    pub end: String,
    pub dims: String,
    pub value: f64,
    pub unit: String,
    pub decimals: Option<i32>,
    /// PDF page and printed label; `None` for an XBRL fact.
    pub cite: Option<Cite>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedRow {
    pub ticker: String,
    pub element: String,
    pub fy: i32,
    pub q: u32,
    pub standalone: Option<f64>,
    pub ttm: Option<f64>,
    pub annual: Option<f64>,
    pub flags: BTreeSet<Flag>,
    pub inputs: CellInputs,
    pub rules_version: String,
    pub computed_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestatementRow {
    pub ticker: String,
    pub element: String,
    pub start: Option<String>,
    pub end: String,
    pub dims: String,
    pub original: f64,
    pub original_filing: String,
    pub restated: f64,
    pub restated_filing: String,
    pub detected_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IngestErrorRow {
    pub source_path: String,
    pub file_hash: String,
    pub error: String,
    pub at: String,
}

fn date(s: &str) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn period(s: &str) -> rusqlite::Result<Period> {
    Period::parse(s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("bad period {s}").into(),
        )
    })
}

const FILING_COLUMNS: &str = "f.id, f.ticker, f.fy, f.period, f.taxonomy, f.industry_code, f.period_start, f.period_end,
    f.audited, f.rounding, f.currency, f.filed_at, f.available_from, f.available_source, f.source_path, f.file_hash,
    f.ingested_at, (SELECT COUNT(*) FROM facts x WHERE x.filing_id = f.id), f.source_kind";

fn filing_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FilingRow> {
    let available_source: String = r.get(13)?;
    Ok(FilingRow {
        filing: Filing {
            id: r.get(0)?,
            ticker: r.get(1)?,
            fy: r.get(2)?,
            period: period(&r.get::<_, String>(3)?)?,
            taxonomy: r.get(4)?,
            industry_code: r.get(5)?,
            period_start: date(&r.get::<_, String>(6)?)?,
            period_end: date(&r.get::<_, String>(7)?)?,
            audited: r.get::<_, i64>(8)? != 0,
            rounding: r.get(9)?,
            currency: r.get(10)?,
            filed_at: r
                .get::<_, Option<String>>(11)?
                .as_deref()
                .map(date)
                .transpose()?,
            available_from: date(&r.get::<_, String>(12)?)?,
            available_source: if available_source == "declared" {
                AvailableSource::Declared
            } else {
                AvailableSource::Estimated
            },
            source_path: r.get(14)?,
            file_hash: r.get(15)?,
            source_kind: {
                // Never a silent default: a PDF filing must not read as XBRL.
                let kind: String = r.get(18)?;
                SourceKind::parse(&kind).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        18,
                        rusqlite::types::Type::Text,
                        format!("bad source_kind {kind}").into(),
                    )
                })?
            },
        },
        ingested_at: r.get(16)?,
        fact_count: r.get(17)?,
    })
}

fn fact_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FactRow> {
    Ok(FactRow {
        filing_id: r.get(0)?,
        element: r.get(1)?,
        ctx_kind: CtxKind::parse(&r.get::<_, String>(2)?).unwrap_or(CtxKind::Instant),
        start: r.get(3)?,
        end: r.get(4)?,
        dims: r.get(5)?,
        value: r.get(6)?,
        unit: r.get(7)?,
        decimals: r.get(8)?,
        cite: r
            .get::<_, Option<u32>>(9)?
            .map(|page| {
                Ok::<_, rusqlite::Error>(Cite {
                    page,
                    label: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                })
            })
            .transpose()?,
    })
}

const FACT_COLUMNS: &str =
    "filing_id, element, ctx_kind, start, end_, dims, value, unit, decimals, page, label";

impl Store {
    // ---- filings and facts ----

    /// Stores one parsed filing and all of its facts, replacing any earlier version of the
    /// same filing id, and clears a previous ingest error for the file.
    pub fn upsert_filing(&self, filing: &Filing, facts: &[Fact], now: &str) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM facts WHERE filing_id = ?1", [&filing.id])?;
        tx.execute(
            "INSERT OR REPLACE INTO filings (id, ticker, fy, period, taxonomy, industry_code, period_start, period_end,
               audited, rounding, currency, filed_at, available_from, available_source, source_path, file_hash, ingested_at,
               source_kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                filing.id,
                filing.ticker,
                filing.fy,
                filing.period.as_str(),
                filing.taxonomy,
                filing.industry_code,
                filing.period_start.to_string(),
                filing.period_end.to_string(),
                filing.audited as i64,
                filing.rounding,
                filing.currency,
                filing.filed_at.map(|d| d.to_string()),
                filing.available_from.to_string(),
                filing.available_source.as_str(),
                filing.source_path,
                filing.file_hash,
                now,
                filing.source_kind.as_str(),
            ],
        )?;
        {
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO facts (filing_id, element, ctx_kind, start, end_, dims, value, unit, decimals, page, label)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for f in facts {
                st.execute(params![
                    filing.id,
                    f.element,
                    f.ctx_kind.as_str(),
                    f.start,
                    f.end,
                    f.dims,
                    f.value,
                    f.unit,
                    f.decimals,
                    f.cite.as_ref().map(|c| c.page),
                    f.cite.as_ref().map(|c| c.label.as_str()),
                ])?;
            }
        }
        tx.execute(
            "DELETE FROM ingest_errors WHERE source_path = ?1",
            [&filing.source_path],
        )?;
        Ok(tx.commit()?)
    }

    pub fn record_ingest_error(
        &self,
        source_path: &str,
        file_hash: &str,
        error: &str,
        now: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO ingest_errors (source_path, file_hash, error, at) VALUES (?1, ?2, ?3, ?4)",
            params![source_path, file_hash, error, now],
        )?;
        Ok(())
    }

    /// Forgets the ingest error of one file (it was read, and is not an error after all).
    pub fn clear_ingest_error(&self, source_path: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM ingest_errors WHERE source_path = ?1",
            [source_path],
        )?;
        Ok(())
    }

    pub fn ingest_errors(&self) -> Result<Vec<IngestErrorRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT source_path, file_hash, error, at FROM ingest_errors ORDER BY source_path",
        )?;
        let rows = st.query_map([], |r| {
            Ok(IngestErrorRow {
                source_path: r.get(0)?,
                file_hash: r.get(1)?,
                error: r.get(2)?,
                at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// `source_path → file_hash` of every file already ingested or recorded as unreadable.
    pub fn known_file_hashes(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn();
        let mut out = HashMap::new();
        for sql in [
            "SELECT source_path, file_hash FROM filings",
            "SELECT source_path, file_hash FROM ingest_errors",
        ] {
            let mut st = conn.prepare(sql)?;
            let rows =
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (p, h) = row?;
                out.insert(p, h);
            }
        }
        Ok(out)
    }

    /// Filings of one ticker (or all), newest period first.
    pub fn filings(&self, ticker: Option<&str>) -> Result<Vec<FilingRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {FILING_COLUMNS} FROM filings f {} ORDER BY f.ticker, f.fy DESC,
               CASE f.period WHEN 'FY' THEN 4 WHEN 'Q3' THEN 3 WHEN 'Q2' THEN 2 ELSE 1 END DESC",
            if ticker.is_some() {
                "WHERE f.ticker = ?1"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let rows = match ticker {
            Some(t) => st.query_map([t], filing_row)?,
            None => st.query_map([], filing_row)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn filing(&self, id: &str) -> Result<Option<FilingRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {FILING_COLUMNS} FROM filings f WHERE f.id = ?1"),
                [id],
                filing_row,
            )
            .optional()?)
    }

    /// How many filings were read from an original PDF.
    pub fn pdf_filing_count(&self) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM filings WHERE source_kind = 'pdf'",
            [],
            |r| r.get(0),
        )?)
    }

    /// The PDF page and printed label of every fact of `ticker` that was read from a PDF
    /// filing, keyed like the derivation names its inputs.
    pub fn pdf_cites(&self, ticker: &str) -> Result<HashMap<derive::FactRef, Cite>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT x.filing_id, x.element, x.ctx_kind, x.start, x.end_, x.dims, x.page, x.label
             FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND f.source_kind = 'pdf' AND x.page IS NOT NULL",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok((
                derive::FactRef {
                    filing_id: r.get(0)?,
                    element: r.get(1)?,
                    ctx_kind: r.get(2)?,
                    start: r.get(3)?,
                    end: r.get(4)?,
                    dims: r.get(5)?,
                },
                Cite {
                    page: r.get(6)?,
                    label: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                },
            ))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn filing_count(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM filings", [], |r| r.get(0))?)
    }

    pub fn fact_count(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM facts", [], |r| r.get(0))?)
    }

    /// The unit each undimensioned element of `ticker` is reported in (first unit seen).
    pub fn element_units(&self, ticker: &str) -> Result<HashMap<String, String>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT x.element, MIN(x.unit) FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND x.dims = '' GROUP BY x.element",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Subindustry code of each ticker's latest filing (pack selection, A2.4/A2.5).
    pub fn latest_industry_codes(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT f.ticker, f.industry_code FROM filings f
             WHERE f.industry_code IS NOT NULL AND f.period_end = (
               SELECT MAX(g.period_end) FROM filings g WHERE g.ticker = f.ticker AND g.industry_code IS NOT NULL)
             ORDER BY f.ticker, f.period DESC",
        )?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (t, c) = row?;
            out.entry(t).or_insert(c);
        }
        Ok(out)
    }

    /// `(end date, value)` of every instant fact of `ticker` for one element and canonical
    /// dims, the original filing of each date first (so comparatives never shadow it).
    pub fn dimensioned_instants(
        &self,
        ticker: &str,
        element: &str,
        dims: &str,
    ) -> Result<Vec<(NaiveDate, f64)>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT x.end_, x.value FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND x.element = ?2 AND x.dims = ?3 AND x.ctx_kind = 'instant'
             ORDER BY x.end_, f.fy, CASE f.period WHEN 'Q1' THEN 1 WHEN 'Q2' THEN 2 WHEN 'Q3' THEN 3 ELSE 4 END",
        )?;
        let rows = st.query_map(params![ticker, element, dims], |r| {
            Ok((date(&r.get::<_, String>(0)?)?, r.get::<_, f64>(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Tickers that have at least one filing.
    pub fn filing_tickers(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare("SELECT DISTINCT ticker FROM filings ORDER BY ticker")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One fact by its key.
    pub fn fact(
        &self,
        filing_id: &str,
        element: &str,
        ctx_kind: CtxKind,
        start: Option<&str>,
        end: &str,
        dims: &str,
    ) -> Result<Option<FactRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {FACT_COLUMNS} FROM facts WHERE filing_id = ?1 AND element = ?2 AND ctx_kind = ?3
                     AND start IS ?4 AND end_ = ?5 AND dims = ?6"
                ),
                params![filing_id, element, ctx_kind.as_str(), start, end, dims],
                fact_row,
            )
            .optional()?)
    }

    /// Facts of one filing, optionally one element only, in element order.
    pub fn facts_of_filing(&self, filing_id: &str, element: Option<&str>) -> Result<Vec<FactRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {FACT_COLUMNS} FROM facts WHERE filing_id = ?1 {} ORDER BY element, ctx_kind, end_, start, dims",
            if element.is_some() {
                "AND element = ?2"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let rows = match element {
            Some(e) => st.query_map(params![filing_id, e], fact_row)?,
            None => st.query_map([filing_id], fact_row)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The same (element, period, dims) as every filing of `ticker` reports it, in filing order.
    pub fn fact_history(
        &self,
        ticker: &str,
        element: &str,
        ctx_kind: CtxKind,
        start: Option<&str>,
        end: &str,
        dims: &str,
    ) -> Result<Vec<FactRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {FACT_COLUMNS} FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND x.element = ?2 AND x.ctx_kind = ?3 AND x.start IS ?4 AND x.end_ = ?5 AND x.dims = ?6
             ORDER BY f.fy, CASE f.period WHEN 'FY' THEN 4 WHEN 'Q3' THEN 3 WHEN 'Q2' THEN 2 ELSE 1 END"
        ))?;
        let rows = st.query_map(
            params![ticker, element, ctx_kind.as_str(), start, end, dims],
            fact_row,
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Every undimensioned fact of `ticker` with its filing, as the derivation wants it.
    pub fn facts_for_derive(&self, ticker: &str) -> Result<Vec<FactIn>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT x.filing_id, f.fy, f.period, x.element, x.ctx_kind, x.start, x.end_, x.dims, x.value, x.decimals
             FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND x.dims = ''",
        )?;
        let rows = st.query_map([ticker], |r| {
            let start: Option<String> = r.get(5)?;
            Ok(FactIn {
                filing_id: r.get(0)?,
                filing_fy: r.get(1)?,
                filing_q: period(&r.get::<_, String>(2)?)?.quarter(),
                element: r.get(3)?,
                ctx_kind: CtxKind::parse(&r.get::<_, String>(4)?).unwrap_or(CtxKind::Instant),
                start: start.as_deref().map(date).transpose()?,
                end: date(&r.get::<_, String>(6)?)?,
                dims: r.get(7)?,
                value: r.get(8)?,
                decimals: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- derived ----

    /// Replaces every derived cell and restatement of `ticker`.
    pub fn replace_derived(
        &self,
        ticker: &str,
        cells: &[Cell],
        restatements: &[Restatement],
        now: &str,
    ) -> Result<()> {
        self.bump();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM derived WHERE ticker = ?1", [ticker])?;
        tx.execute("DELETE FROM restatements WHERE ticker = ?1", [ticker])?;
        {
            let mut st = tx.prepare(
                "INSERT INTO derived (ticker, element, fy, q, standalone, ttm, annual, flags, inputs, rules_version, computed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for c in cells {
                st.execute(params![
                    ticker,
                    c.element,
                    c.fy,
                    c.q,
                    c.standalone,
                    c.ttm,
                    c.annual,
                    derive::flags_to_string(&c.flags),
                    serde_json::to_string(&c.inputs)?,
                    RULES_VERSION,
                    now,
                ])?;
            }
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO restatements (ticker, element, start, end_, dims, original, original_filing, restated, restated_filing, detected_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for r in restatements {
                st.execute(params![
                    ticker,
                    r.element,
                    r.start.map(|d| d.to_string()),
                    r.end.to_string(),
                    r.dims,
                    r.original,
                    r.original_filing,
                    r.restated,
                    r.restated_filing,
                    now,
                ])?;
            }
        }
        Ok(tx.commit()?)
    }

    /// Tickers whose derived cells were computed under another rules version than
    /// `RULES_VERSION`, plus tickers that have undimensioned facts (what `facts_for_derive` reads) but no derived
    /// row at all, in order.
    pub fn tickers_with_stale_rules(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker FROM derived WHERE rules_version != ?1
             UNION
             SELECT f.ticker FROM filings f
             WHERE EXISTS (SELECT 1 FROM facts x WHERE x.filing_id = f.id AND x.dims = '')
               AND NOT EXISTS (SELECT 1 FROM derived d WHERE d.ticker = f.ticker)
             ORDER BY 1",
        )?;
        let rows = st.query_map([RULES_VERSION], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Derived cells of `ticker`, optionally one element, oldest period first.
    pub fn derived(&self, ticker: &str, element: Option<&str>) -> Result<Vec<DerivedRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT ticker, element, fy, q, standalone, ttm, annual, flags, inputs, rules_version, computed_at
             FROM derived WHERE ticker = ?1 {} ORDER BY element, fy, q",
            if element.is_some() {
                "AND element = ?2"
            } else {
                ""
            }
        );
        let mut st = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<DerivedRow> {
            let inputs: String = r.get(8)?;
            Ok(DerivedRow {
                ticker: r.get(0)?,
                element: r.get(1)?,
                fy: r.get(2)?,
                q: r.get(3)?,
                standalone: r.get(4)?,
                ttm: r.get(5)?,
                annual: r.get(6)?,
                flags: derive::flags_from_string(&r.get::<_, String>(7)?),
                inputs: serde_json::from_str(&inputs).unwrap_or_default(),
                rules_version: r.get(9)?,
                computed_at: r.get(10)?,
            })
        };
        let rows = match element {
            Some(e) => st.query_map(params![ticker, e], map)?,
            None => st.query_map([ticker], map)?,
        };
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Restatements of one (element, period, dims) of `ticker`.
    pub fn restatements(
        &self,
        ticker: &str,
        element: &str,
        start: Option<&str>,
        end: &str,
        dims: &str,
    ) -> Result<Vec<RestatementRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, element, start, end_, dims, original, original_filing, restated, restated_filing, detected_at
             FROM restatements WHERE ticker = ?1 AND element = ?2 AND start IS ?3 AND end_ = ?4 AND dims = ?5
             ORDER BY restated_filing",
        )?;
        let rows = st.query_map(params![ticker, element, start, end, dims], |r| {
            Ok(RestatementRow {
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

    /// `(ticker, fy)` of every year with a restated FY figure, in order: the years the
    /// reconciliation check leaves to the `restated` flag and the restatements table.
    pub fn restated_years(&self) -> Result<Vec<(String, i32)>> {
        let conn = self.conn();
        let mut st = conn.prepare(RESTATED_YEARS_SQL)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// `(ticker, element, fy)` of every year whose quarters do not reconcile with the FY and
    /// that no recorded restatement of the FY explains.
    pub fn reconciliation_failures(&self) -> Result<Vec<(String, String, i32)>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT ticker, element, fy FROM derived WHERE q = 4 AND (',' || flags || ',') LIKE '%,reconcile_failed,%'
             ORDER BY ticker, element, fy",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- shares ----

    /// Share-count facts of `ticker` (unit `shares` or a known element), any dimension-free context.
    pub fn share_facts(&self, ticker: &str) -> Result<Vec<shares::ShareFact>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT x.filing_id, x.element, x.ctx_kind, x.end_, x.value, x.unit
             FROM facts x JOIN filings f ON f.id = x.filing_id
             WHERE f.ticker = ?1 AND x.dims = '' AND (x.unit = 'shares' OR x.element IN (SELECT value FROM json_each(?2)))",
        )?;
        let elements = serde_json::to_string(shares::SHARE_COUNT_ELEMENTS)?;
        let rows = st.query_map(params![ticker, elements], |r| {
            let kind: String = r.get(2)?;
            Ok(shares::ShareFact {
                filing_id: r.get(0)?,
                element: r.get(1)?,
                kind: if kind == "duration" {
                    shares::Kind::Duration
                } else {
                    shares::Kind::Instant
                },
                end: date(&r.get::<_, String>(3)?)?,
                value: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Net income and basic EPS of the same dimension-free context per filing of `ticker`
    /// (D13 option c, R3.2): every numerator × EPS element pair found; `shares::implied_from_eps`
    /// picks one per period.
    pub fn eps_pairs(&self, ticker: &str) -> Result<Vec<shares::EpsPair>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT n.filing_id, n.end_, n.element, n.value, e.element, e.value
             FROM facts n
             JOIN facts e ON e.filing_id = n.filing_id AND e.ctx_kind = n.ctx_kind
                         AND e.start IS n.start AND e.end_ = n.end_ AND e.dims = ''
             JOIN filings f ON f.id = n.filing_id
             WHERE f.ticker = ?1 AND n.dims = '' AND n.ctx_kind = 'duration'
               AND n.element IN (SELECT value FROM json_each(?2))
               AND e.element IN (SELECT value FROM json_each(?3))
             ORDER BY n.end_, n.filing_id",
        )?;
        let nums = serde_json::to_string(shares::EPS_NUMERATOR_ELEMENTS)?;
        let eps = serde_json::to_string(shares::EPS_ELEMENTS)?;
        let rows = st.query_map(params![ticker, nums, eps], |r| {
            Ok(shares::EpsPair {
                filing_id: r.get(0)?,
                period_end: date(&r.get::<_, String>(1)?)?,
                numerator_element: r.get(2)?,
                net_income: r.get(3)?,
                eps_element: r.get(4)?,
                eps: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The full point-in-time input set of `ticker` for the valuation history (R3.2).
    pub fn share_series(&self, ticker: &str) -> Result<shares::ShareSeries> {
        Ok(shares::ShareSeries {
            facts: self.share_facts(ticker)?,
            manual: self.shares_manual_all(ticker)?,
            derived: shares::implied_from_eps(&self.eps_pairs(ticker)?),
        })
    }

    /// Every manual share count of `ticker`, oldest first.
    pub fn shares_manual_all(&self, ticker: &str) -> Result<Vec<shares::ManualShares>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT as_of, shares, source_note FROM shares_manual WHERE ticker = ?1 ORDER BY as_of",
        )?;
        let rows = st.query_map([ticker], |r| {
            Ok(shares::ManualShares {
                as_of: date(&r.get::<_, String>(0)?)?,
                shares: r.get(1)?,
                source_note: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}
