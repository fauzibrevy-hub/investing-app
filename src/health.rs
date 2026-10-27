//! `GET /api/health` (public) and `GET /api/health?deep=1` (per-source status,
//! prd.md §7). Deep checks are cheap local probes: they report whether each
//! source is reachable from this process, never its contents.

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::config::Config;

#[derive(Deserialize)]
struct HealthQuery {
    deep: Option<String>,
}

fn source(ok: bool, detail: &str) -> Value {
    json!({"ok": ok, "detail": detail})
}

fn file_source(path: &Path, what: &str) -> Value {
    if path.is_file() {
        source(true, what)
    } else {
        source(false, &format!("{what} not found"))
    }
}

pub fn deep_sources(cfg: &Config) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(
        "sheets".into(),
        file_source(&cfg.google_credentials_path, "service account credentials"),
    );
    m.insert(
        "vault".into(),
        if cfg.vault_root.is_dir() {
            source(true, "vault root")
        } else {
            source(false, "vault root not found")
        },
    );
    m.insert(
        "thesis_drafts".into(),
        match cfg.thesis_drafts_path.parent() {
            Some(p) if p.is_dir() => source(true, "drafts directory"),
            _ => source(false, "drafts directory not found"),
        },
    );
    m.insert("idx".into(), file_source(&cfg.idx_db_path, "IDX database"));
    m.insert(
        "data_dir".into(),
        if cfg.data_dir.is_dir() {
            source(true, "data dir")
        } else {
            source(false, "data dir not found")
        },
    );
    m.extend(crate::research::health::sources(cfg));
    m
}

async fn health(State(cfg): State<Arc<Config>>, Query(q): Query<HealthQuery>) -> Json<Value> {
    let deep = q
        .deep
        .as_deref()
        .is_some_and(|v| matches!(v, "1" | "true" | "yes"));
    if !deep {
        return Json(json!({"status": "ok"}));
    }
    let sources = deep_sources(&cfg);
    let all_ok = sources.values().all(|s| s["ok"] == true);
    Json(json!({"status": if all_ok { "ok" } else { "degraded" }, "sources": sources}))
}

pub fn router(cfg: Arc<Config>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .with_state(cfg)
}
