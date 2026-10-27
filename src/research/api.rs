//! `/api/research/*` (plan §7, §8.1 B13). Auth is applied by the app-wide middleware; every
//! write needs an `Idempotency-Key` (cookie sessions also need `X-Finance-Client`, which the
//! same middleware enforces). A key seen before with the same request replays the stored
//! response; the same key with a different request is a 409. The journal is durable, so a
//! retry after a restart does not repeat the effect.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::UnboundedSender;

use super::engine::Engine;
use super::events;
use super::kpi::{self, KpiValue};
use super::packs::{payload, PackSet, Packs};
use super::prices::payload as prices_payload;
use super::store::ManualShares;
use super::universe::{self, UniverseEntry, UniverseError};
use super::valuation::{self, FcfBase, LoadedRules};
use crate::error::ApiError;
use crate::idempotency::{Entry, IdempotencyStore};

mod lookup;
mod r4;
mod r5;
mod r8;
mod r9;

const REFRESH_EVERY_SECS: i64 = 60;
const MAX_SOURCE_NOTE_CHARS: usize = 200;
const MAX_KEY_LEN: usize = 255;

pub struct Options {
    /// Durable `Idempotency-Key` journal.
    pub idem_path: PathBuf,
    /// Scraper watchlist rewritten after a universe change.
    pub watchlist: Option<PathBuf>,
    /// Receives `None` (all) or one ticker when a refresh is accepted.
    pub refresh_tx: Option<UnboundedSender<Option<String>>>,
    /// Loaded pack definitions (research plan §4.3).
    pub packs: Arc<Packs>,
    /// Directory holding `rules.toml` (research plan §4.6, R3.4); `None` uses the embedded
    /// rules and never reloads.
    pub rules_dir: Option<PathBuf>,
    /// Where uploaded documents are kept, `<sha256>.<ext>` each (R8); created on the first
    /// upload.
    pub documents_dir: PathBuf,
}

#[derive(Clone)]
pub struct ApiState {
    engine: Option<Arc<Engine>>,
    idem: Arc<Mutex<Option<IdempotencyStore>>>,
    watchlist: Option<PathBuf>,
    refresh_tx: Option<UnboundedSender<Option<String>>>,
    refreshed: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
    packs: Arc<Packs>,
    rules: Arc<RwLock<LoadedRules>>,
    rules_dir: Option<PathBuf>,
    documents_dir: PathBuf,
}

impl ApiState {
    pub fn new(engine: Option<Arc<Engine>>, opts: Options) -> Self {
        let now = engine.as_ref().map_or_else(Utc::now, |e| e.now());
        let idem = match IdempotencyStore::open(&opts.idem_path, now.timestamp().max(0) as u64) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::error!("research idempotency journal: {e}");
                None
            }
        };
        let now_s = now.to_rfc3339();
        let rules = match &opts.rules_dir {
            Some(dir) => LoadedRules::load(dir, &now_s),
            None => LoadedRules::embedded(&now_s),
        };
        if let Some(e) = &rules.error {
            tracing::warn!("research rules: {e}; embedded rules in use");
        }
        Self {
            engine,
            idem: Arc::new(Mutex::new(idem)),
            watchlist: opts.watchlist,
            refresh_tx: opts.refresh_tx,
            refreshed: Arc::default(),
            packs: opts.packs,
            rules: Arc::new(RwLock::new(rules)),
            rules_dir: opts.rules_dir,
            documents_dir: opts.documents_dir,
        }
    }

    fn rules(&self) -> LoadedRules {
        self.rules.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn pack_set(&self) -> Arc<PackSet> {
        self.packs.set()
    }

    fn engine(&self) -> Result<&Arc<Engine>, ApiError> {
        self.engine.as_ref().ok_or_else(unavailable)
    }
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Research store unavailable",
    )
}

fn store_error(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("research api: {e}");
    unavailable()
}

/// Upper-cases and checks the shape; anything else cannot be a universe ticker (404).
fn ticker_of(raw: &str) -> Result<String, ApiError> {
    universe::normalize_ticker(raw)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))
}

fn json_body(body: &Bytes) -> Result<Value, ApiError> {
    serde_json::from_slice(body).map_err(|e| {
        ApiError::invalid(
            &["body"],
            "json_invalid",
            format!("JSON decode error: {e}"),
            Value::Null,
        )
    })
}

fn field_error(name: &str, kind: &str, msg: &str, input: &Value) -> ApiError {
    ApiError::invalid(&["body", name], kind, msg, input.clone())
}

fn reply(status: StatusCode, body: Value) -> Response {
    if status == StatusCode::NO_CONTENT {
        status.into_response()
    } else {
        (status, Json(body)).into_response()
    }
}

fn body_hash(method: &str, path: &str, body: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(method.as_bytes());
    h.update(b" ");
    h.update(path.as_bytes());
    h.update(b"\n");
    h.update(body);
    hex::encode(h.finalize())
}

/// Runs `effect` once per `Idempotency-Key`. `effect` returns the status and JSON body to
/// store; failures are not recorded, so a retry runs the effect again.
fn idempotent(
    st: &ApiState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &Bytes,
    effect: impl FnOnce(&str) -> Result<(StatusCode, Value), ApiError>,
) -> Result<Response, ApiError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|k| !k.is_empty() && k.len() <= MAX_KEY_LEN)
        .ok_or_else(|| {
            ApiError::invalid(
                &["header", "idempotency-key"],
                "missing",
                "Field required",
                Value::Null,
            )
        })?;
    let fingerprint = body_hash(method, path, body);
    let now = st.engine()?.now().timestamp().max(0) as u64;
    let mut guard = st.idem.lock().unwrap_or_else(|p| p.into_inner());
    let journal = guard.as_mut().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Idempotency store unavailable",
        )
    })?;
    match journal.get(key, now) {
        Some(Entry::Done {
            fingerprint: fp,
            status,
            body,
        }) => {
            if *fp != fingerprint {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "Idempotency-Key was already used for a different request",
                ));
            }
            let status = StatusCode::from_u16(*status).unwrap_or(StatusCode::OK);
            let mut res = reply(status, body.clone());
            res.headers_mut()
                .insert("idempotent-replay", HeaderValue::from_static("true"));
            return Ok(res);
        }
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Idempotency-Key is already in use",
            ))
        }
        None => {}
    }
    let (status, out) = effect(key)?;
    if let Err(e) = journal.record(
        key,
        Entry::Done {
            fingerprint,
            status: status.as_u16(),
            body: out.clone(),
        },
        now,
    ) {
        tracing::error!("research idempotency journal write failed: {e}");
    }
    Ok(reply(status, out))
}

/// Rounds every non-integer number to four decimals, so responses carry readable figures
/// and not the full float tail.
fn rounded(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => n
            .as_f64()
            .and_then(|x| serde_json::Number::from_f64((x * 1e4).round() / 1e4))
            .map_or(Value::Null, Value::Number),
        Value::Array(a) => Value::Array(a.into_iter().map(rounded).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (k, rounded(x))).collect()),
        other => other,
    }
}

fn out(v: Value) -> Json<Value> {
    Json(rounded(v))
}

// ---- reads ----

/// `GET /api/research/rules` (research plan §5, design.md §4.4 Rules card): the canon
/// rule set with its version and changelog.
async fn rules_canon(State(st): State<ApiState>) -> Json<Value> {
    Json(st.rules().json(super::derive::RULES_VERSION))
}

#[derive(serde::Deserialize)]
struct ValuationQuery {
    fcf: Option<String>,
}

/// The ticker as the universe table knows it (404 otherwise).
fn canon_ticker(st: &ApiState, raw: &str) -> Result<String, ApiError> {
    let ticker = ticker_of(raw)?;
    st.engine()?
        .store
        .universe_entry(&ticker)
        .map_err(store_error)?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!("{ticker} is not in the universe"),
            )
        })?;
    Ok(ticker)
}

