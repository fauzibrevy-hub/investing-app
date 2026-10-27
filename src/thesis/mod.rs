//! Area 3 (investment theses): `/api/thesis/*`, ported from MC
//! `backend/api/thesis_bank.py`.
//!
//! - `summary`, `{ticker}`, `macro`: read-only parsers over the Obsidian
//!   vault (`INVESTING_VAULT_ROOT`); the app never writes to the vault.
//! - `drafts*`: the drafts JSON store (`INVESTING_THESIS_DRAFTS_PATH`) plus
//!   read-only vault theses.

mod dossier;
mod drafts;
mod lens;
mod md;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Map, Value};

use crate::auth::now_secs;
use crate::config::Config;
use crate::error::{ApiError, ApiResult};

const VIEW_TTL: Duration = Duration::from_secs(60);
const ARTICLES_TTL: Duration = Duration::from_secs(600);

type Cache<V> = Mutex<HashMap<String, (Instant, V)>>;

pub struct ThesisState {
    vault_root: PathBuf,
    drafts_path: PathBuf,
    /// Serializes read-modify-write of the drafts file.
    drafts_lock: tokio::sync::Mutex<()>,
    views: Cache<Value>,
    articles: Cache<Map<String, Value>>,
}

impl ThesisState {
    pub fn new(cfg: &Config) -> Self {
        Self {
            vault_root: cfg.vault_root.clone(),
            drafts_path: cfg.thesis_drafts_path.clone(),
            drafts_lock: tokio::sync::Mutex::new(()),
            views: Mutex::default(),
            articles: Mutex::default(),
        }
    }

    fn stock_root(&self) -> PathBuf {
        self.vault_root.join("finance/stock")
    }

    fn article_bank(&self) -> PathBuf {
        self.vault_root.join("finance/article-bank")
    }

    fn vault_thesis_dir(&self) -> PathBuf {
        self.vault_root.join("finance/thesis")
    }

    fn articles_for(&self, tickers: &[String]) -> Map<String, Value> {
        let mut sorted = tickers.to_vec();
        sorted.sort();
        let key = sorted.join(",");
        if let Some((at, v)) = self.articles.lock().expect("lock").get(&key) {
            if at.elapsed() < ARTICLES_TTL {
                return v.clone();
            }
        }
        let v = dossier::article_index(&self.article_bank(), tickers);
        self.articles
            .lock()
            .expect("lock")
            .insert(key, (Instant::now(), v.clone()));
        v
    }

    /// MC `_cached`: results carrying a vault read failure are not cached.
    fn cached(&self, key: &str, build: impl FnOnce() -> Value) -> Value {
        if let Some((at, v)) = self.views.lock().expect("lock").get(key) {
            if at.elapsed() < VIEW_TTL {
                return v.clone();
            }
        }
        let v = build();
        if !dossier::has_read_error(&v) {
            self.views
                .lock()
                .expect("lock")
                .insert(key.to_string(), (Instant::now(), v.clone()));
        }
        v
    }
}

type St = State<Arc<ThesisState>>;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        tracing::error!("thesis worker failed: {e}");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
    })
}

async fn summary(State(st): St) -> ApiResult {
    let v = blocking(move || {
        st.cached("summary", || {
            dossier::summary(&st.stock_root(), &|t| st.articles_for(t), now_secs())
        })
    })
    .await?;
    Ok(Json(v))
}

async fn macro_lens(State(st): St) -> ApiResult {
    let v =
        blocking(move || st.cached("macro", || lens::build(&st.vault_root, now_secs()))).await?;
    Ok(Json(v))
}

async fn detail(State(st): St, UrlPath(ticker): UrlPath<String>) -> ApiResult {
    let t = crate::py::strip(&ticker).to_uppercase();
    let key = format!("detail:{t}");
    let tt = t.clone();
    let v = blocking(move || {
        // `None` (no dossier) is cached by MC too; represent it as JSON null.
        st.cached(&key, || {
            dossier::detail(&st.stock_root(), &|x| st.articles_for(x), &tt).unwrap_or(Value::Null)
        })
    })
    .await?;
    if v.is_null() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("Dossier {t} tidak ditemukan di finance/stock"),
        ));
    }
    Ok(Json(v))
}

