//! `idx-dei` header → filing metadata (research plan §4.2).
//!
//! IDX writes every header value bilingually as `"Indonesian / English"`; the English half
//! drives the mapping and the Indonesian half is kept where the schema stores the declared
//! text (`rounding`). Nothing here guesses: a missing period end or an unreadable period
//! is an error, and `available_from` says whether it was declared or estimated.

use std::collections::BTreeMap;

use chrono::{Datelike, Days, NaiveDate};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DeiError {
    #[error("idx-dei header has no {0}")]
    Missing(&'static str),
    #[error("idx-dei {field} is not a date: {value}")]
    BadDate { field: &'static str, value: String },
    #[error("idx-dei period is not a quarter or a year: {0}")]
    BadPeriod(String),
    #[error("entity code is not a four-letter ticker: {0}")]
    BadTicker(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Period {
    Q1,
    Q2,
    Q3,
    FY,
}

impl Period {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Q1 => "Q1",
            Self::Q2 => "Q2",
            Self::Q3 => "Q3",
            Self::FY => "FY",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Q1" => Some(Self::Q1),
            "Q2" => Some(Self::Q2),
            "Q3" => Some(Self::Q3),
            "FY" => Some(Self::FY),
            _ => None,
        }
    }

    /// Quarter number 1 to 4 (FY is the fourth).
    pub fn quarter(self) -> u32 {
        match self {
            Self::Q1 => 1,
            Self::Q2 => 2,
            Self::Q3 => 3,
            Self::FY => 4,
        }
    }

    pub fn from_quarter(q: u32) -> Option<Self> {
        match q {
            1 => Some(Self::Q1),
            2 => Some(Self::Q2),
            3 => Some(Self::Q3),
            4 => Some(Self::FY),
            _ => None,
        }
    }

    /// POJK filing deadline after period end: 30 days (Q1, Q3), 60 (Q2), 90 (FY).
    pub fn deadline_days(self) -> u64 {
        match self {
            Self::Q1 | Self::Q3 => 30,
            Self::Q2 => 60,
            Self::FY => 90,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvailableSource {
    Declared,
    Estimated,
}

impl AvailableSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Estimated => "estimated",
        }
    }
}

/// Filing metadata read from the header.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub ticker: String,
    pub entity_name: Option<String>,
    pub fy: i32,
    pub period: Period,
    /// `general`, `infrastructure`, `financial_sharia`, or a slug of the English name.
    pub taxonomy: String,
    /// Subindustry code, e.g. `G111`.
    pub industry_code: Option<String>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub audited: bool,
    /// Declared rounding, Indonesian half: `Jutaan`, `Satuan Penuh`.
    pub rounding: String,
    pub currency: String,
    /// Date of the board of directors' statement letter, when present (not the filing date).
    pub statement_letter_date: Option<NaiveDate>,
}

/// `"Indonesian / English"` → `(indonesian, english)`; a value without `/` is both halves.
pub fn halves(v: &str) -> (String, String) {
    match v.split_once(" / ") {
        Some((id, en)) => (id.trim().to_string(), en.trim().to_string()),
        None => (v.trim().to_string(), v.trim().to_string()),
    }
}

fn date(dei: &BTreeMap<String, String>, field: &'static str) -> Result<NaiveDate, DeiError> {
    let v = dei.get(field).ok_or(DeiError::Missing(field))?;
    NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").map_err(|_| DeiError::BadDate {
        field,
        value: v.clone(),
    })
}

fn opt_date(dei: &BTreeMap<String, String>, field: &str) -> Option<NaiveDate> {
    dei.get(field)
        .and_then(|v| NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok())
}

pub fn taxonomy_of(main_industry: &str) -> String {
    let (_, en) = halves(main_industry);
    let lower = en.to_ascii_lowercase();
    match lower.as_str() {
        "general" => "general".into(),
        "infrastructure" => "infrastructure".into(),
        "financial and sharia" => "financial_sharia".into(),
        _ => {
            let slug: String = lower
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            slug.trim_matches('_')
                .split('_')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("_")
        }
    }
}

fn period_of(dei: &BTreeMap<String, String>, end: NaiveDate) -> Result<Period, DeiError> {
    match dei.get("PeriodOfFinancialStatementsSubmissions") {
        Some(v) => {
            let (id, en) = halves(v);
            let en = en.to_ascii_lowercase();
            let id = id.to_ascii_lowercase();
            if en.starts_with("first") || id == "kuartal i" {
                Ok(Period::Q1)
            } else if en.starts_with("second") || id == "kuartal ii" {
                Ok(Period::Q2)
            } else if en.starts_with("third") || id == "kuartal iii" {
                Ok(Period::Q3)
            } else if en.starts_with("annual") || id == "tahunan" {
                Ok(Period::FY)
            } else {
                Err(DeiError::BadPeriod(v.clone()))
            }
        }
        // Calendar fiscal years (ASUMSI): the period end month names the quarter.
        None => Period::from_quarter(end.month().div_ceil(3))
            .filter(|_| end.month() % 3 == 0)
            .ok_or_else(|| DeiError::BadPeriod(end.to_string())),
    }
}