fn valuation_inputs(
    st: &ApiState,
    raw: &str,
) -> Result<(String, valuation::Inputs, valuation::Rules), ApiError> {
    let ticker = canon_ticker(st, raw)?;
    let engine = st.engine()?;
    let store = &engine.store;
    let rules = st.rules().rules;
    let inputs = valuation::Inputs::load(store, &st.pack_set(), &ticker, engine.today(), &rules)
        .map_err(store_error)?;
    Ok((ticker, inputs, rules))
}

/// `GET /api/research/{ticker}/prices?range=1y|3y|5y|10y` (research plan §5, R3.5;
/// design.md §4.2 Harga).
#[derive(serde::Deserialize)]
struct PricesQuery {
    range: Option<String>,
}

async fn prices_canon(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<PricesQuery>,
) -> Result<Json<Value>, ApiError> {
    let range = match q.range.as_deref() {
        None => prices_payload::Range::Y1,
        Some(s) => prices_payload::Range::parse(s).ok_or_else(|| {
            ApiError::invalid(
                &["query", "range"],
                "enum",
                "Input should be '1y', '3y', '5y' or '10y'",
                json!(s),
            )
        })?,
    };
    let ticker = canon_ticker(&st, &raw)?;
    let engine = st.engine()?;
    let store = &engine.store;
    let entry = store
        .universe_entry(&ticker)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
    let inputs = prices_payload::PriceInputs::load(store, &st.pack_set(), &entry, engine.today())
        .map_err(store_error)?;
    Ok(out(prices_payload::build(
        &inputs,
        range,
        engine.today(),
        &engine.now().to_rfc3339(),
    )))
}

/// `GET /api/research/{ticker}/valuation?fcf=ttm|avg5y` (design.md §4.2 Valuasi).
async fn valuation_get(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<ValuationQuery>,
) -> Result<Json<Value>, ApiError> {
    let fcf = match q.fcf.as_deref() {
        None => FcfBase::Ttm,
        Some(s) => FcfBase::parse(s).ok_or_else(|| {
            ApiError::invalid(
                &["query", "fcf"],
                "enum",
                "Input should be 'ttm' or 'avg5y'",
                json!(s),
            )
        })?,
    };
    let (_, inputs, rules) = valuation_inputs(&st, &raw)?;
    let now = st.engine()?.now().to_rfc3339();
    Ok(out(valuation::build(&inputs, &rules, fcf, &now)))
}

fn scenario_number(
    v: &Value,
    name: &str,
    key: &str,
    lo: f64,
    hi: f64,
    exclusive_lo: bool,
) -> Result<Option<f64>, ApiError> {
    let x = &v[key];
    if x.is_null() {
        return Ok(None);
    }
    let field = format!("{name}.{key}");
    let n = x
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| field_error(&field, "float_parsing", "Input should be a number", x))?;
    let ok = if exclusive_lo { n > lo } else { n >= lo } && n <= hi;
    if !ok {
        return Err(field_error(
            &field,
            "range",
            &format!("Input should be between {lo} and {hi}"),
            x,
        ));
    }
    Ok(Some(n))
}

/// `PUT /api/research/{ticker}/scenarios` (A3.4): `{bear|base|bull: {growth, margin,
/// exit_multiple, years} | null}`; absent names are untouched, `null` clears one, partial
/// inputs are kept and the value per share appears once all four exist.
async fn scenarios_put(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/scenarios");
    idempotent(&st, &headers, "PUT", &path, &body, |_| {
        let v = json_body(&body)?;
        let obj = v
            .as_object()
            .ok_or_else(|| field_error("body", "object", "Input should be an object", &v))?;
        for key in obj.keys() {
            if !["bear", "base", "bull"].contains(&key.as_str()) {
                return Err(field_error(
                    key,
                    "unknown",
                    "Scenario names are bear, base and bull",
                    &obj[key],
                ));
            }
        }
        let engine = st.engine()?;
        let now = engine.now().to_rfc3339();
        // The whole body is validated before anything is written (a 422 never leaves a
        // partial write), and unknown row fields are refused rather than read as null.
        let mut writes: Vec<(&str, Option<super::store::ScenarioRow>)> = Vec::new();
        for name in ["bear", "base", "bull"] {
            let Some(s) = obj.get(name) else { continue };
            if s.is_null() {
                writes.push((name, None));
                continue;
            }
            let row = s.as_object().ok_or_else(|| {
                field_error(name, "object", "Input should be an object or null", s)
            })?;
            for key in row.keys() {
                if !["growth", "margin", "exit_multiple", "years"].contains(&key.as_str()) {
                    return Err(field_error(
                        &format!("{name}.{key}"),
                        "unknown",
                        "Scenario fields are growth, margin, exit_multiple and years",
                        &row[key],
                    ));
                }
            }
            let years = match &s["years"] {
                Value::Null => None,
                y => Some(y.as_u64().filter(|y| (1..=20).contains(y)).ok_or_else(|| {
                    field_error(
                        &format!("{name}.years"),
                        "range",
                        "Input should be an integer between 1 and 20",
                        y,
                    )
                })? as u32),
            };
            writes.push((
                name,
                Some(super::store::ScenarioRow {
                    ticker: ticker.clone(),
                    name: name.into(),
                    growth: scenario_number(s, name, "growth", -50.0, 100.0, false)?,
                    margin: scenario_number(s, name, "margin", -100.0, 100.0, false)?,
                    exit_multiple: scenario_number(s, name, "exit_multiple", 0.0, 100.0, true)?,
                    years,
                    updated_at: now.clone(),
                }),
            ));
        }
        for (name, row) in writes {
            match row {
                None => engine
                    .store
                    .delete_scenario(&ticker, name, &now)
                    .map_err(store_error)?,
                Some(r) => engine.store.set_scenario(&r).map_err(store_error)?,
            }
        }
        let (_, inputs, rules) = valuation_inputs(&st, &raw)?;
        let v = valuation::build(&inputs, &rules, FcfBase::Ttm, &now);
        Ok((
            StatusCode::OK,
            rounded(
                json!({"ticker": ticker, "price": v["price"], "shares": v["shares"], "scenarios": v["scenarios"], "rules_version": v["rules_version"]}),
            ),
        ))
    })
}

// ---- writes ----

async fn seen(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/seen");
    idempotent(&st, &headers, "POST", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        engine
            .store
            .set_seen(&ticker, &engine.now().to_rfc3339())
            .map_err(store_error)?;
        Ok((StatusCode::NO_CONTENT, Value::Null))
    })
}

async fn put_shares(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/shares");
    idempotent(&st, &headers, "PUT", &path, &body, |_| {
        let v = json_body(&body)?;
        let shares = v["shares"]
            .as_f64()
            .filter(|s| s.is_finite() && *s > 0.0)
            .ok_or_else(|| {
                field_error(
                    "shares",
                    "greater_than",
                    "Input should be greater than 0",
                    &v["shares"],
                )
            })?;
        let as_of = v["as_of"]
            .as_str()
            .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
            .ok_or_else(|| {
                field_error(
                    "as_of",
                    "date_parsing",
                    "Input should be a date (YYYY-MM-DD)",
                    &v["as_of"],
                )
            })?;
        let note = v["source_note"].as_str().map(str::trim).unwrap_or_default();
        let len = note.chars().count();
        if len == 0 || len > MAX_SOURCE_NOTE_CHARS {
            return Err(field_error(
                "source_note",
                "string_length",
                "Source note must be 1 to 200 characters",
                &v["source_note"],
            ));
        }
        let entered_at = st.engine()?.now().to_rfc3339();
        st.engine()?
            .store
            .set_shares_manual(&ManualShares {
                ticker: ticker.clone(),
                shares,
                as_of: as_of.to_string(),
                source_note: note.to_string(),
                entered_at: entered_at.clone(),
            })
            .map_err(store_error)?;
        Ok((
            StatusCode::OK,
            json!({"ticker": ticker, "shares": shares, "as_of": as_of, "source_note": note, "entered_at": entered_at}),
        ))
    })
}

#[derive(serde::Deserialize)]
struct SharesDeleteQuery {
    as_of: Option<String>,
}

