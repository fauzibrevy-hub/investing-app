//! Finance App backend: one axum binary serving `/api/*` and the built UI
//! (`web/dist`) on `127.0.0.1:8787`.

pub mod auth;
pub mod config;
pub mod error;
pub mod finance;
pub mod health;
pub mod http;
pub mod idempotency;
pub mod idx;
pub mod macro_watch;
pub mod py;
pub mod query;
pub mod research;
pub mod sheets;
pub mod thesis;

use std::sync::Arc;

use axum::body::Body;
use axum::http::header::{HeaderValue, CACHE_CONTROL};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{middleware, Router};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::error::ApiError;

async fn api_not_found() -> impl IntoResponse {
    ApiError::new(StatusCode::NOT_FOUND, "Not Found")
}

/// Cache policy for the built UI (plan.md §P5).
///
/// Vite writes content-hashed names under `/assets/`, so those can be cached forever; everything
/// else (`index.html`, `sw.js`, `manifest.webmanifest`, the unhashed icons) must be revalidated or
/// a new deploy never reaches the installed PWA. Without this layer the binary sent no
/// `Cache-Control` at all, so the browser applied its own heuristic freshness.
async fn static_cache_headers(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut res = next.run(req).await;
    let policy: Option<&'static str> = if path.starts_with("/api/") {
        None
    } else if path.starts_with("/assets/") {
        Some("public, max-age=31536000, immutable")
    } else {
        Some("no-cache")
    };
    if let Some(policy) = policy {
        res.headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static(policy));
    }
    res
}

pub fn build_app(cfg: Config) -> Router {
    let cfg = Arc::new(cfg);
    let auth = auth::Auth::new(&cfg.token);

    let api = Router::new()
        .merge(health::router(cfg.clone()))
        .merge(auth::router(auth.clone()))
        .merge(finance::router(cfg.clone()))
        .merge(thesis::router(cfg.clone()))
        .merge(idx::router(cfg.clone()))
        .merge(macro_watch::router(cfg.clone()))
        .merge(research::router(cfg.clone()))
        .route("/api/{*rest}", axum::routing::any(api_not_found));

    // A missing web/dist simply yields 404 for every non-API path.
    let static_files = ServeDir::new(&cfg.web_dist).append_index_html_on_directories(true);

    api.fallback_service(static_files)
        .layer(middleware::from_fn_with_state(auth, auth::require_auth))
        // Cache policy for the built UI; see `static_cache_headers`.
        .layer(middleware::from_fn(static_cache_headers))
        // Logs method, path, status and latency only: no query, headers or body.
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request<Body>| {
            tracing::info_span!("http", method = %req.method(), path = %req.uri().path())
        }))
}
