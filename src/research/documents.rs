//! Documents and evidence (research plan §3.1 `documents.rs`, R4.5; design.md §4.2 Sumber):
//! the owner files a presentation, an operational update or an article by URL or pasted
//! text, with its date, and may attach it to a claim or to a KPI cell as evidence. Nothing
//! is fetched: a URL is stored as written and only its shape is checked.
//!
//! Pure: validation and the KPI key format. Rows and routes are `store/r4.rs` and
//! `api/r4.rs`.

use chrono::NaiveDate;

pub const MAX_URL_CHARS: usize = 500;
pub const MAX_TEXT_CHARS: usize = 20_000;

/// Document kinds and their Indonesian labels.
pub const KINDS: [(&str, &str); 6] = [
    ("presentasi", "Presentasi"),
    ("update_operasional", "Update operasional"),
    ("laporan", "Laporan"),
    ("artikel", "Artikel"),
    ("catatan", "Catatan"),
    ("lainnya", "Lainnya"),
];

pub fn kind_label(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l)
}

/// A validation failure naming the field (HTTP 422).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub kind: &'static str,
    pub message: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewDocument {
    pub kind: String,
    pub url: Option<String>,
    pub text: Option<String>,
    pub doc_date: Option<NaiveDate>,
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A document needs a URL (`http://` or `https://`), pasted text, or both; the date is
/// optional and cannot lie after `today`.
pub fn check_document(
    kind: &str,
    url: Option<&str>,
    text: Option<&str>,
    doc_date: Option<NaiveDate>,
    today: NaiveDate,
) -> Result<NewDocument, FieldError> {
    if kind_label(kind).is_none() {
        return Err(FieldError {
            field: "kind",
            kind: "enum",
            message: "Input should be 'presentasi', 'update_operasional', 'laporan', 'artikel', 'catatan' or 'lainnya'",
        });
    }
    let (url, text) = (clean(url), clean(text));
    if url.is_none() && text.is_none() {
        return Err(FieldError {
            field: "url",
            kind: "missing",
            message: "A document needs a url or text",
        });
    }
    if let Some(u) = &url {
        let well_formed = (u.starts_with("http://") || u.starts_with("https://"))
            && u.len() <= MAX_URL_CHARS
            && !u.contains(char::is_whitespace);
        if !well_formed {
            return Err(FieldError {
                field: "url",
                kind: "url_parsing",
                message: "Input should be an http or https URL of at most 500 characters",
            });
        }
    }
    if text
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_TEXT_CHARS)
    {
        return Err(FieldError {
            field: "text",
            kind: "string_length",
            message: "Text must be at most 20000 characters",
        });
    }
    if doc_date.is_some_and(|d| d > today) {
        return Err(FieldError {
            field: "doc_date",
            kind: "value_error",
            message: "A document cannot be dated in the future",
        });
    }
    Ok(NewDocument {
        kind: kind.into(),
        url,
        text,
        doc_date,
    })
}

/// `<metric>:<FY>Q<q>`, the key an evidence link to a KPI cell carries.
pub fn kpi_key(metric: &str, fy: i32, q: u32) -> String {
    format!("{metric}:{fy}Q{q}")
}

/// The inverse of `kpi_key`; `None` unless the metric is non-empty and the quarter 1 to 4.
pub fn parse_kpi_key(key: &str) -> Option<(String, i32, u32)> {
    let (metric, period) = key.rsplit_once(':')?;
    let (fy, q) = period.split_once('Q')?;
    let (fy, q) = (fy.parse::<i32>().ok()?, q.parse::<u32>().ok()?);
    let ok = !metric.is_empty()
        && !metric.contains(char::is_whitespace)
        && (1..=4).contains(&q)
        && (1900..=2200).contains(&fy);
    ok.then(|| (metric.to_string(), fy, q))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    const TODAY: &str = "2026-10-24";

    fn check(kind: &str, url: Option<&str>, text: Option<&str>) -> Result<NewDocument, FieldError> {
        check_document(kind, url, text, None, d(TODAY))
    }

    #[test]
    fn a_document_needs_a_known_kind_and_a_url_or_text() {
        for (kind, label) in KINDS {
            assert_eq!(kind_label(kind), Some(label));
            assert!(check(kind, Some("https://example.test/a"), None).is_ok());
        }
        assert_eq!(
            check("berita", Some("https://example.test"), None)
                .unwrap_err()
                .field,
            "kind"
        );
        assert_eq!(check("artikel", None, None).unwrap_err().field, "url");
        assert_eq!(
            check("artikel", Some("  "), Some("  ")).unwrap_err().field,
            "url"
        );
        let both = check(
            "artikel",
            Some(" https://example.test/a "),
            Some(" ringkasan "),
        )
        .unwrap();
        assert_eq!(both.url.as_deref(), Some("https://example.test/a"));
        assert_eq!(both.text.as_deref(), Some("ringkasan"));
        let text_only = check("catatan", None, Some("hasil rapat")).unwrap();
        assert_eq!(text_only.url, None);
    }

    #[test]
    fn a_url_is_checked_for_shape_only() {
        for bad in [
            "ftp://example.test/a",
            "example.test/a",
            "javascript:alert(1)",
            "https://example.test/a b",
        ] {
            assert_eq!(
                check("artikel", Some(bad), None).unwrap_err().field,
                "url",
                "{bad}"
            );
        }
        let long = format!("https://example.test/{}", "a".repeat(500));
        assert_eq!(
            check("artikel", Some(&long), None).unwrap_err().field,
            "url"
        );
        assert!(check("artikel", Some("http://example.test/a"), None).is_ok());
    }

    #[test]
    fn text_and_date_are_bounded() {
        assert!(check("catatan", None, Some(&"x".repeat(20_000))).is_ok());
        assert_eq!(
            check("catatan", None, Some(&"x".repeat(20_001)))
                .unwrap_err()
                .field,
            "text"
        );
        let ok = check_document("catatan", None, Some("x"), Some(d(TODAY)), d(TODAY)).unwrap();
        assert_eq!(ok.doc_date, Some(d(TODAY)));
        let future = check_document("catatan", None, Some("x"), Some(d("2026-10-25")), d(TODAY));
        assert_eq!(future.unwrap_err().field, "doc_date");
    }

    #[test]
    fn kpi_keys_round_trip_and_reject_nonsense() {
        let key = kpi_key("production_mt", 2026, 2);
        assert_eq!(key, "production_mt:2026Q2");
        assert_eq!(parse_kpi_key(&key), Some(("production_mt".into(), 2026, 2)));
        for bad in [
            "",
            "production_mt",
            "production_mt:2026",
            "production_mt:2026Q5",
            ":2026Q2",
            "a b:2026Q1",
            "x:20Q1",
            "x:abcdQ1",
        ] {
            assert_eq!(parse_kpi_key(bad), None, "{bad}");
        }
    }
}