/// `DELETE /{ticker}/shares[?as_of=YYYY-MM-DD]`: one dated count, or every count of the
/// ticker when `as_of` is absent (the original contract).
async fn delete_shares(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<SharesDeleteQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let as_of = match q.as_of.as_deref() {
        None => None,
        Some(d) => Some(
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map(|_| d.to_string())
                .map_err(|_| {
                    ApiError::invalid(
                        &["query", "as_of"],
                        "date_parsing",
                        "Input should be a date (YYYY-MM-DD)",
                        json!(d),
                    )
                })?,
        ),
    };
    let path = match &as_of {
        Some(d) => format!("/api/research/{ticker}/shares?as_of={d}"),
        None => format!("/api/research/{ticker}/shares"),
    };
    idempotent(&st, &headers, "DELETE", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        let now = engine.now().to_rfc3339();
        match &as_of {
            Some(d) => engine.store.delete_shares_manual_at(&ticker, d, &now),
            None => engine.store.delete_shares_manual(&ticker, &now),
        }
        .map_err(store_error)?;
        Ok((StatusCode::NO_CONTENT, Value::Null))
    })
}

fn map_universe(e: UniverseError) -> ApiError {
    match e {
        UniverseError::Invalid => ApiError::invalid(
            &["body", "ticker"],
            "ticker_invalid",
            "Ticker must be four letters A-Z",
            Value::Null,
        ),
        UniverseError::Held(t) => ApiError::new(
            StatusCode::CONFLICT,
            format!("{t} is held in the portfolio and cannot be removed"),
        ),
        UniverseError::NotFound(t) => {
            ApiError::new(StatusCode::NOT_FOUND, format!("{t} is not in the universe"))
        }
        UniverseError::TooManyPeers => ApiError::invalid(
            &["body", "peers"],
            "too_many",
            format!("At most {} peers per ticker", universe::MAX_PEERS),
            Value::Null,
        ),
        UniverseError::SelfPeer => ApiError::invalid(
            &["body", "peers"],
            "self_peer",
            "A ticker cannot be its own peer",
            Value::Null,
        ),
        UniverseError::NoteTooLong => ApiError::invalid(
            &["body", "business_note"],
            "string_length",
            format!(
                "Business note must be at most {} characters",
                universe::MAX_NOTE_CHARS
            ),
            Value::Null,
        ),
        UniverseError::UnknownPack(id) => ApiError::invalid(
            &["body", "pack_override"],
            "unknown_pack",
            "Unknown pack id",
            json!(id),
        ),
        UniverseError::Store(e) => store_error(e),
    }
}

/// One `GET /universe` item: the row, its pack choice (override next to the filing-derived
/// default), peers and the average cost when held.
fn universe_entry_json(
    e: &UniverseEntry,
    packs: &PackSet,
    filing_codes: &HashMap<String, String>,
    peers: &std::collections::BTreeMap<String, Vec<String>>,
    avg_price: Option<f64>,
) -> Value {
    let choice = universe::pack_choice(packs, e, filing_codes.get(&e.ticker).map(String::as_str));
    json!({
        "ticker": e.ticker,
        "role": e.role.as_str(),
        "peer_of": e.peer_of,
        "pack": choice,
        "business_note": e.business_note,
        "added_at": e.added_at,
        "synced_at": e.synced_at,
        "peers": peers.get(&e.ticker).cloned().unwrap_or_default(),
        "avg_price": avg_price,
    })
}

fn universe_items(st: &ApiState) -> Result<Vec<Value>, ApiError> {
    let store = store_of(st)?;
    let entries = store.universe().map_err(store_error)?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let peers = universe::peer_map(&entries);
    let packs = st.pack_set();
    entries
        .iter()
        .map(|e| {
            let avg = if e.role == universe::Role::Held {
                store.avg_price(&e.ticker).map_err(store_error)?
            } else {
                None
            };
            Ok(universe_entry_json(e, &packs, &codes, &peers, avg))
        })
        .collect()
}

fn universe_item(st: &ApiState, ticker: &str) -> Result<Value, ApiError> {
    universe_items(st)?
        .into_iter()
        .find(|i| i["ticker"] == ticker)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!("{ticker} is not in the universe"),
            )
        })
}

// ---- operating KPIs (research plan §5, R2.3; prd.md A2.3) ----

/// Re-evaluates the open claims of `ticker` and runs the inbox producers after its data
/// changed (A4.1, A4.3). A failure is logged, never returned: the write that triggered it
/// already happened.
fn reevaluate_claims(st: &ApiState, ticker: &str) {
    let Ok(engine) = st.engine() else { return };
    if let Err(e) = events::react::react(
        &engine.store,
        &st.pack_set(),
        &st.rules().rules.pojk_offsets,
        engine.now(),
        engine.today(),
        events::react::Scope::Ticker(ticker),
    ) {
        tracing::warn!("research react {ticker}: {e}");
    }
}

fn map_kpi_field(e: &kpi::FieldError) -> ApiError {
    ApiError::invalid(&["body", e.field], e.kind, &e.message, e.input.clone())
}

/// The pack in effect for `ticker` and its KPI metric `metric`, or 404 when the ticker is
/// not in the universe, has no pack, or the pack has no such KPI.
fn kpi_target(
    st: &ApiState,
    ticker: &str,
    metric: &str,
) -> Result<(Arc<PackSet>, String, String), ApiError> {
    let store = store_of(st)?;
    let entry = store
        .universe_entry(ticker)
        .map_err(store_error)?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!("{ticker} is not in the universe"),
            )
        })?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let packs = st.pack_set();
    let choice = universe::pack_choice(&packs, &entry, codes.get(ticker).map(String::as_str));
    let pack_id = choice.id.ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            format!("{ticker} has no pack; set a pack override first"),
        )
    })?;
    let pack = packs
        .get(&pack_id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown pack"))?;
    let (owner, m) = kpi::kpi_metric(&packs, pack, metric).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            format!("{metric} is not a KPI of pack {pack_id}"),
        )
    })?;
    let (owner_id, metric_id) = (owner.id.clone(), m.id.clone());
    Ok((packs, owner_id, metric_id))
}

/// `PUT /api/research/{ticker}/kpi/{metric}/{fy}/{q}` with body
/// `{value|unknown, document, document_date}`; replaces the cell and logs the write.
async fn put_kpi(
    State(st): State<ApiState>,
    Path((raw, metric, fy, q)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = ticker_of(&raw)?;
    let path = format!("/api/research/{ticker}/kpi/{metric}/{fy}/{q}");
    idempotent(&st, &headers, "PUT", &path, &body, |_| {
        let (packs, pack_id, metric_id) = kpi_target(&st, &ticker, &metric)?;
        let m = packs
            .get(&pack_id)
            .and_then(|p| p.metric(&metric_id))
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown metric"))?;
        let cadence = m.cadence.unwrap_or(super::packs::Cadence::Quarterly);
        let (fy_n, q_n) = kpi::parse_period(cadence, &fy, &q).map_err(|e| map_kpi_field(&e))?;
        let input = kpi::parse_body(&json_body(&body)?).map_err(|e| map_kpi_field(&e))?;
        let engine = st.engine()?;
        let row = KpiValue {
            ticker: ticker.clone(),
            metric: metric_id,
            fy: fy_n,
            q: q_n,
            value: input.value,
            unknown: input.unknown,
            unit: m.unit.clone(),
            document: input.document,
            document_date: input.document_date,
            entered_at: engine.now().to_rfc3339(),
        };
        engine.store.set_kpi(&row).map_err(store_error)?;
        reevaluate_claims(&st, &ticker);
        let mut out = row.to_json();
        out["ticker"] = json!(ticker);
        out["metric"] = json!(row.metric);
        out["pack"] = json!(pack_id);
        Ok((StatusCode::OK, out))
    })
}

/// `DELETE /api/research/{ticker}/kpi/{metric}/{fy}/{q}`: back to "not entered".
async fn delete_kpi(
    State(st): State<ApiState>,
    Path((raw, metric, fy, q)): Path<(String, String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = ticker_of(&raw)?;
    let path = format!("/api/research/{ticker}/kpi/{metric}/{fy}/{q}");
    idempotent(&st, &headers, "DELETE", &path, &Bytes::new(), |_| {
        // A stored row can always be removed, also after the pack in effect changed; the
        // period is parsed as quarterly so an annual row keyed at Q4 is reachable too.
        let engine = st.engine()?;
        if engine
            .store
            .universe_entry(&ticker)
            .map_err(store_error)?
            .is_none()
        {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                format!("{ticker} is not in the universe"),
            ));
        }
        let (fy_n, q_n) = kpi::parse_period(super::packs::Cadence::Quarterly, &fy, &q)
            .map_err(|e| map_kpi_field(&e))?;
        if engine
            .store
            .kpi(&ticker, &metric, fy_n, q_n)
            .map_err(store_error)?
            .is_none()
        {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "KPI value not entered",
            ));
        }
        engine
            .store
            .delete_kpi(&ticker, &metric, fy_n, q_n, &engine.now().to_rfc3339())
            .map_err(store_error)?;
        reevaluate_claims(&st, &ticker);
        Ok((StatusCode::NO_CONTENT, Value::Null))
    })
}

