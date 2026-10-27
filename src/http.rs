//! Shared outbound HTTP client. Extracted unchanged from `macro_watch::upstream` so the
//! research adapters reuse the same no-redirect, 8 s, httpx-worded error behaviour.

use std::time::Duration;

use serde_json::Value;
/// A Python exception: class name and `str(e)`.
#[derive(Debug, Clone, PartialEq)]
pub struct PyErr {
    pub kind: String,
    pub msg: String,
}

impl PyErr {
    pub fn new(kind: &str, msg: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            msg: msg.into(),
        }
    }
}

pub type PyResult<T> = Result<T, PyErr>;

pub struct Response {
    pub text: String,
}

impl Response {
    /// httpx `Response.json()`.
    pub fn json(&self) -> PyResult<Value> {
        serde_json::from_str(&self.text).map_err(|e| {
            let trimmed = self.text.trim_start();
            let starts_ok = trimmed.starts_with(['{', '[', '"', '-', 't', 'f', 'n'])
                || trimmed.starts_with(|c: char| c.is_ascii_digit());
            if !starts_ok {
                let lead = self.text.len() - trimmed.len();
                let (line, col) = line_col(&self.text, lead);
                PyErr::new(
                    "JSONDecodeError",
                    format!("Expecting value: line {line} column {col} (char {lead})"),
                )
            } else {
                PyErr::new("JSONDecodeError", e.to_string())
            }
        })
    }
}

fn line_col(s: &str, pos: usize) -> (usize, usize) {
    let before = &s[..pos];
    let line = before.matches('\n').count() + 1;
    let col = pos - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
    (line, col)
}

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    pub fn new(user_agent: Option<&str>) -> Self {
        Self::with_timeout(user_agent, Duration::from_secs(8))
    }

    /// Same client with a different overall timeout (FRED needs about 30 s at times).
    pub fn with_timeout(user_agent: Option<&str>, timeout: Duration) -> Self {
        let mut b = reqwest::Client::builder()
            .timeout(timeout)
            // httpx does not follow redirects by default; MC reports them as errors.
            .redirect(reqwest::redirect::Policy::none());
        if let Some(ua) = user_agent {
            b = b.user_agent(ua);
        }
        Self {
            client: b.build().expect("reqwest client"),
        }
    }

    /// `client.get(url)` + `raise_for_status()`.
    pub async fn get(&self, url: &str) -> PyResult<Response> {
        let res = self.client.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                PyErr::new(
                    if e.is_connect() {
                        "ConnectTimeout"
                    } else {
                        "ReadTimeout"
                    },
                    "",
                )
            } else if e.is_connect() {
                PyErr::new("ConnectError", "All connection attempts failed")
            } else {
                PyErr::new("RemoteProtocolError", e.without_url().to_string())
            }
        })?;
        let status = res.status();
        if !status.is_success() {
            return Err(status_error(&res, url));
        }
        let text = res
            .text()
            .await
            .map_err(|e| PyErr::new("ReadError", e.without_url().to_string()))?;
        Ok(Response { text })
    }
}

/// httpx `Response.raise_for_status()` message.
fn status_error(res: &reqwest::Response, url: &str) -> PyErr {
    let status = res.status();
    let code = status.as_u16();
    let reason = res
        .extensions()
        .get::<hyper::ext::ReasonPhrase>()
        .map(|r| String::from_utf8_lossy(r.as_bytes()).into_owned())
        .or_else(|| status.canonical_reason().map(str::to_string))
        .unwrap_or_default();
    let kind = match code / 100 {
        1 => "Informational response",
        3 => "Redirect response",
        4 => "Client error",
        _ => "Server error",
    };
    let shown = url;
    let more = format!("For more information check: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/{code}");
    let location = res
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|l| l.to_str().ok());
    let msg = match location.filter(|_| status.is_redirection()) {
        Some(loc) => format!(
            "{kind} '{code} {reason}' for url '{shown}'\nRedirect location: '{loc}'\n{more}"
        ),
        None => format!("{kind} '{code} {reason}' for url '{shown}'\n{more}"),
    };
    PyErr::new("HTTPStatusError", msg)
}