fn audited_of(v: Option<&String>) -> bool {
    v.is_some_and(|v| {
        let (id, en) = halves(v);
        en.eq_ignore_ascii_case("audited") || id.eq_ignore_ascii_case("diaudit")
    })
}

fn currency_of(v: Option<&String>, unit_fallback: Option<&str>) -> Result<String, DeiError> {
    if let Some(v) = v {
        let (_, en) = halves(v);
        if let Some(code) = en
            .split_whitespace()
            .last()
            .filter(|w| w.len() == 3 && w.bytes().all(|b| b.is_ascii_uppercase()))
        {
            return Ok(code.to_string());
        }
        let upper = v.to_ascii_uppercase();
        for code in ["IDR", "USD"] {
            if upper.contains(code) {
                return Ok(code.into());
            }
        }
    }
    unit_fallback
        .map(str::to_string)
        .ok_or(DeiError::Missing("DescriptionOfPresentationCurrency"))
}

/// Reads the header. `fallback_ticker` is the file name's ticker, used only when the header
/// has no `EntityCode`; `unit_fallback` is the first currency unit of the document.
pub fn header(
    dei: &BTreeMap<String, String>,
    fallback_ticker: Option<&str>,
    unit_fallback: Option<&str>,
) -> Result<Header, DeiError> {
    let raw_ticker = dei
        .get("EntityCode")
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .or_else(|| fallback_ticker.map(|t| t.trim().to_ascii_uppercase()))
        .ok_or(DeiError::Missing("EntityCode"))?;
    if raw_ticker.len() != 4 || !raw_ticker.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(DeiError::BadTicker(raw_ticker));
    }
    let period_end = date(dei, "CurrentPeriodEndDate")?;
    let period_start = date(dei, "CurrentPeriodStartDate")?;
    let period = period_of(dei, period_end)?;
    let taxonomy = dei
        .get("EntityMainIndustry")
        .map(|v| taxonomy_of(v))
        .ok_or(DeiError::Missing("EntityMainIndustry"))?;
    let industry_code = dei.get("Subindustry").and_then(|v| {
        let code = v.split('.').next()?.trim();
        (!code.is_empty() && code.len() <= 8 && code.bytes().all(|b| b.is_ascii_alphanumeric()))
            .then(|| code.to_string())
    });
    let rounding = dei
        .get("LevelOfRoundingUsedInFinancialStatements")
        .map(|v| halves(v).0)
        .filter(|s| !s.is_empty())
        .ok_or(DeiError::Missing(
            "LevelOfRoundingUsedInFinancialStatements",
        ))?;
    Ok(Header {
        ticker: raw_ticker,
        entity_name: dei
            .get("EntityName")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        fy: period_end.year(),
        period,
        taxonomy,
        industry_code,
        period_start,
        period_end,
        audited: audited_of(dei.get("TypeOfReportOnFinancialStatements")),
        rounding,
        currency: currency_of(dei.get("DescriptionOfPresentationCurrency"), unit_fallback)?,
        statement_letter_date: opt_date(dei, "DateOfBoardOfDirectorsStatementLetter"),
    })
}

