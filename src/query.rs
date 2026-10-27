//! Query-string validation that reports errors the way FastAPI/pydantic v2
//! does, so 422 bodies match MC.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::error::ApiError;

/// Collects validation errors across several parameters, like FastAPI does
/// before returning one 422 with every problem listed.
#[derive(Default)]
pub struct QueryCheck {
    errors: Vec<Value>,
}

impl QueryCheck {
    /// Required string parameter.
    pub fn required<'a>(
        &mut self,
        params: &'a HashMap<String, String>,
        name: &str,
    ) -> Option<&'a str> {
        let v = params.get(name).map(String::as_str);
        if v.is_none() {
            self.errors.push(json!({"type": "missing", "loc": ["query", name], "msg": "Field required", "input": null}));
        }
        v
    }

    /// Optional integer with inclusive bounds (`Query(default, ge=.., le=..)`).
    pub fn int(
        &mut self,
        params: &HashMap<String, String>,
        name: &str,
        default: i64,
        ge: Option<i64>,
        le: Option<i64>,
    ) -> i64 {
        let Some(raw) = params.get(name) else {
            return default;
        };
        let loc = json!(["query", name]);
        let Some(n) = parse_int(raw) else {
            self.errors.push(json!({
                "type": "int_parsing", "loc": loc,
                "msg": "Input should be a valid integer, unable to parse string as an integer",
                "input": raw,
            }));
            return default;
        };
        if let Some(ge) = ge.filter(|ge| n < *ge) {
            self.errors.push(json!({
                "type": "greater_than_equal", "loc": loc,
                "msg": format!("Input should be greater than or equal to {ge}"),
                "input": raw, "ctx": {"ge": ge},
            }));
        } else if let Some(le) = le.filter(|le| n > *le) {
            self.errors.push(json!({
                "type": "less_than_equal", "loc": loc,
                "msg": format!("Input should be less than or equal to {le}"),
                "input": raw, "ctx": {"le": le},
            }));
        }
        n
    }

    pub fn finish(self) -> Result<(), ApiError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ApiError::Validation(self.errors))
        }
    }
}

fn parse_int(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        || digits.starts_with('_')
    {
        return None;
    }
    s.replace('_', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn reports_fastapi_shaped_errors() {
        let p = params(&[("limit", "0"), ("skip", "x")]);
        let mut q = QueryCheck::default();
        q.int(&p, "skip", 0, Some(0), Some(10000));
        q.int(&p, "limit", 50, Some(1), Some(500));
        q.required(&p, "tx_type");
        let ApiError::Validation(errs) = q.finish().unwrap_err() else {
            panic!()
        };
        assert_eq!(errs[0]["type"], "int_parsing");
        assert_eq!(
            errs[1],
            json!({
                "type": "greater_than_equal", "loc": ["query", "limit"],
                "msg": "Input should be greater than or equal to 1", "input": "0", "ctx": {"ge": 1}
            })
        );
        assert_eq!(
            errs[2],
            json!({"type": "missing", "loc": ["query", "tx_type"], "msg": "Field required", "input": null})
        );
    }

    #[test]
    fn defaults_and_valid_values() {
        let mut q = QueryCheck::default();
        assert_eq!(q.int(&params(&[]), "limit", 50, Some(1), Some(500)), 50);
        assert_eq!(
            q.int(
                &params(&[("limit", " 200 ")]),
                "limit",
                50,
                Some(1),
                Some(500)
            ),
            200
        );
        assert!(q.finish().is_ok());
    }
}
