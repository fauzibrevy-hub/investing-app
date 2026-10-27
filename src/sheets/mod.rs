//! Google Sheets API v4 client (prd.md §6.1): service-account JWT → OAuth2
//! access token, `values.get`, `values.update`, `spreadsheets.get`,
//! `batchUpdate`, plus a small TTL cache for reads.
//!
//! Replaces MC `backend/services/google_sheets.py` transport. The OAuth user
//! fallback MC has is intentionally not supported.

mod cache;
mod token;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

pub use cache::TtlCache;
pub use token::TokenSource;

const SHEETS_BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";
pub const SCOPE: &str = "https://www.googleapis.com/auth/spreadsheets";

#[derive(Debug, thiserror::Error)]
pub enum SheetsError {
    #[error("Google credentials unavailable: {0}")]
    Credentials(String),
    #[error("Google API request failed: {0}")]
    Http(String),
    #[error("Google API returned {status}: {message}")]
    Status { status: u16, message: String },
}

pub type Rows = Vec<Vec<String>>;
/// Raw cells from an `UNFORMATTED_VALUE` read: numbers stay JSON numbers.
pub type Grid = Vec<Vec<Value>>;

#[derive(Clone)]
pub struct SheetsClient {
    http: reqwest::Client,
    base: Arc<str>,
    token: Arc<TokenSource>,
}

impl SheetsClient {
    pub fn new(credentials_path: PathBuf) -> Self {
        Self::with_parts(SHEETS_BASE, TokenSource::service_account(credentials_path))
    }

    /// Test/alternate constructor: custom API base URL and token source.
    pub fn with_parts(base: &str, token: TokenSource) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .build()
            .expect("reqwest client");
        Self {
            http,
            base: base.trim_end_matches('/').into(),
            token: Arc::new(token),
        }
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value, SheetsError> {
        let token = self.token.access_token(&self.http).await?;
        let res = req
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| SheetsError::Http(e.without_url().to_string()))?;
        let status = res.status();
        let body: Value = res.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let message = body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(SheetsError::Status {
                status: status.as_u16(),
                message,
            });
        }
        Ok(body)
    }

    fn url(&self, spreadsheet_id: &str, suffix: &str) -> String {
        format!("{}/{}{}", self.base, encode(spreadsheet_id), suffix)
    }

    /// `values.get` with the default FORMATTED_VALUE rendering, like MC's
    /// `read_sheet`. Cells come back as strings; rows keep Google's ragged
    /// shape (trailing empty cells omitted).
    pub async fn values_get(&self, spreadsheet_id: &str, range: &str) -> Result<Rows, SheetsError> {
        let url = self.url(spreadsheet_id, &format!("/values/{}", encode(range)));
        let body = self.send(self.http.get(url)).await?;
        Ok(rows_from(&body["values"]))
    }

    /// `values.get` with `UNFORMATTED_VALUE`: numeric cells come back as
    /// numbers whatever their display format (e.g. accounting "-" for 0).
    pub async fn values_get_unformatted(
        &self,
        spreadsheet_id: &str,
        range: &str,
    ) -> Result<Grid, SheetsError> {
        let url = self.url(spreadsheet_id, &format!("/values/{}", encode(range)));
        let req = self
            .http
            .get(url)
            .query(&[("valueRenderOption", "UNFORMATTED_VALUE")]);
        let body = self.send(req).await?;
        Ok(body["values"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|r| r.as_array().cloned().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `values.update` with `USER_ENTERED`, on an explicit range.
    pub async fn values_update(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<Value>>,
    ) -> Result<Value, SheetsError> {
        let url = self.url(spreadsheet_id, &format!("/values/{}", encode(range)));
        let req = self
            .http
            .put(url)
            .query(&[("valueInputOption", "USER_ENTERED")])
            .json(&json!({"range": range, "majorDimension": "ROWS", "values": values}));
        self.send(req).await
    }

    /// `spreadsheets.get` restricted to `fields`.
    pub async fn spreadsheet_get(
        &self,
        spreadsheet_id: &str,
        fields: &str,
    ) -> Result<Value, SheetsError> {
        let req = self
            .http
            .get(self.url(spreadsheet_id, ""))
            .query(&[("fields", fields)]);
        self.send(req).await
    }

    pub async fn batch_update(
        &self,
        spreadsheet_id: &str,
        requests: Vec<Value>,
    ) -> Result<Value, SheetsError> {
        let req = self
            .http
            .post(self.url(spreadsheet_id, ":batchUpdate"))
            .json(&json!({"requests": requests}));
        self.send(req).await
    }
}

fn rows_from(values: &Value) -> Rows {
    values
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| cells.iter().map(cell_string).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cell_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Percent-encode a path segment (A1 ranges contain `'`, `!`, `:` and spaces),
/// matching Python's `quote(range, safe='')`.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 0-based column index → A1 letters (0 → A, 25 → Z, 26 → AA).
pub fn col_letter(mut index: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a1_ranges_like_python_quote() {
        assert_eq!(
            encode("'Transaction Log'!A1:Z"),
            "%27Transaction%20Log%27%21A1%3AZ"
        );
    }

    #[test]
    fn column_letters() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(35), "AJ");
        assert_eq!(col_letter(701), "ZZ");
        assert_eq!(col_letter(702), "AAA");
    }

    #[test]
    fn rows_are_strings_and_ragged() {
        let rows = rows_from(&json!([["a", 1, null], [], ["x"]]));
        assert_eq!(
            rows,
            vec![
                vec!["a".to_string(), "1".into(), "".into()],
                vec![],
                vec!["x".to_string()]
            ]
        );
    }
}