/// `GET /api/research/{ticker}/pack` (design.md §4.2 Paket; A2.1 to A2.5): the pack
/// section payload with coverage.
async fn pack_section(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ticker = ticker_of(&raw)?;
    let engine = st.engine()?;
    let store = &engine.store;
    let entry = store
        .universe_entry(&ticker)
        .map_err(store_error)?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!("{ticker} is not in the universe"),
            )
        })?;
    let entries = store.universe().map_err(store_error)?;
    let peers = universe::peer_map(&entries)
        .remove(&ticker)
        .unwrap_or_default();
    let packs = st.pack_set();
    let v = payload::build(
        store,
        &packs,
        &entry,
        &peers,
        engine.today(),
        &engine.now().to_rfc3339(),
    )
    .map_err(store_error)?;
    Ok(Json(v))
}

fn packs_status_json(st: &ApiState) -> Value {
    let s = st.packs.status();
    json!({
        "source": s.source,
        "dir": st.packs.dir().display().to_string(),
        "files": s.files,
        "loaded": s.set.len(),
        "invalid": s.invalid.iter().map(|(f, e)| json!({"file": f, "error": e})).collect::<Vec<_>>(),
        "embedded_ids": s.embedded_ids,
        "loaded_at": s.loaded_at,
        "ok": s.ok(),
    })
}

/// `GET /api/research/packs` (design.md §4.4): definitions, members, load status.
async fn packs_list(State(st): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let store = store_of(&st)?;
    let packs = st.pack_set();
    let entries = store.universe().map_err(store_error)?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let mut members: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for e in &entries {
        let choice = universe::pack_choice(&packs, e, codes.get(&e.ticker).map(String::as_str));
        members
            .entry(super::packs::CORE_ID.to_string())
            .or_default()
            .push(e.ticker.clone());
        if let Some(id) = choice.id {
            members.entry(id).or_default().push(e.ticker.clone());
        }
    }
    let index = payload::driver_index(store).map_err(store_error)?;
    Ok(Json(json!({
        "status": packs_status_json(&st),
        "packs": payload::definitions_json(&packs, &members, &index),
        "driver_catalogue": super::prices::drivers::CATALOGUE.iter().map(|d| json!({
            "id": d.id, "label": d.label, "unit": d.unit, "cadence": d.cadence.as_str(),
            "source": d.source, "proxy_for": d.proxy_for, "note": d.note,
            "status": payload::driver_status(d.has_source(), index.get(d.id)),
        })).collect::<Vec<_>>(),
    })))
}

/// `POST /api/research/packs/reload` (A2.4, design.md §4.4 "Muat ulang"): re-reads the
/// pack directory without a restart. Not journaled: reloading twice is harmless.
async fn packs_reload(State(st): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let now = st.engine()?.now().to_rfc3339();
    let loaded = st.packs.reload(&now);
    if let Some(dir) = &st.rules_dir {
        let rules = LoadedRules::load(dir, &now);
        if let Some(e) = &rules.error {
            tracing::warn!("research rules reload: {e}; embedded rules in use");
        }
        *st.rules.write().unwrap_or_else(|p| p.into_inner()) = rules;
    }
    let status = packs_status_json(&st);
    let rules = st.rules();
    Ok(Json(json!({
        "status": status,
        "reloaded": loaded.set.len(),
        "rules": {"version": rules.rules.version, "source": rules.source, "error": rules.error},
    })))
}

/// `GET /api/research/universe` (research plan §5): the canon rows.
async fn universe_canon(State(st): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let items = universe_items(&st)?;
    Ok(Json(json!({
        "as_of": st.engine()?.now().to_rfc3339(),
        "items": items,
    })))
}

fn opt_string(v: &Value, key: &str) -> Result<Option<Option<String>>, ApiError> {
    match v.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
        Some(other) => Err(field_error(
            key,
            "string_type",
            "Input should be a string or null",
            other,
        )),
    }
}

fn string_list(v: &Value, key: &str) -> Result<Option<Vec<String>>, ApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| {
                i.as_str().map(str::to_string).ok_or_else(|| {
                    field_error(key, "string_type", "Input should be a list of tickers", i)
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(other) => Err(field_error(
            key,
            "list_type",
            "Input should be a list of tickers",
            other,
        )),
    }
}

/// `PUT /api/research/universe/{ticker}`: pack override, business note, peers.
async fn update_ticker(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/universe/{}", raw.trim().to_ascii_uppercase());
    idempotent(&st, &headers, "PUT", &path, &body, |_| {
        let v = json_body(&body)?;
        let patch = universe::Patch {
            pack_override: opt_string(&v, "pack_override")?,
            business_note: opt_string(&v, "business_note")?,
            peers: string_list(&v, "peers")?,
        };
        let engine = st.engine()?;
        let packs = st.pack_set();
        // New peer rows get the same exchange-list check and price backfill as POST.
        let mut new_peers: Vec<String> = Vec::new();
        for t in patch.peers.iter().flatten() {
            let Some(t) = universe::normalize_ticker(t) else {
                continue;
            };
            if new_peers.contains(&t) {
                continue;
            }
            if engine
                .store
                .universe_entry(&t)
                .map_err(store_error)?
                .is_none()
            {
                new_peers.push(t);
            }
        }
        if !new_peers.is_empty() {
            let (listed, list_available) = lookup::listed_tickers(&st)?;
            if list_available {
                if let Some(t) = new_peers.iter().find(|t| !listed.contains_key(*t)) {
                    return Err(ApiError::invalid(
                        &["body", "peers"],
                        "not_listed",
                        format!("{t} is not in the exchange list"),
                        Value::String(t.clone()),
                    ));
                }
            }
        }
        let entry = universe::update(
            &engine.store,
            &packs,
            &raw,
            &patch,
            &engine.now().to_rfc3339(),
        )
        .map_err(map_universe)?;
        write_watchlist(&st, &engine.store.universe().map_err(store_error)?);
        if let Some(tx) = &st.refresh_tx {
            for t in &new_peers {
                if engine
                    .store
                    .universe_entry(t)
                    .map_err(store_error)?
                    .is_some()
                {
                    let _ = tx.send(Some(t.clone()));
                }
            }
        }
        Ok((StatusCode::OK, universe_item(&st, &entry.ticker)?))
    })
}

fn write_watchlist(st: &ApiState, entries: &[universe::UniverseEntry]) {
    if let Some(path) = &st.watchlist {
        if let Err(e) = universe::write_watchlist(path, entries) {
            tracing::warn!("research watchlist: {e}");
        }
    }
}

async fn add_ticker(
    State(st): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    idempotent(
        &st,
        &headers,
        "POST",
        "/api/research/universe",
        &body,
        |_| {
            let v = json_body(&body)?;
            let role = match v.get("role") {
                None | Some(Value::Null) => None,
                Some(Value::String(r)) if r == "manual" => None,
                Some(Value::String(r)) if r == "peer" => Some(universe::Role::Peer),
                Some(other) => {
                    return Err(ApiError::invalid(
                        &["body", "role"],
                        "enum",
                        "Input should be 'manual' or 'peer'",
                        other.clone(),
                    ))
                }
            };
            let req = universe::AddRequest {
                ticker: v["ticker"].as_str().unwrap_or_default().to_string(),
                role,
                peer_of: opt_string(&v, "peer_of")?.flatten(),
                pack_override: opt_string(&v, "pack_override")?.flatten(),
                business_note: opt_string(&v, "business_note")?.flatten(),
            };
            let engine = st.engine()?;
            let packs = st.pack_set();
            // A ticker the exchange list does not know is refused when that list can be
            // read (A6.2); a row already stored (held included) is never re-checked.
            let was_known = match universe::normalize_ticker(&req.ticker) {
                Some(t) => engine
                    .store
                    .universe_entry(&t)
                    .map_err(store_error)?
                    .is_some(),
                None => false,
            };
            if !was_known {
                if let Some(t) = universe::normalize_ticker(&req.ticker) {
                    let (listed, list_available) = lookup::listed_tickers(&st)?;
                    if list_available && !listed.contains_key(&t) {
                        return Err(ApiError::invalid(
                            &["body", "ticker"],
                            "not_listed",
                            format!("{t} is not in the exchange list"),
                            Value::String(t),
                        ));
                    }
                }
            }
            let entry = universe::add(&engine.store, &packs, &req, &engine.now().to_rfc3339())
                .map_err(map_universe)?;
            write_watchlist(&st, &engine.store.universe().map_err(store_error)?);
            // A new row gets its price history now, not at the next 16:30 slot (A6.2); the
            // worker is the same one `POST /refresh` feeds. No worker (tests, scheduler
            // disabled) means the next scheduled pass fetches it.
            if !was_known {
                if let Some(tx) = &st.refresh_tx {
                    let _ = tx.send(Some(entry.ticker.clone()));
                }
            }
            Ok((StatusCode::CREATED, universe_item(&st, &entry.ticker)?))
        },
    )
}

async fn remove_ticker(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/universe/{}", raw.to_ascii_uppercase());
    idempotent(&st, &headers, "DELETE", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        universe::remove(&engine.store, &raw, &engine.now().to_rfc3339()).map_err(map_universe)?;
        write_watchlist(&st, &engine.store.universe().map_err(store_error)?);
        Ok((StatusCode::NO_CONTENT, Value::Null))
    })
}

