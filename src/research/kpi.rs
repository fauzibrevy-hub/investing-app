//! Operating KPIs entered by hand with provenance (research plan §3.1 `kpi.rs`, D7;
//! prd.md A2.3). One row per (ticker, metric, fiscal year, quarter) carries either a value
//! or an explicit `unknown`, plus the document and its date. "Unknown" is a deliberate
//! entry and is distinct from a cell nobody has filled. Every write is appended to
//! `research-log.jsonl` by the store.

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{json, Value};

use super::packs::{Cadence, Layer, Metric, Pack, PackSet};

/// `document` length limit (a URL or a short reference).
pub const MAX_DOCUMENT_CHARS: usize = 500;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KpiValue {
    pub ticker: String,
    pub metric: String,
    pub fy: i32,
    pub q: u32,
    /// `None` when `unknown`.
    pub value: Option<f64>,
    pub unknown: bool,
    /// The pack metric's unit at entry time.
    pub unit: String,
    /// URL or free text naming the source document.
    pub document: String,
    /// `YYYY-MM-DD`.
    pub document_date: String,
    pub entered_at: String,
}

impl KpiValue {
    /// The cell as the pack section renders it: the value or "unknown", with provenance.
    pub fn to_json(&self) -> Value {
        json!({
            "state": if self.unknown { "unknown" } else { "entered" },
            "value": self.value,
            "unknown": self.unknown,
            "unit": self.unit,
            "fy": self.fy,
            "q": self.q,
            "provenance": {
                "kind": "manual",
                "document": self.document,
                "document_date": self.document_date,
                "entered_at": self.entered_at,
            },
        })
    }
}

/// A field-level validation failure (HTTP 422).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldError {
    pub field: &'static str,
    pub kind: &'static str,
    pub message: String,
    pub input: Value,
}

fn field(field: &'static str, kind: &'static str, message: &str, input: &Value) -> Box<FieldError> {
    Box::new(FieldError {
        field,
        kind,
        message: message.into(),
        input: input.clone(),
    })
}

/// What `PUT /{ticker}/kpi/{metric}/{fy}/{q}` carries after validation.
#[derive(Clone, Debug, PartialEq)]
pub struct KpiInput {
    pub value: Option<f64>,
    pub unknown: bool,
    pub document: String,
    pub document_date: String,
}

/// Body `{value|unknown, document, document_date}`: exactly one of a finite `value` or
/// `unknown: true`; a non-empty document; an ISO date.
pub fn parse_body(v: &Value) -> Result<KpiInput, Box<FieldError>> {
    let unknown = match v.get("unknown") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(field(
                "unknown",
                "bool_type",
                "Input should be a boolean",
                other,
            ))
        }
    };
    let value =
        match v.get("value") {
            None | Some(Value::Null) => None,
            Some(n) => Some(n.as_f64().filter(|x| x.is_finite()).ok_or_else(|| {
                field("value", "float_type", "Input should be a finite number", n)
            })?),
        };
    match (value, unknown) {
        (Some(_), true) => {
            return Err(field(
                "value",
                "value_or_unknown",
                "Give a value or unknown: true, not both",
                &v["value"],
            ))
        }
        (None, false) => {
            return Err(field(
                "value",
                "missing",
                "A value or unknown: true is required",
                &Value::Null,
            ))
        }
        _ => {}
    }
    let document = v["document"].as_str().map(str::trim).unwrap_or_default();
    let len = document.chars().count();
    if len == 0 || len > MAX_DOCUMENT_CHARS {
        return Err(field(
            "document",
            "string_length",
            "Document reference must be 1 to 500 characters",
            &v["document"],
        ));
    }
    let document_date = v["document_date"]
        .as_str()
        .map(str::trim)
        .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .ok_or_else(|| {
            field(
                "document_date",
                "date_parsing",
                "Input should be a date (YYYY-MM-DD)",
                &v["document_date"],
            )
        })?;
    Ok(KpiInput {
        value,
        unknown,
        document: document.to_string(),
        document_date: document_date.to_string(),
    })
}

/// `fy` and `q` from the path; an annual KPI is keyed at Q4 of its year (ASUMSI R2.3).
pub fn parse_period(cadence: Cadence, fy: &str, q: &str) -> Result<(i32, u32), Box<FieldError>> {
    let fy_n = fy
        .parse::<i32>()
        .ok()
        .filter(|y| (2000..=2100).contains(y))
        .ok_or_else(|| {
            field(
                "fy",
                "int_parsing",
                "Fiscal year must be 2000 to 2100",
                &json!(fy),
            )
        })?;
    let q_n = q
        .parse::<u32>()
        .ok()
        .filter(|q| (1..=4).contains(q))
        .ok_or_else(|| field("q", "int_parsing", "Quarter must be 1 to 4", &json!(q)))?;
    if cadence == Cadence::Annual && q_n != 4 {
        return Err(field(
            "q",
            "annual_at_q4",
            "An annual KPI is entered at Q4 of its year",
            &json!(q),
        ));
    }
    Ok((fy_n, q_n))
}

/// The KPI metric `id` of `pack` (or of core), if it is one.
pub fn kpi_metric<'a>(
    packs: &'a PackSet,
    pack: &'a Pack,
    id: &str,
) -> Option<(&'a Pack, &'a Metric)> {
    packs
        .resolve(pack, id)
        .or_else(|| packs.resolve(pack, &format!("core.{id}")))
        .filter(|(_, m)| m.layer == Layer::Kpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_needs_exactly_one_of_value_or_unknown() {
        let ok =
            parse_body(&json!({"value": 12.5, "document": " doc ", "document_date": "2026-04-30"}))
                .unwrap();
        assert_eq!(ok.value, Some(12.5));
        assert_eq!(ok.document, "doc");
        let unk =
            parse_body(&json!({"unknown": true, "document": "x", "document_date": "2026-04-30"}))
                .unwrap();
        assert!(unk.unknown && unk.value.is_none());
        for (body, f) in [
            (
                json!({"document": "x", "document_date": "2026-04-30"}),
                "value",
            ),
            (
                json!({"value": 1, "unknown": true, "document": "x", "document_date": "2026-04-30"}),
                "value",
            ),
            (
                json!({"value": "1", "document": "x", "document_date": "2026-04-30"}),
                "value",
            ),
            (
                json!({"value": 1, "document": "", "document_date": "2026-04-30"}),
                "document",
            ),
            (
                json!({"value": 1, "document": "x", "document_date": "30/04/2026"}),
                "document_date",
            ),
            (
                json!({"unknown": "yes", "document": "x", "document_date": "2026-04-30"}),
                "unknown",
            ),
        ] {
            assert_eq!(parse_body(&body).unwrap_err().field, f, "{body}");
        }
    }

    #[test]
    fn period_parsing_and_annual_rule() {
        assert_eq!(
            parse_period(Cadence::Quarterly, "2026", "2").unwrap(),
            (2026, 2)
        );
        assert_eq!(
            parse_period(Cadence::Annual, "2025", "4").unwrap(),
            (2025, 4)
        );
        assert_eq!(
            parse_period(Cadence::Annual, "2025", "2").unwrap_err().kind,
            "annual_at_q4"
        );
        assert_eq!(
            parse_period(Cadence::Quarterly, "26", "2")
                .unwrap_err()
                .field,
            "fy"
        );
        assert_eq!(
            parse_period(Cadence::Quarterly, "2026", "5")
                .unwrap_err()
                .field,
            "q"
        );
    }
}