async fn list_drafts(State(st): St, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let ticker = q
        .get("ticker")
        .filter(|t| !t.is_empty())
        .map(|t| crate::py::strip(t).to_uppercase());
    let v = blocking(move || -> Result<Value, ApiError> {
        let store = drafts::load(&st.drafts_path)?;
        let mut list: Vec<Value> = store.into_iter().map(|(_, v)| v).collect();
        if let Some(t) = ticker {
            list.retain(|d| d.get("ticker").and_then(Value::as_str) == Some(t.as_str()));
        }
        let created = |d: &Value| {
            d.get("created_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        list.sort_by_key(|d| std::cmp::Reverse(created(d)));
        let vault = drafts::vault_theses(&st.vault_thesis_dir());
        let total = list.len() + vault.len();
        Ok(json!({"drafts": list, "vault_drafts": vault, "total": total}))
    })
    .await??;
    Ok(Json(v))
}

async fn list_vault_drafts(State(st): St) -> ApiResult {
    let vault = blocking(move || drafts::vault_theses(&st.vault_thesis_dir())).await?;
    Ok(Json(
        json!({"total": vault.len(), "vault_drafts": vault, "is_readonly": true}),
    ))
}

fn not_found(id: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, format!("Draf {id} tidak ditemukan."))
}

async fn get_draft(State(st): St, UrlPath(id): UrlPath<String>) -> ApiResult {
    let v = blocking(move || -> Result<Value, ApiError> {
        let store = drafts::load(&st.drafts_path)?;
        if let Some(d) = store.get(&id) {
            return Ok(d.clone());
        }
        drafts::vault_theses(&st.vault_thesis_dir())
            .into_iter()
            .find(|vd| {
                vd["id"].as_str() == Some(id.as_str())
                    || vd["file_info"]["name"].as_str() == Some(id.as_str())
            })
            .ok_or_else(|| not_found(&id))
    })
    .await??;
    Ok(Json(v))
}

fn save_failed(e: std::io::Error) -> ApiError {
    tracing::error!("cannot write thesis drafts store: {e}");
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Drafts store corrupted or unavailable",
    )
}

async fn create_draft(State(st): St, body: axum::body::Bytes) -> Result<Response, ApiError> {
    let body: Value = serde_json::from_slice(&body).map_err(|_| {
        ApiError::invalid(&["body"], "json_invalid", "JSON decode error", json!({}))
    })?;
    let now = chrono::Utc::now();
    let iso = now.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
    let draft = drafts::build_draft(&body, now.timestamp() as u64, &iso, &suffix)?;

    let _guard = st.drafts_lock.lock().await;
    let path = st.drafts_path.clone();
    let record = draft.clone();
    blocking(move || -> Result<(), ApiError> {
        let mut store = drafts::load(&path)?;
        store.insert(
            record["id"].as_str().unwrap_or_default().to_string(),
            record,
        );
        drafts::save(&path, &store).map_err(save_failed)
    })
    .await??;
    Ok((StatusCode::CREATED, Json(draft)).into_response())
}

/// prd.md A3.3: 204 No Content (MC answers 200 with a small JSON body).
async fn delete_draft(State(st): St, UrlPath(id): UrlPath<String>) -> Result<Response, ApiError> {
    let _guard = st.drafts_lock.lock().await;
    let path = st.drafts_path.clone();
    blocking(move || -> Result<(), ApiError> {
        let mut store = drafts::load(&path)?;
        if store.shift_remove(&id).is_none() {
            return Err(not_found(&id));
        }
        drafts::save(&path, &store).map_err(save_failed)
    })
    .await??;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub fn router(cfg: Arc<Config>) -> Router {
    let state = Arc::new(ThesisState::new(&cfg));
    Router::new()
        .route("/api/thesis/summary", get(summary))
        .route("/api/thesis/macro", get(macro_lens))
        .route("/api/thesis/drafts", get(list_drafts).post(create_draft))
        .route("/api/thesis/drafts/vault", get(list_vault_drafts))
        .route(
            "/api/thesis/drafts/{draft_id}",
            get(get_draft).delete(delete_draft),
        )
        .route("/api/thesis/{ticker}", get(detail))
        .with_state(state)
}