#[derive(serde::Deserialize)]
struct RefreshQuery {
    ticker: Option<String>,
}

async fn refresh(
    State(st): State<ApiState>,
    Query(q): Query<RefreshQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = match q.ticker.as_deref() {
        Some(raw) => Some(canon_ticker(&st, raw)?),
        None => None,
    };
    let path = format!(
        "/api/research/refresh?ticker={}",
        ticker.as_deref().unwrap_or("")
    );
    let throttled = std::cell::Cell::new(false);
    let res = idempotent(&st, &headers, "POST", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        let now = engine.now();
        let slot = ticker.clone().unwrap_or_else(|| "*".into());
        {
            let mut seen = st.refreshed.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(last) = seen.get(&slot) {
                if (now - *last).num_seconds() < REFRESH_EVERY_SECS {
                    throttled.set(true);
                    return Err(ApiError::new(
                        StatusCode::TOO_MANY_REQUESTS,
                        "Refresh is limited to once a minute",
                    ));
                }
            }
            let tx = st.refresh_tx.as_ref().ok_or_else(|| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Refresh unavailable")
            })?;
            tx.send(ticker.clone()).map_err(|_| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Refresh unavailable")
            })?;
            seen.insert(slot, now);
        }
        Ok((StatusCode::ACCEPTED, json!({"status": "accepted"})))
    });
    match res {
        Err(e) if throttled.get() => {
            let mut r = e.into_response();
            r.headers_mut().insert(
                RETRY_AFTER,
                HeaderValue::from_str(&REFRESH_EVERY_SECS.to_string()).expect("digits"),
            );
            Ok(r)
        }
        other => other,
    }
}

// ---- canon v1 reads over facts (research plan §5, PRD A1.1, A1.4 to A1.7) ----
//
// These payloads carry values exactly as reported, so they bypass `out()`'s rounding.

use super::derive::{FactRef, Flag, Input};
use super::store::{DerivedRow, FilingRow};
use super::xbrl::labels::Labels;
use super::xbrl::{Cite, CtxKind};

/// Core rows every ticker shows (design.md §3.1), with element alternates in priority
/// order so taxonomy variants (bank income, capex naming) still populate the row.
/// Free cash flow is a pack formula and arrives with R2.1.
const CORE_ROWS: &[(&str, &str, &str, &[&str])] = &[
    (
        "revenue",
        "Pendapatan",
        "laba_rugi",
        &[
            "idx-cor:SalesAndRevenue",
            "idx-cor:IncomeFromFundManagementAsMudharib",
            "idx-cor:InterestIncome",
        ],
    ),
    (
        "gross_profit",
        "Laba kotor",
        "laba_rugi",
        &["idx-cor:GrossProfit"],
    ),
    (
        "operating_profit",
        "Laba usaha",
        "laba_rugi",
        &[
            "idx-cor:ProfitLossFromOperatingActivities",
            "idx-cor:ProfitLossBeforeIncomeTax",
        ],
    ),
    (
        "net_income",
        "Laba bersih",
        "laba_rugi",
        &["idx-cor:ProfitLoss"],
    ),
    (
        "net_income_parent",
        "Laba bersih entitas induk",
        "laba_rugi",
        &["idx-cor:ProfitLossAttributableToParentEntity"],
    ),
    (
        "eps_basic",
        "Laba per saham dasar",
        "laba_rugi",
        &["idx-cor:BasicEarningsLossPerShareFromContinuingOperations"],
    ),
    (
        "operating_cash_flow",
        "Arus kas operasi",
        "arus_kas",
        &["idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities"],
    ),
    (
        "capex",
        "Belanja modal",
        "arus_kas",
        &[
            "idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment",
            "idx-cor:AcquisitionOfPropertyPlantAndEquipment",
        ],
    ),
    (
        "depreciation",
        "Penyusutan",
        "arus_kas",
        &[
            "idx-cor:Depreciation",
            "idx-cor:DepreciationOfPropertyPlantAndEquipment",
        ],
    ),
    (
        "dividends_paid",
        "Dividen dibayar",
        "arus_kas",
        &[
            "idx-cor:DividendsPaidFromFinancingActivities",
            "idx-cor:DividendsPaidFromOperatingActivities",
        ],
    ),
    (
        "cash",
        "Kas dan setara kas",
        "neraca",
        &["idx-cor:CashAndCashEquivalents", "idx-cor:Cash"],
    ),
    ("assets", "Total aset", "neraca", &["idx-cor:Assets"]),
    (
        "liabilities",
        "Total liabilitas",
        "neraca",
        &["idx-cor:Liabilities"],
    ),
    ("equity", "Total ekuitas", "neraca", &["idx-cor:Equity"]),
    (
        "equity_parent",
        "Ekuitas entitas induk",
        "neraca",
        &["idx-cor:EquityAttributableToEquityOwnersOfParentEntity"],
    ),
    (
        "debt_short",
        "Pinjaman bank jangka pendek",
        "neraca",
        &["idx-cor:ShortTermBankLoans"],
    ),
    (
        "debt_long",
        "Pinjaman bank jangka panjang",
        "neraca",
        &["idx-cor:LongTermBankLoans"],
    ),
    ("bonds", "Obligasi", "neraca", &["idx-cor:BondsPayable"]),
    (
        "borrowings",
        "Pinjaman",
        "neraca",
        &["idx-cor:Borrowings", "idx-cor:BankLoans"],
    ),
    (
        "deposits",
        "Dana pihak ketiga",
        "neraca",
        &[
            "idx-cor:DepositsFromCustomers",
            "idx-cor:TotalThirdPartyFunds",
        ],
    ),
];

