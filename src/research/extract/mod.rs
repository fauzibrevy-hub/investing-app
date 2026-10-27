//! Document extraction (research plan §6 R8, prd.md §8 A8.1 to A8.3): upload a PDF or a
//! presentation to a dossier and get a ranked list of KPI proposals with page references.
//! Rule-based only: no model, no network, no paid service, and no host binary; the same file
//! always gives the same proposals. A proposal is stored in `extraction_proposals` and
//! reaches `kpi_values` only when the owner accepts it.
//!
//! Pure parts live here: `doc` turns bytes into text per page, `rules` turns text into
//! proposals, and `check_upload` validates the form fields. Rows and routes are
//! `store/r8.rs` and `api/r8.rs`.

pub mod doc;
pub mod isolated;
pub mod rules;

use chrono::NaiveDate;

use super::documents::{self, FieldError};

/// What an upload form carries besides the file, after validation.
#[derive(Clone, Debug, PartialEq)]
pub struct Upload {
    pub kind: String,
    pub doc_date: NaiveDate,
    pub fy: i32,
    pub q: u32,
}

/// `kind`, `doc_date` (required: it dates the KPI observation), and the reported period
/// `fy` and `q`.
pub fn check_upload(
    kind: Option<&str>,
    doc_date: Option<&str>,
    fy: Option<&str>,
    q: Option<&str>,
    today: NaiveDate,
) -> Result<Upload, FieldError> {
    let kind = kind.map(str::trim).unwrap_or_default();
    if documents::kind_label(kind).is_none() {
        return Err(FieldError {
            field: "kind",
            kind: "enum",
            message: "Input should be 'presentasi', 'update_operasional', 'laporan', 'artikel', 'catatan' or 'lainnya'",
        });
    }
    let doc_date = doc_date
        .map(str::trim)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .ok_or(FieldError {
            field: "doc_date",
            kind: "date_parsing",
            message: "Input should be a date, YYYY-MM-DD",
        })?;
    if doc_date > today {
        return Err(FieldError {
            field: "doc_date",
            kind: "value_error",
            message: "A document cannot be dated in the future",
        });
    }
    let fy = fy
        .map(str::trim)
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|y| (2000..=2100).contains(y))
        .ok_or(FieldError {
            field: "fy",
            kind: "int_parsing",
            message: "Fiscal year must be 2000 to 2100",
        })?;
    let q = q
        .map(str::trim)
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|q| (1..=4).contains(q))
        .ok_or(FieldError {
            field: "q",
            kind: "int_parsing",
            message: "Quarter must be 1 to 4",
        })?;
    Ok(Upload {
        kind: kind.to_string(),
        doc_date,
        fy,
        q,
    })
}

/// A file name as the owner's browser sent it, made safe to show: the last path segment,
/// control characters dropped, at most 100 characters.
pub fn clean_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(100).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "dokumen".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn an_upload_form_needs_kind_date_and_period() {
        let today = d("2026-10-25");
        let ok = check_upload(
            Some("presentasi"),
            Some("2026-04-30"),
            Some("2026"),
            Some("1"),
            today,
        )
        .unwrap();
        assert_eq!((ok.fy, ok.q, ok.doc_date), (2026, 1, d("2026-04-30")));
        for (kind, date, fy, q, field) in [
            (None, Some("2026-04-30"), Some("2026"), Some("1"), "kind"),
            (
                Some("berita"),
                Some("2026-04-30"),
                Some("2026"),
                Some("1"),
                "kind",
            ),
            (Some("laporan"), None, Some("2026"), Some("1"), "doc_date"),
            (
                Some("laporan"),
                Some("30/04/2026"),
                Some("2026"),
                Some("1"),
                "doc_date",
            ),
            (
                Some("laporan"),
                Some("2026-10-26"),
                Some("2026"),
                Some("1"),
                "doc_date",
            ),
            (
                Some("laporan"),
                Some("2026-04-30"),
                Some("26"),
                Some("1"),
                "fy",
            ),
            (Some("laporan"), Some("2026-04-30"), None, Some("1"), "fy"),
            (
                Some("laporan"),
                Some("2026-04-30"),
                Some("2026"),
                Some("5"),
                "q",
            ),
            (
                Some("laporan"),
                Some("2026-04-30"),
                Some("2026"),
                Some("Q1"),
                "q",
            ),
        ] {
            assert_eq!(
                check_upload(kind, date, fy, q, today).unwrap_err().field,
                field
            );
        }
    }

    #[test]
    fn file_names_are_reduced_to_a_safe_base_name() {
        assert_eq!(clean_filename("C:\\docs\\paparan Q1.pdf"), "paparan Q1.pdf");
        assert_eq!(clean_filename("../../etc/passwd"), "passwd");
        assert_eq!(clean_filename("a\u{0}b\nc.pdf"), "abc.pdf");
        assert_eq!(clean_filename(""), "dokumen");
        assert_eq!(clean_filename(&"x".repeat(300)).chars().count(), 100);
    }
}
