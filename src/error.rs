//! Error bodies in the same shapes MC (FastAPI) produces, so clients and
//! `parity` see identical error JSON:
//! - `HTTPException`  → `{"detail": "<message>"}`
//! - request validation → 422 `{"detail": [{"type", "loc", "msg", "input"}, ...]}`

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum ApiError {
    /// FastAPI `HTTPException(status_code, detail=...)`.
    Http(StatusCode, String),
    /// FastAPI `RequestValidationError` (always 422).
    Validation(Vec<Value>),
}

impl ApiError {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self::Http(status, detail.into())
    }

    /// One "missing" entry per absent query parameter, as FastAPI reports them.
    pub fn missing_query(names: &[&str]) -> Self {
        Self::Validation(
            names
                .iter()
                .map(|n| json!({"type": "missing", "loc": ["query", n], "msg": "Field required", "input": null}))
                .collect(),
        )
    }

    /// A single validation entry, e.g. `("body", "amount", "value_error", "Value error, ...", input)`.
    pub fn invalid(loc: &[&str], kind: &str, msg: impl Into<String>, input: Value) -> Self {
        Self::Validation(vec![validation_entry(loc, kind, msg, input)])
    }
}

pub fn validation_entry(loc: &[&str], kind: &str, msg: impl Into<String>, input: Value) -> Value {
    json!({"type": kind, "loc": loc, "msg": msg.into(), "input": input})
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Http(status, detail) => {
                (status, Json(json!({ "detail": detail }))).into_response()
            }
            Self::Validation(detail) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "detail": detail })),
            )
                .into_response(),
        }
    }
}

pub type ApiResult<T = Json<Value>> = Result<T, ApiError>;