const GROUP_LABELS: &[(&str, &str)] = &[
    ("laba_rugi", "Laba rugi"),
    ("arus_kas", "Arus kas"),
    ("neraca", "Neraca"),
];

/// Group of an element not in the core list: instants are balance sheet, cash-flow-named
/// durations are cash flow, the rest profit and loss.
fn group_of(element: &str, instant: bool) -> &'static str {
    if instant {
        "neraca"
    } else if [
        "CashFlow",
        "Payments",
        "Proceeds",
        "Acquisition",
        "Dividends",
        "Repayment",
    ]
    .iter()
    .any(|w| element.contains(w))
    {
        "arus_kas"
    } else {
        "laba_rugi"
    }
}

fn store_of(st: &ApiState) -> Result<&Arc<super::store::Store>, ApiError> {
    Ok(&st.engine()?.store)
}

fn filing_json(r: &FilingRow) -> Value {
    let f = &r.filing;
    json!({
        "id": f.id, "ticker": f.ticker, "fy": f.fy, "period": f.period.as_str(),
        "taxonomy": f.taxonomy, "industry_code": f.industry_code,
        "period_start": f.period_start.to_string(), "period_end": f.period_end.to_string(),
        "audited": f.audited, "rounding": f.rounding, "currency": f.currency,
        "filed_at": f.filed_at.map(|d| d.to_string()),
        "available_from": f.available_from.to_string(), "available_source": f.available_source.as_str(),
        "source_path": f.source_path, "file_hash": f.file_hash, "source_kind": f.source_kind.as_str(),
        "ingested_at": r.ingested_at, "fact_count": r.fact_count,
    })
}

fn flags_json(flags: &std::collections::BTreeSet<Flag>) -> Value {
    json!(flags.iter().map(|f| f.as_str()).collect::<Vec<_>>())
}

fn fact_ref_json(f: &FactRef) -> Value {
    json!({
        "filing_id": f.filing_id, "element": f.element, "ctx_kind": f.ctx_kind,
        "start": f.start, "end": f.end, "dims": f.dims,
        "ctx": match &f.start { Some(s) => format!("{s}_{}", f.end), None => f.end.clone() },
    })
}

/// `{"kind": "pdf", "page", "label"}` of a fact read from a PDF filing, else `null`.
fn cite_json(cite: Option<&Cite>) -> Value {
    match cite {
        Some(c) => json!({"kind": "pdf", "page": c.page, "label": c.label}),
        None => Value::Null,
    }
}

/// The operands of a derived value; an operand read from a PDF filing carries its page.
fn inputs_json(inputs: &[Input], cites: &std::collections::HashMap<FactRef, Cite>) -> Value {
    json!(inputs
        .iter()
        .map(|i| json!({
            "role": i.role, "value": i.value, "fact": fact_ref_json(&i.fact),
            "cite": cite_json(cites.get(&i.fact)),
        }))
        .collect::<Vec<_>>())
}

/// Human formula from the roles the derivation recorded.
fn formula_of(view: &str, inputs: &[Input]) -> String {
    let roles: Vec<&str> = inputs.iter().map(|i| i.role.as_str()).collect();
    match (view, roles.as_slice()) {
        (_, ["instant"]) => "nilai pada tanggal neraca".into(),
        ("standalone", [one]) => (*one).to_string(),
        ("standalone", [a, b]) => format!("{a} − {b}"),
        ("annual", _) => "FY sebagaimana dilaporkan".into(),
        ("ttm", ["fy"]) => "FY sebagaimana dilaporkan".into(),
        ("ttm", [a, b, c]) if b.starts_with("prev_fy") => format!("{a} + {b} − {c}"),
        ("ttm", _) if roles.iter().any(|r| r.starts_with("standalone_")) => {
            "Σ empat kuartal mandiri terakhir".into()
        }
        _ => roles.join(", "),
    }
}

fn quarter_end_date(fy: i32, q: u32) -> Option<NaiveDate> {
    match q {
        1 => NaiveDate::from_ymd_opt(fy, 3, 31),
        2 => NaiveDate::from_ymd_opt(fy, 6, 30),
        3 => NaiveDate::from_ymd_opt(fy, 9, 30),
        4 => NaiveDate::from_ymd_opt(fy, 12, 31),
        _ => None,
    }
}

#[derive(serde::Deserialize)]
struct FilingsQuery {
    ticker: Option<String>,
}

/// `GET /api/research/filings?ticker=` (A1.1): every filing on disk with its header, plus
/// the files the parser could not read.
async fn filings_list(
    State(st): State<ApiState>,
    Query(q): Query<FilingsQuery>,
) -> Result<Json<Value>, ApiError> {
    let store = store_of(&st)?;
    let ticker = q
        .ticker
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .map(ticker_of)
        .transpose()?;
    let rows = store.filings(ticker.as_deref()).map_err(store_error)?;
    let unparsed = store.ingest_errors().map_err(store_error)?;
    Ok(Json(json!({
        "items": rows.iter().map(filing_json).collect::<Vec<_>>(),
        "unparsed": unparsed.iter().map(|e| json!({"source_path": e.source_path, "error": e.error, "at": e.at})).collect::<Vec<_>>(),
    })))
}

#[derive(serde::Deserialize)]
struct FactsFinancialsQuery {
    view: Option<String>,
    periods: Option<String>,
    /// `all`, or a comma list of elements; default: the core rows.
    elements: Option<String>,
}

fn cell_value<'a>(row: &'a DerivedRow, view: &str) -> (Option<f64>, &'a [Input]) {
    match view {
        // Instants (balance sheet) carry their value in `standalone`; they have no TTM.
        "ttm" if row.flags.contains(&Flag::Instant) => (row.standalone, &row.inputs.standalone),
        "ttm" => (row.ttm, &row.inputs.ttm),
        "annual" => (row.annual, &row.inputs.annual),
        _ => (row.standalone, &row.inputs.standalone),
    }
}

