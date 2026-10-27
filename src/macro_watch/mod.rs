//! Area 5 (macro watchdog): `/api/macro/{registry,observations,scoring,trigger,stream}`,
//! ported from MC `backend/api/macro.py` and `backend/services/macro_ingestion.py` /
//! `macro_scoring.py`. (`macro` is a reserved word in Rust, hence `macro_watch`.)
//!
//! The store (`INVESTING_MACRO_STORE_PATH`) is app-owned: a derived cache rebuilt
//! from the same public upstreams MC uses (FRED, Yahoo Finance, open.er-api,
//! BI, stats.gov.cn). A background watcher re-ingests every 60 s like MC's
//! `run_macro_release_watcher`. `/api/thesis/macro` lives in `crate::thesis`.

pub mod ingest;
pub mod observe;
pub mod scoring;
pub mod upstream;

use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Mutex};

use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use crate::py::isoformat;
use ingest::Clients;
use observe::{Store, REGISTRY, REGISTRY_JSON};
use upstream::Endpoints;

const WATCH_INTERVAL: Duration = Duration::from_secs(60);
const SSE_PING_AFTER: Duration = Duration::from_secs(15);

pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct MacroState {
    store_path: PathBuf,
    store: Mutex<Store>,
    /// One ingestion cycle at a time (watcher vs. trigger).
    cycle: Mutex<()>,
    clients: Clients,
    events: broadcast::Sender<(String, Value)>,
    env_disable_fred: bool,
    clock: Clock,
}

impl MacroState {
    pub fn new(cfg: &Config, endpoints: Endpoints, clock: Clock) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            store: Mutex::new(observe::load_store(&cfg.macro_store_path)),
            store_path: cfg.macro_store_path.clone(),
            cycle: Mutex::new(()),
            clients: Clients::new(endpoints),
            events,
            env_disable_fred: cfg.macro_disable_fred,
            clock,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    fn now_iso(&self) -> String {
        isoformat(&self.now().fixed_offset())
    }

    fn broadcast(&self, event: &str, data: Value) {
        // No subscribers is fine: MC also drops events nobody listens to.
        let _ = self.events.send((event.to_string(), data));
    }

    pub async fn observations(&self, fred_isolated: bool, sandbox: bool) -> Vec<Value> {
        let today = self.now().date_naive();
        if sandbox {
            return observe::observations(today, fred_isolated, &Store::new());
        }
        observe::observations(today, fred_isolated, &*self.store.lock().await)
    }

    /// `fetch_canonical_observations_async(run_ingest=True)`; returns the ids
    /// of observations that changed (each is broadcast as an SSE `update`).
    pub async fn ingest(&self, disable_fred: bool) -> Vec<String> {
        let _cycle = self.cycle.lock().await;
        let now = self.now();
        let results = ingest::fetch_all(&self.clients, disable_fred, now).await;
        let (changed, snapshot) = {
            let mut store = self.store.lock().await;
            let changed = ingest::apply_all(&mut store, &results);
            observe::save_store(&self.store_path, &store);
            (changed, store.clone())
        };
        let today = now.date_naive();
        for reg in REGISTRY.iter().filter(|r| changed.contains(&r.metric_id)) {
            let o = observe::build_observation(reg, today, disable_fred, &snapshot);
            self.broadcast(
                "update",
                json!({
                    "event": "update", "metric_id": o["metric_id"], "series": o["series"], "value": o["value"],
                    "observed_at": o["observed_at"], "freshness": o["freshness"], "quality_note": o["quality_note"],
                    "timestamp": self.now_iso(),
                }),
            );
        }
        changed
    }
}

type St = State<Arc<MacroState>>;
type Params = Query<HashMap<String, String>>;

/// Pydantic lax bool for a query flag; absent → false.
fn query_flag(p: &HashMap<String, String>, name: &str) -> Result<bool, ApiError> {
    match p.get(name).map(|v| v.trim().to_ascii_lowercase()) {
        None => Ok(false),
        Some(v) => match v.as_str() {
            "1" | "on" | "t" | "true" | "y" | "yes" => Ok(true),
            "0" | "off" | "f" | "false" | "n" | "no" => Ok(false),
            _ => Err(ApiError::invalid(
                &["query", name],
                "bool_parsing",
                "Input should be a valid boolean, unable to interpret input",
                json!(p[name]),
            )),
        },
    }
}

fn header_flag(h: &HeaderMap, name: &str, accepted: &[&str]) -> bool {
    h.get(name)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| accepted.contains(&v.trim().to_lowercase().as_str()))
}

fn fred_flag(
    st: &MacroState,
    p: &HashMap<String, String>,
    h: &HeaderMap,
) -> Result<bool, ApiError> {
    let flag = query_flag(p, "disable_fred")?
        || header_flag(h, "x-macro-disable-fred", &["1", "true", "yes", "isolated"]);
    Ok(flag || st.env_disable_fred)
}

fn sandbox_flag(p: &HashMap<String, String>, h: &HeaderMap) -> Result<bool, ApiError> {
    Ok(query_flag(p, "sandbox_empty_store")?
        || header_flag(
            h,
            "x-macro-sandbox-empty-store",
            &["1", "true", "yes", "sandbox"],
        ))
}

