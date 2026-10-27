//! Auth (prd.md §5): one secret, `INVESTING_APP_TOKEN`.
//!
//! - Non-browser clients send `Authorization: Bearer <token>`.
//! - The PWA exchanges the token once at `POST /api/session` for an HMAC-signed
//!   `investing_session` cookie. The signing key is derived from the token, so
//!   rotating the token invalidates every session.
//! - Cookie-authenticated writes must also carry `X-Investing-Client: web`.
//! - `GET /api/health` and `/api/session` (GET checks the cookie, POST
//!   issues it, DELETE clears it) are the only routes reachable without
//!   credentials.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::ApiError;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "investing_session";
pub const SESSION_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;
const CLIENT_HEADER: &str = "x-investing-client";

#[derive(Clone)]
pub struct Auth {
    token: Arc<str>,
    session_key: Arc<[u8; 32]>,
}

impl Auth {
    pub fn new(token: &str) -> Self {
        let mut mac =
            HmacSha256::new_from_slice(token.as_bytes()).expect("hmac accepts any key length");
        mac.update(b"finance-session-v1");
        let key: [u8; 32] = mac.finalize().into_bytes().into();
        Self {
            token: token.into(),
            session_key: Arc::new(key),
        }
    }

    fn token_matches(&self, candidate: &str) -> bool {
        candidate.as_bytes().ct_eq(self.token.as_bytes()).into()
    }

    fn sign(&self, payload: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.session_key[..]).expect("32-byte key");
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn issue_session(&self, now: u64) -> String {
        let payload = format!("v1.{now}");
        let sig = self.sign(&payload);
        format!("{payload}.{sig}")
    }

    pub fn verify_session(&self, value: &str, now: u64) -> bool {
        let Some((payload, sig)) = value.rsplit_once('.') else {
            return false;
        };
        let Some(issued) = payload
            .strip_prefix("v1.")
            .and_then(|s| s.parse::<u64>().ok())
        else {
            return false;
        };
        let expected = self.sign(payload);
        let sig_ok: bool = sig.as_bytes().ct_eq(expected.as_bytes()).into();
        sig_ok && issued <= now + 60 && now.saturating_sub(issued) <= SESSION_MAX_AGE_SECS
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| v)
}

fn is_public(method: &Method, path: &str) -> bool {
    (path == "/api/health" && (method == Method::GET || method == Method::HEAD))
        || path == "/api/session"
}

fn unauthorized() -> Response {
    let mut res = ApiError::new(StatusCode::UNAUTHORIZED, "Not authenticated").into_response();
    res.headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    res
}

/// Middleware guarding every `/api/*` route.
pub async fn require_auth(State(auth): State<Auth>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if !path.starts_with("/api/") || is_public(req.method(), path) {
        return next.run(req).await;
    }
    let headers = req.headers();
    if let Some(token) = bearer(headers) {
        return if auth.token_matches(token) {
            next.run(req).await
        } else {
            unauthorized()
        };
    }
    if let Some(cookie) = session_cookie(headers) {
        if auth.verify_session(cookie, now_secs()) {
            let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
            let from_web = headers.get(CLIENT_HEADER).and_then(|v| v.to_str().ok()) == Some("web");
            if !safe && !from_web {
                return ApiError::new(StatusCode::FORBIDDEN, "Missing X-Investing-Client header")
                    .into_response();
            }
            return next.run(req).await;
        }
    }
    unauthorized()
}

#[derive(Deserialize)]
struct SessionRequest {
    token: String,
}

fn set_cookie(value: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={value}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={max_age}"
    ))
    .expect("cookie is ASCII")
}

async fn create_session(State(auth): State<Auth>, body: Option<Json<SessionRequest>>) -> Response {
    match body {
        Some(Json(req)) if auth.token_matches(req.token.trim()) => {
            let cookie = auth.issue_session(now_secs());
            (
                StatusCode::NO_CONTENT,
                [(
                    header::SET_COOKIE,
                    set_cookie(&cookie, SESSION_MAX_AGE_SECS),
                )],
            )
                .into_response()
        }
        _ => unauthorized(),
    }
}

async fn delete_session() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, set_cookie("", 0))],
    )
        .into_response()
}

/// Lets the PWA check at startup whether its cookie is still valid:
/// 204 when it is, 401 otherwise.
async fn check_session(State(auth): State<Auth>, headers: HeaderMap) -> Response {
    match session_cookie(&headers) {
        Some(c) if auth.verify_session(c, now_secs()) => StatusCode::NO_CONTENT.into_response(),
        _ => unauthorized(),
    }
}

pub fn router(auth: Auth) -> Router {
    Router::new()
        .route(
            "/api/session",
            get(check_session)
                .post(create_session)
                .delete(delete_session),
        )
        .with_state(auth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip_and_expiry() {
        let auth = Auth::new(&"a".repeat(32));
        let c = auth.issue_session(1_000);
        assert!(auth.verify_session(&c, 1_000));
        assert!(auth.verify_session(&c, 1_000 + SESSION_MAX_AGE_SECS));
        assert!(!auth.verify_session(&c, 1_001 + SESSION_MAX_AGE_SECS));
        assert!(!auth.verify_session(&c.replace("v1.1000", "v1.1001"), 1_000));
    }

    #[test]
    fn rotating_token_invalidates_sessions() {
        let c = Auth::new(&"a".repeat(32)).issue_session(1_000);
        assert!(!Auth::new(&"b".repeat(32)).verify_session(&c, 1_000));
    }

    #[test]
    fn parses_bearer_and_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bearer  abc "),
        );
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("x=1; investing_session=v1.2.ff"),
        );
        assert_eq!(bearer(&h), Some("abc"));
        assert_eq!(session_cookie(&h), Some("v1.2.ff"));
    }
}