/// `GET /api/research/{ticker}/financials?view=standalone|ttm|annual&periods=12&elements=`
/// (A1.3 to A1.6): grouped `QuarterTable` payload with a `FactChip` target per cell.
async fn financials_facts(
    State(st): State<ApiState>,
    Path(ticker): Path<String>,
    Query(q): Query<FactsFinancialsQuery>,
) -> Result<Json<Value>, ApiError> {
    let ticker = ticker_of(&ticker)?;
    let store = store_of(&st)?;
    let view = match q.view.as_deref().unwrap_or("standalone") {
        v @ ("standalone" | "ttm" | "annual") => v.to_string(),
        other => {
            return Err(ApiError::invalid(
                &["query", "view"],
                "enum",
                "Input should be 'standalone', 'ttm' or 'annual'",
                json!(other),
            ))
        }
    };
    let periods_n = match q.periods.as_deref() {
        None => 12usize,
        Some(p) => p
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=40).contains(n))
            .ok_or_else(|| {
                ApiError::invalid(
                    &["query", "periods"],
                    "int_parsing",
                    "Input should be an integer between 1 and 40",
                    json!(p),
                )
            })?,
    };
    let filings = store.filings(Some(&ticker)).map_err(store_error)?;
    if filings.is_empty() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("{ticker} has no filings"),
        ));
    }
    let rows = store.derived(&ticker, None).map_err(store_error)?;
    let units = store.element_units(&ticker).map_err(store_error)?;
    let labels = Labels::default();
    // One index for every cell lookup instead of a scan per cell.
    let by_key: std::collections::HashMap<(&str, i32, u32), &DerivedRow> = rows
        .iter()
        .map(|r| ((r.element.as_str(), r.fy, r.q), r))
        .collect();
    let mut elements_present: Vec<&DerivedRow> = Vec::new();
    {
        let mut seen = std::collections::HashSet::new();
        for r in &rows {
            if seen.insert(r.element.as_str()) {
                elements_present.push(r);
            }
        }
    }

    // Periods: newest `periods_n` of the periods any filing covers (annual: FY only).
    let mut keys: Vec<(i32, u32)> = filings
        .iter()
        .map(|f| (f.filing.fy, f.filing.period.quarter()))
        .filter(|(_, q)| view != "annual" || *q == 4)
        .collect();
    if view == "annual" {
        // Comparative FY figures may exist one year before the first FY filing.
        if let Some(min) = keys.iter().map(|k| k.0).min() {
            if rows
                .iter()
                .any(|r| r.fy == min - 1 && r.q == 4 && r.annual.is_some())
            {
                keys.push((min - 1, 4));
            }
        }
    }
    keys.sort_unstable();
    keys.dedup();
    let keys: Vec<(i32, u32)> = keys.into_iter().rev().take(periods_n).rev().collect();
    let filing_of = |fy: i32, q: u32| {
        filings
            .iter()
            .find(|f| f.filing.fy == fy && f.filing.period.quarter() == q)
    };
    let periods: Vec<Value> = keys
        .iter()
        .map(|&(fy, q)| {
            let f = filing_of(fy, q);
            json!({
                "fy": fy, "q": q,
                "label": if view == "annual" { fy.to_string() } else { format!("{fy}Q{q}") },
                "end": quarter_end_date(fy, q).map(|d| d.to_string()),
                "filing_id": f.map(|f| f.filing.id.clone()),
                "source_kind": f.map(|f| f.filing.source_kind.as_str()),
                "audited": f.map(|f| f.filing.audited),
                "derived_fy": view == "standalone" && q == 4,
            })
        })
        .collect();

    // Row selection.
    let present = |element: &str| elements_present.iter().any(|r| r.element == element);
    let selection: Vec<(String, String, &'static str, String)> = match q.elements.as_deref() {
        Some("all") => {
            let mut elements: Vec<&DerivedRow> = Vec::new();
            for r in &rows {
                if !elements.iter().any(|e| e.element == r.element) {
                    elements.push(r);
                }
            }
            elements
                .iter()
                .map(|r| {
                    let instant = r.flags.contains(&Flag::Instant);
                    (
                        r.element.clone(),
                        labels.label(&r.element).id,
                        group_of(&r.element, instant),
                        r.element.clone(),
                    )
                })
                .collect()
        }
        Some(list) => list
            .split(',')
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(|e| {
                let instant = elements_present
                    .iter()
                    .find(|r| r.element == e)
                    .is_some_and(|r| r.flags.contains(&Flag::Instant));
                (
                    e.to_string(),
                    labels.label(e).id,
                    group_of(e, instant),
                    e.to_string(),
                )
            })
            .collect(),
        None => CORE_ROWS
            .iter()
            .filter_map(|(id, label, group, alternates)| {
                let element = alternates.iter().find(|e| present(e))?;
                Some((
                    id.to_string(),
                    label.to_string(),
                    *group,
                    element.to_string(),
                ))
            })
            .collect(),
    };

    // The unit as the facts carry it (`IDR`, `USD/shares`, `pure`), never assumed.
    let unit_of = |element: &str| -> Option<String> { units.get(element).cloned() };
    let mut groups: Vec<Value> = Vec::new();
    for (gid, glabel) in GROUP_LABELS {
        let group_rows: Vec<Value> = selection
            .iter()
            .filter(|(_, _, g, _)| g == gid)
            .map(|(id, label, _, element)| {
                let cells: Vec<Value> = keys
                    .iter()
                    .map(|&(fy, q)| {
                        let row = by_key.get(&(element.as_str(), fy, q)).copied();
                        match row {
                            Some(r) => {
                                let (value, inputs) = cell_value(r, &view);
                                json!({
                                    "value": value,
                                    "flags": flags_json(&r.flags),
                                    "fact": inputs.first().map(|i| fact_ref_json(&i.fact)),
                                    "inputs": inputs.len(),
                                })
                            }
                            None => {
                                json!({"value": null, "flags": ["gap"], "fact": null, "inputs": 0})
                            }
                        }
                    })
                    .collect();
                let unit = if element.contains("PerShare") {
                    unit_of(element).map(|u| format!("{u}/shares"))
                } else {
                    unit_of(element)
                };
                json!({"id": id, "label": label, "element": element, "unit": unit, "cells": cells})
            })
            .collect();
        if !group_rows.is_empty() {
            groups.push(json!({"id": gid, "label": glabel, "rows": group_rows}));
        }
    }

    // Shares outstanding per period, point-in-time with the source (D13, design §4.2).
    let series = store.share_series(&ticker).map_err(store_error)?;
    let shares: Vec<Value> = keys
        .iter()
        .map(|&(fy, q)| {
            quarter_end_date(fy, q)
                .and_then(|d| series.at(d))
                .map(|r| json!({"shares": r.shares, "source_label": r.source.label(), "source": r.source}))
                .unwrap_or(Value::Null)
        })
        .collect();

    let shown: Vec<Value> = filings
        .iter()
        .filter(|f| keys.contains(&(f.filing.fy, f.filing.period.quarter())))
        .map(filing_json)
        .collect();
    let newest = filings.first().map(|f| &f.filing);
    Ok(Json(json!({
        "ticker": ticker,
        "view": view,
        "rules_version": super::derive::RULES_VERSION,
        "currency": newest.map(|f| f.currency.clone()),
        "rounding": newest.map(|f| f.rounding.clone()),
        "taxonomy": newest.map(|f| f.taxonomy.clone()),
        "periods": periods,
        "groups": groups,
        "shares": shares,
        "filings": shown,
    })))
}

#[derive(serde::Deserialize)]
struct FactQuery {
    /// `YYYY-MM-DD` (instant) or `YYYY-MM-DD_YYYY-MM-DD` (duration).
    ctx: Option<String>,
    dims: Option<String>,
}

fn parse_ctx(ctx: &str) -> Result<(CtxKind, Option<String>, String), ApiError> {
    let bad = || {
        ApiError::invalid(
            &["query", "ctx"],
            "string_pattern_mismatch",
            "Input should be 'YYYY-MM-DD' for an instant or 'YYYY-MM-DD_YYYY-MM-DD' for a duration",
            json!(ctx),
        )
    };
    let is_date = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok();
    match ctx.split_once('_') {
        Some((s, e)) if is_date(s) && is_date(e) => {
            Ok((CtxKind::Duration, Some(s.to_string()), e.to_string()))
        }
        None if is_date(ctx) => Ok((CtxKind::Instant, None, ctx.to_string())),
        _ => Err(bad()),
    }
}