/// When the filing became public. The scraper's `financial_reports.csv` carries no filing
/// date (checked read-only in R1.2), so until a declared date exists every filing is
/// estimated as period end plus the POJK deadline (plan §4.2).
pub fn available_from(period_end: NaiveDate, period: Period) -> (NaiveDate, AvailableSource) {
    let d = period_end
        .checked_add_days(Days::new(period.deadline_days()))
        .unwrap_or(period_end);
    (d, AvailableSource::Estimated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dei(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn full() -> BTreeMap<String, String> {
        dei(&[
            ("EntityCode", "AAAA"),
            ("EntityName", "PT Contoh Umum Tbk"),
            ("EntityMainIndustry", "Umum / General"),
            (
                "Subindustry",
                "C141. Construction Machinery & Heavy Vehicles",
            ),
            ("CurrentPeriodStartDate", "2025-01-01"),
            ("CurrentPeriodEndDate", "2025-06-30"),
            (
                "PeriodOfFinancialStatementsSubmissions",
                "Kuartal II / Second Quarter",
            ),
            (
                "TypeOfReportOnFinancialStatements",
                "Tidak Diaudit / Unaudit",
            ),
            (
                "LevelOfRoundingUsedInFinancialStatements",
                "Satuan Penuh / Full Amount",
            ),
            ("DescriptionOfPresentationCurrency", "Dollar Amerika / USD"),
            ("DateOfBoardOfDirectorsStatementLetter", "2025-07-28"),
        ])
    }

    #[test]
    fn reads_a_general_interim_header() {
        let h = header(&full(), None, None).unwrap();
        assert_eq!(h.ticker, "AAAA");
        assert_eq!(h.fy, 2025);
        assert_eq!(h.period, Period::Q2);
        assert_eq!(h.taxonomy, "general");
        assert_eq!(h.industry_code.as_deref(), Some("C141"));
        assert!(
            !h.audited,
            "Tidak Diaudit contains the word Diaudit but is unaudited"
        );
        assert_eq!(h.rounding, "Satuan Penuh");
        assert_eq!(h.currency, "USD");
        assert_eq!(h.entity_name.as_deref(), Some("PT Contoh Umum Tbk"));
        assert_eq!(
            h.statement_letter_date,
            Some(NaiveDate::from_ymd_opt(2025, 7, 28).unwrap())
        );
    }

    #[test]
    fn taxonomies_audit_flags_and_periods() {
        let mut d = full();
        d.insert(
            "EntityMainIndustry".into(),
            "Keuangan dan Syariah / Financial and Sharia".into(),
        );
        d.insert(
            "TypeOfReportOnFinancialStatements".into(),
            "Diaudit / Audited".into(),
        );
        d.insert(
            "PeriodOfFinancialStatementsSubmissions".into(),
            "Tahunan / Annual".into(),
        );
        d.insert("CurrentPeriodEndDate".into(), "2024-12-31".into());
        d.insert(
            "LevelOfRoundingUsedInFinancialStatements".into(),
            "Jutaan / In Million".into(),
        );
        d.insert(
            "DescriptionOfPresentationCurrency".into(),
            "Rupiah / IDR".into(),
        );
        let h = header(&d, None, None).unwrap();
        assert_eq!(h.taxonomy, "financial_sharia");
        assert!(h.audited);
        assert_eq!(h.period, Period::FY);
        assert_eq!(h.fy, 2024);
        assert_eq!(h.rounding, "Jutaan");
        assert_eq!(h.currency, "IDR");

        d.insert(
            "TypeOfReportOnFinancialStatements".into(),
            "Ditelaah secara Terbatas / Limited Review".into(),
        );
        assert!(!header(&d, None, None).unwrap().audited);
        assert_eq!(
            taxonomy_of("Infrastruktur / Infrastructure"),
            "infrastructure"
        );
        assert_eq!(taxonomy_of("Lainnya / Some New Thing"), "some_new_thing");
    }

    #[test]
    fn period_falls_back_to_the_end_month_and_rejects_nonsense() {
        let mut d = full();
        d.remove("PeriodOfFinancialStatementsSubmissions");
        d.insert("CurrentPeriodEndDate".into(), "2025-09-30".into());
        assert_eq!(header(&d, None, None).unwrap().period, Period::Q3);
        d.insert("CurrentPeriodEndDate".into(), "2025-08-31".into());
        assert!(matches!(
            header(&d, None, None),
            Err(DeiError::BadPeriod(_))
        ));
        d.insert(
            "PeriodOfFinancialStatementsSubmissions".into(),
            "Bulanan / Monthly".into(),
        );
        assert!(matches!(
            header(&d, None, None),
            Err(DeiError::BadPeriod(_))
        ));
    }

    #[test]
    fn ticker_comes_from_the_header_else_the_file_name() {
        let mut d = full();
        d.remove("EntityCode");
        assert_eq!(header(&d, Some("bbbb"), None).unwrap().ticker, "BBBB");
        assert_eq!(header(&d, None, None), Err(DeiError::Missing("EntityCode")));
        assert_eq!(
            header(&d, Some("TOOLONG"), None),
            Err(DeiError::BadTicker("TOOLONG".into()))
        );
    }

    #[test]
    fn missing_dates_rounding_or_currency_are_errors_not_guesses() {
        let mut d = full();
        d.remove("CurrentPeriodEndDate");
        assert_eq!(
            header(&d, None, None),
            Err(DeiError::Missing("CurrentPeriodEndDate"))
        );
        let mut d = full();
        d.insert("CurrentPeriodStartDate".into(), "01/01/2025".into());
        assert!(matches!(
            header(&d, None, None),
            Err(DeiError::BadDate { .. })
        ));
        let mut d = full();
        d.remove("DescriptionOfPresentationCurrency");
        assert_eq!(header(&d, None, Some("IDR")).unwrap().currency, "IDR");
        assert_eq!(
            header(&d, None, None),
            Err(DeiError::Missing("DescriptionOfPresentationCurrency"))
        );
        let mut d = full();
        d.remove("LevelOfRoundingUsedInFinancialStatements");
        assert!(matches!(header(&d, None, None), Err(DeiError::Missing(_))));
    }

    #[test]
    fn available_from_is_estimated_by_pojk_deadline() {
        let end = NaiveDate::from_ymd_opt(2025, 6, 30).unwrap();
        let (d, s) = available_from(end, Period::Q2);
        assert_eq!(d, NaiveDate::from_ymd_opt(2025, 8, 29).unwrap());
        assert_eq!(s, AvailableSource::Estimated);
        let end = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        assert_eq!(
            available_from(end, Period::FY).0,
            NaiveDate::from_ymd_opt(2025, 3, 31).unwrap()
        );
        let end = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
        assert_eq!(
            available_from(end, Period::Q1).0,
            NaiveDate::from_ymd_opt(2025, 4, 30).unwrap()
        );
    }
}