async fn registry() -> Json<Value> {
    Json(json!({"status": "ok", "total_metrics": REGISTRY_JSON.len(), "registry": *REGISTRY_JSON}))
}

async fn observations(State(st): St, Query(p): Params, h: HeaderMap) -> ApiResult {
    let sandbox = sandbox_flag(&p, &h)?;
    let fred = fred_flag(&st, &p, &h)?;
    let now_iso = st.now_iso();
    let obs = st.observations(fred, sandbox).await;
    Ok(Json(json!({
        "status": "ok", "as_of": now_iso, "fred_isolated": fred, "sandbox_empty_store": sandbox,
        "total_metrics": obs.len(), "observations": obs,
    })))
}

async fn scoring_endpoint(State(st): St, Query(p): Params, h: HeaderMap) -> ApiResult {
    let vix = query_flag(&p, "disable_vix")?
        || header_flag(&h, "x-macro-disable-vix", &["1", "true", "yes", "cut"]);
    let hy = query_flag(&p, "disable_hy")?
        || header_flag(&h, "x-macro-disable-hy", &["1", "true", "yes", "cut"]);
    let sandbox = sandbox_flag(&p, &h)?;
    let fred = fred_flag(&st, &p, &h)?;
    let mut disabled = BTreeSet::new();
    if vix {
        disabled.insert("vix".to_string());
    }
    if hy {
        disabled.insert("us_hy_oas".to_string());
    }
    for m in p
        .get("disable_metrics")
        .map(String::as_str)
        .unwrap_or("")
        .split(',')
    {
        if !m.trim().is_empty() {
            disabled.insert(m.trim().to_string());
        }
    }
    let now_iso = st.now_iso();
    let obs = st.observations(fred, sandbox).await;
    let result = scoring::calculate(&obs, &disabled, &st.now_iso());
    let mut out = json!({"status": "ok", "as_of": now_iso, "fred_isolated": fred, "sandbox_empty_store": sandbox});
    out.as_object_mut()
        .expect("object")
        .extend(result.as_object().cloned().expect("object"));
    Ok(Json(out))
}

async fn trigger(State(st): St, Query(p): Params, h: HeaderMap) -> ApiResult {
    let fred = fred_flag(&st, &p, &h)?;
    st.ingest(fred).await;
    let total = REGISTRY.len();
    st.broadcast(
        "update",
        json!({"event": "update", "trigger": "release_cycle_executed", "total_metrics": total, "timestamp": st.now_iso()}),
    );
    Ok(Json(json!({
        "status": "ok",
        "message": "Macro release check executed, store updated, and broadcasted to active SSE listeners",
        "total_metrics": total,
        "fred_isolated": fred,
    })))
}

fn event(name: &str, data: &Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

async fn stream(
    State(st): St,
    Query(p): Params,
    h: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let fred = fred_flag(&st, &p, &h)?;
    // Subscribe before the snapshot so no update between the two is lost.
    let rx = st.events.subscribe();
    let obs = st.observations(fred, false).await;
    let init = json!({"event": "init", "fred_isolated": fred, "total_metrics": obs.len(), "observations": obs});
    let first = futures::stream::once(async move { Ok(event("init", &init)) });
    let rest = futures::stream::unfold((rx, st), |(mut rx, st)| async move {
        loop {
            match tokio::time::timeout(SSE_PING_AFTER, rx.recv()).await {
                Ok(Ok((name, data))) => return Some((Ok(event(&name, &data)), (rx, st))),
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => return None,
                Err(_) => {
                    let ping = json!({"time": st.now_iso()});
                    return Some((Ok(event("ping", &ping)), (rx, st)));
                }
            }
        }
    });
    Ok(Sse::new(futures::StreamExt::chain(first, rest)))
}

/// MC `run_macro_release_watcher`: ingest, announce, wait 60 s, repeat.
/// A failing cycle is logged and retried next round instead of stopping.
pub fn spawn_watcher(st: Arc<MacroState>) {
    tokio::spawn(async move {
        tracing::info!("macro release watcher started");
        loop {
            st.ingest(st.env_disable_fred).await;
            st.broadcast("update", json!({"event": "update", "trigger": "scheduled_release_cycle", "timestamp": st.now_iso()}));
            tokio::time::sleep(WATCH_INTERVAL).await;
        }
    });
}

pub fn router_with(state: Arc<MacroState>) -> Router {
    Router::new()
        .route("/api/macro/registry", get(registry))
        .route("/api/macro/observations", get(observations))
        .route("/api/macro/scoring", get(scoring_endpoint))
        .route("/api/macro/trigger", post(trigger))
        .route("/api/macro/stream", get(stream))
        .with_state(state)
}

pub fn router(cfg: Arc<Config>) -> Router {
    let state = Arc::new(MacroState::new(
        &cfg,
        Endpoints::default(),
        Arc::new(Utc::now),
    ));
    if cfg.macro_watcher {
        spawn_watcher(state.clone());
    }
    router_with(state)
}