/// `GET /api/research/facts/{filing_id}/{element}?ctx=&dims=` (A1.5, A1.7): the
/// `FactDrawer` payload: the fact, its filing, the same period in every filing, the
/// restatements recorded against it, and every derived cell it feeds with the formula.
async fn fact_detail(
    State(st): State<ApiState>,
    Path((filing_id, element)): Path<(String, String)>,
    Query(q): Query<FactQuery>,
) -> Result<Json<Value>, ApiError> {
    let store = store_of(&st)?;
    let ctx = q
        .ctx
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| ApiError::missing_query(&["ctx"]))?;
    let (kind, start, end) = parse_ctx(ctx.trim())?;
    let dims = q.dims.unwrap_or_default();
    let filing = store
        .filing(&filing_id)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown filing"))?;
    let fact = store
        .fact(&filing_id, &element, kind, start.as_deref(), &end, &dims)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown fact"))?;
    let ticker = &filing.filing.ticker;
    let cites = store.pdf_cites(ticker).map_err(store_error)?;
    let history = store
        .fact_history(ticker, &element, kind, start.as_deref(), &end, &dims)
        .map_err(store_error)?;
    let restatements = store
        .restatements(ticker, &element, start.as_deref(), &end, &dims)
        .map_err(store_error)?;
    let this_ref = FactRef {
        filing_id: filing_id.clone(),
        element: element.clone(),
        ctx_kind: kind.as_str().into(),
        start: start.clone(),
        end: end.clone(),
        dims: dims.clone(),
    };
    let mut derived = Vec::new();
    for row in store.derived(ticker, Some(&element)).map_err(store_error)? {
        for (view, value, inputs) in [
            ("standalone", row.standalone, &row.inputs.standalone),
            ("ttm", row.ttm, &row.inputs.ttm),
            ("annual", row.annual, &row.inputs.annual),
        ] {
            if inputs.iter().any(|i| i.fact == this_ref) {
                derived.push(json!({
                    "fy": row.fy, "q": row.q, "view": view, "value": value,
                    "flags": flags_json(&row.flags),
                    "formula": formula_of(view, inputs),
                    "inputs": inputs_json(inputs, &cites),
                    "rules_version": row.rules_version,
                }));
            }
        }
    }
    let label = Labels::default().label(&element);
    let filings_by_id: std::collections::HashMap<String, FilingRow> = store
        .filings(Some(ticker))
        .map_err(store_error)?
        .into_iter()
        .map(|f| (f.filing.id.clone(), f))
        .collect();
    Ok(Json(json!({
        "fact": {
            "filing_id": fact.filing_id, "element": fact.element,
            "label": {"id": label.id, "en": label.en},
            "ctx_kind": fact.ctx_kind.as_str(), "start": fact.start, "end": fact.end, "dims": fact.dims,
            "value": fact.value, "unit": fact.unit, "decimals": fact.decimals,
            "cite": cite_json(fact.cite.as_ref()),
        },
        "filing": filing_json(&filing),
        "history": history.iter().map(|h| json!({
            "filing_id": h.filing_id, "value": h.value, "decimals": h.decimals,
            "source_kind": filings_by_id.get(&h.filing_id).map(|f| f.filing.source_kind.as_str()),
            "cite": cite_json(h.cite.as_ref()),
            "available_from": filings_by_id.get(&h.filing_id).map(|f| f.filing.available_from.to_string()),
            "original": h.filing_id == filing_id,
        })).collect::<Vec<_>>(),
        "restatements": restatements.iter().map(|r| json!({
            "original": r.original, "original_filing": r.original_filing,
            "restated": r.restated, "restated_filing": r.restated_filing, "detected_at": r.detected_at,
        })).collect::<Vec<_>>(),
        "derived": derived,
    })))
}

pub fn router(st: ApiState) -> Router {
    Router::new()
        .route(
            "/api/research/universe",
            get(universe_canon).post(add_ticker),
        )
        .route(
            "/api/research/universe/{ticker}",
            put(update_ticker).delete(remove_ticker),
        )
        .route("/api/research/lookup", get(lookup::lookup))
        .route("/api/research/rules", get(rules_canon))
        .route("/api/research/refresh", post(refresh))
        .route("/api/research/desk", get(r4::desk))
        .route("/api/research/compare", get(r5::compare))
        .route("/api/research/calibration", get(r4::calibration_get))
        .route("/api/research/events/{id}/dismiss", post(r4::event_dismiss))
        .route("/api/research/filings", get(filings_list))
        .route("/api/research/packs", get(packs_list))
        .route("/api/research/packs/reload", post(packs_reload))
        .route(
            "/api/research/facts/{filing_id}/{element}",
            get(fact_detail),
        )
        .route("/api/research/{ticker}", get(r5::detail))
        .route("/api/research/{ticker}/financials", get(financials_facts))
        .route("/api/research/{ticker}/pack", get(pack_section))
        .route("/api/research/{ticker}/valuation", get(valuation_get))
        .route("/api/research/{ticker}/scenarios", put(scenarios_put))
        .route("/api/research/{ticker}/prices", get(prices_canon))
        .route("/api/research/{ticker}/seen", post(seen))
        .route(
            "/api/research/{ticker}/shares",
            put(put_shares).delete(delete_shares),
        )
        .route(
            "/api/research/{ticker}/kpi/{metric}/{fy}/{q}",
            put(put_kpi).delete(delete_kpi),
        )
        .route(
            "/api/research/{ticker}/claims",
            get(r4::claims_list).post(r4::claims_create),
        )
        .route(
            "/api/research/{ticker}/calendar",
            get(r4::calendar_list).post(r4::calendar_add),
        )
        .route(
            "/api/research/{ticker}/calendar/{date}/{kind}",
            delete(r4::calendar_delete),
        )
        .route(
            "/api/research/{ticker}/documents",
            get(r4::documents_list).post(r4::documents_add),
        )
        .route(
            "/api/research/{ticker}/documents/upload",
            post(r8::upload).layer(DefaultBodyLimit::max(r8::UPLOAD_BODY_LIMIT)),
        )
        .route("/api/research/{ticker}/structure", get(r9::structure_get))
        .route(
            "/api/research/{ticker}/structure/nodes",
            post(r9::node_post),
        )
        .route(
            "/api/research/{ticker}/structure/nodes/{id}",
            put(r9::node_put).delete(r9::node_delete),
        )
        .route(
            "/api/research/{ticker}/structure/draft",
            post(r9::draft_import),
        )
        .route("/api/research/{ticker}/proposals", get(r8::proposals_list))
        .route(
            "/api/research/proposals/{id}/accept",
            post(r8::proposal_accept),
        )
        .route(
            "/api/research/proposals/{id}/reject",
            post(r8::proposal_reject),
        )
        .route("/api/research/evidence", post(r4::evidence_add))
        .route(
            "/api/research/claims/{id}/evidence",
            get(r4::claim_evidence),
        )
        .route("/api/research/claims/{id}", patch(r4::claims_patch))
        .route("/api/research/claims/{id}/evals", get(r4::claims_evals))
        .route(
            "/api/research/{ticker}/journal",
            get(r4::journal_list).post(r4::journal_add),
        )
        // GET only: an entry has no PUT and no DELETE (D9).
        .route("/api/research/journal/{id}", get(r4::journal_get))
        .route(
            "/api/research/journal/{id}/annotations",
            post(r4::journal_annotate),
        )
        .with_state(st)
}

#[cfg(test)]
mod cell_value_tests {
    use super::*;

    fn input(role: &str, value: f64) -> Input {
        Input {
            role: role.into(),
            value,
            fact: FactRef {
                filing_id: "f".into(),
                element: "e".into(),
                ctx_kind: "instant".into(),
                start: None,
                end: "2026-06-30".into(),
                dims: String::new(),
            },
        }
    }

    fn row(flags: &[Flag]) -> DerivedRow {
        let inputs = super::super::derive::CellInputs {
            standalone: vec![input("instant", 7.0)],
            ttm: vec![input("fy", 99.0)],
            ..Default::default()
        };
        DerivedRow {
            ticker: "T".into(),
            element: "e".into(),
            fy: 2026,
            q: 2,
            standalone: Some(7.0),
            ttm: Some(99.0),
            annual: Some(5.0),
            flags: flags.iter().copied().collect(),
            inputs,
            rules_version: "v".into(),
            computed_at: "t".into(),
        }
    }

    #[test]
    fn instant_row_uses_standalone_under_ttm() {
        let r = row(&[Flag::Instant]);
        let (v, inputs) = cell_value(&r, "ttm");
        assert_eq!(v, Some(7.0));
        assert_eq!(inputs[0].role, "instant");
    }

    #[test]
    fn instant_row_with_no_ttm_still_shows_standalone() {
        let mut r = row(&[Flag::Instant]);
        r.ttm = None;
        r.inputs.ttm.clear();
        assert_eq!(cell_value(&r, "ttm").0, Some(7.0));
    }

    #[test]
    fn duration_row_keeps_ttm() {
        let r = row(&[]);
        let (v, inputs) = cell_value(&r, "ttm");
        assert_eq!(v, Some(99.0));
        assert_eq!(inputs[0].role, "fy");
    }

    #[test]
    fn annual_view_unchanged_for_instant() {
        let r = row(&[Flag::Instant]);
        assert_eq!(cell_value(&r, "annual").0, Some(5.0));
    }
}
