//! R4 handlers (research plan §5): claims, journal, documents and evidence, the Desk,
//! event dismissal and the calibration view. Same conventions as the rest of the API:
//! writes need an `Idempotency-Key`, ids are derived from it so a retry after a restart
//! cannot create a second row, and 422 bodies name the field.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use chrono::NaiveDate;
use serde_json::{json, Value};

use super::{
    body_hash, canon_ticker, field_error, idempotent, json_body, kpi_target, out, store_error,
    ApiError, ApiState,
};
use crate::research::calendar;
use crate::research::calibration;
use crate::research::claims::source::{self, TickerData};
use crate::research::claims::{self, Comparator, Spec, Status, Window};
use crate::research::documents;
use crate::research::events::react;
use crate::research::journal;
use crate::research::prices::payload::split_ratios;
use crate::research::store::{
    AnnotationRow, CalendarRow, ClaimEvalRow, ClaimRow, DocumentRow, EvidenceRow, JournalRow,
};

// ---- claims ----

fn parse_day(v: &Value) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(v.as_str()?, "%Y-%m-%d").ok()
}

fn status_json(s: &str) -> Value {
    json!({"status": s, "label": Status::parse(s).map(Status::label)})
}

/// One claim as the claim card renders it.
fn claim_json(
    c: &ClaimRow,
    metric: Option<&source::Metric>,
    last: Option<&ClaimEvalRow>,
    evidence: usize,
) -> Value {
    let spec = source::spec_of(c);
    let label = metric.map_or(c.metric.as_str(), |m| m.label.as_str());
    let unit = metric.map_or("", |m| m.unit.as_str());
    json!({
        "id": c.id, "ticker": c.ticker, "statement": c.statement,
        "metric": c.metric, "metric_label": label, "unit": unit,
        "comparator": c.comparator, "threshold": c.threshold, "threshold_hi": c.threshold_hi,
        "window": c.window, "due": c.due,
        "status": c.status, "status_label": Status::parse(&c.status).map(Status::label),
        "created_at": c.created_at, "closed_at": c.closed_at,
        "preview": spec.map(|s| claims::preview(label, unit, &s)),
        "last_eval": last.map(|e| json!({
            "evaluated_at": e.evaluated_at, "status": e.status,
            "status_label": Status::parse(&e.status).map(Status::label), "value": e.value,
        })),
        "evidence_count": evidence,
    })
}

/// `GET /api/research/{ticker}/claims`: the claim list (Patah, Belum teruji, Berlaku,
/// Kedaluwarsa; closed claims last) with the metrics the editor can bind to.
pub(super) async fn claims_list(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let engine = st.engine()?;
    let store = &engine.store;
    let entry = store
        .universe_entry(&ticker)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
    let packs = st.pack_set();
    let applied = source::applied_packs(store, &packs, &entry).map_err(store_error)?;
    let evidence = store.evidence_counts(&ticker).map_err(store_error)?;
    let mut rows = store.claims(Some(&ticker)).map_err(store_error)?;
    rows.sort_by(|a, b| {
        let rank = |c: &ClaimRow| {
            (
                c.closed_at.is_some(),
                Status::parse(&c.status).map_or(9, Status::rank),
            )
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut items = Vec::new();
    for c in &rows {
        let last = store.last_eval(&c.id).map_err(store_error)?;
        let metric = source::resolve(&applied, &c.metric).ok();
        items.push(claim_json(
            c,
            metric.as_ref(),
            last.as_ref(),
            evidence.get(&c.id).copied().unwrap_or(0),
        ));
    }
    let [holding, broken, not_testable, expired] = source::status_counts(&rows);
    Ok(out(json!({
        "ticker": ticker,
        "items": items,
        "counts": {"holding": holding, "broken": broken, "not_testable": not_testable, "expired": expired},
        "metrics": source::catalogue(&applied),
        "windows": ["latest", "at_due", "every_quarter", "quarter:<FY>Q<n>"],
        "comparators": Comparator::ALL.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
    })))
}

/// `POST /api/research/{ticker}/claims` with `{statement, metric, comparator, threshold,
/// threshold_hi?, window, due}`. The claim is evaluated once on save, so it appears with
/// its first status; later evaluations come from the re-evaluation hook.
pub(super) async fn claims_create(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/claims");
    idempotent(&st, &headers, "POST", &path, &body, |key| {
        let v = json_body(&body)?;
        let statement = v["statement"].as_str().map(str::trim).unwrap_or_default();
        let len = statement.chars().count();
        if len == 0 || len > claims::MAX_STATEMENT_CHARS {
            return Err(field_error(
                "statement",
                "string_length",
                "Statement must be 1 to 200 characters",
                &v["statement"],
            ));
        }
        let comparator = v["comparator"]
            .as_str()
            .and_then(Comparator::parse)
            .ok_or_else(|| {
                field_error(
                    "comparator",
                    "enum",
                    "Input should be '<', '<=', '>', '>=' or 'between'",
                    &v["comparator"],
                )
            })?;
        let threshold = v["threshold"].as_f64().ok_or_else(|| {
            field_error(
                "threshold",
                "float_parsing",
                "Input should be a number",
                &v["threshold"],
            )
        })?;
        let threshold_hi = match &v["threshold_hi"] {
            Value::Null => None,
            x => Some(x.as_f64().ok_or_else(|| {
                field_error(
                    "threshold_hi",
                    "float_parsing",
                    "Input should be a number",
                    x,
                )
            })?),
        };
        claims::check_thresholds(comparator, threshold, threshold_hi)
            .map_err(|e| field_error(e.field, "value_error", e.message, &v[e.field]))?;
        let window = v["window"]
            .as_str()
            .and_then(Window::parse)
            .ok_or_else(|| {
                field_error(
                    "window",
                    "enum",
                    "Input should be 'latest', 'at_due', 'every_quarter' or 'quarter:<FY>Q<n>'",
                    &v["window"],
                )
            })?;
        let engine = st.engine()?;
        let today = engine.today();
        let due = parse_day(&v["due"])
            .ok_or_else(|| {
                field_error(
                    "due",
                    "date_parsing",
                    "Input should be a date, YYYY-MM-DD",
                    &v["due"],
                )
            })
            .and_then(|d| {
                if d < today {
                    Err(field_error(
                        "due",
                        "value_error",
                        "Due date must not be in the past",
                        &v["due"],
                    ))
                } else {
                    Ok(d)
                }
            })?;
        let store = &engine.store;
        let entry = store
            .universe_entry(&ticker)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
        let packs = st.pack_set();
        let data = TickerData::load(store, &packs, &entry).map_err(store_error)?;
        let metric_id = v["metric"].as_str().unwrap_or_default();
        let metric = source::resolve(&data.applied, metric_id)
            .map_err(|e| field_error("metric", "value_error", &e.message(), &v["metric"]))?;
        if window.needs_quarters() && !metric.quarterly() {
            return Err(field_error(
                "window",
                "value_error",
                "A price metric has one value per day: use 'latest' or 'at_due'",
                &v["window"],
            ));
        }
        let spec = Spec {
            comparator,
            threshold,
            threshold_hi,
            window,
            since: today,
            due,
        };
        let obs = data.observations(&packs, &metric, today, due);
        let eval = claims::evaluate(&spec, &obs, today);
        let now = engine.now().to_rfc3339();
        // Derived from the key, so a retry after a restart cannot create a second claim.
        let id = format!("c-{}", &body_hash("claim", &ticker, key.as_bytes())[..16]);
        // A retry that got past the idempotency journal must not reset the claim or add a
        // second first evaluation.
        if let Some(existing) = store.claim(&id).map_err(store_error)? {
            let last = store.last_eval(&id).map_err(store_error)?;
            return Ok((
                StatusCode::CREATED,
                claim_json(&existing, Some(&metric), last.as_ref(), 0),
            ));
        }
        let row = ClaimRow {
            id: id.clone(),
            ticker: ticker.clone(),
            statement: statement.to_string(),
            metric: metric.id.clone(),
            comparator: comparator.as_str().into(),
            threshold,
            threshold_hi,
            window: window.to_string(),
            due: due.to_string(),
            status: eval.status.as_str().into(),
            created_at: now.clone(),
            closed_at: None,
        };
        let first = ClaimEvalRow {
            claim_id: id,
            evaluated_at: now,
            status: eval.status.as_str().into(),
            value: eval.value,
            inputs: eval.inputs_json(window, today).to_string(),
        };
        store.insert_claim(&row).map_err(store_error)?;
        store.record_eval(&first).map_err(store_error)?;
        Ok((
            StatusCode::CREATED,
            claim_json(&row, Some(&metric), Some(&first), 0),
        ))
    })
}

/// `PATCH /api/research/claims/{id}` with `{"closed": true}`: closes the claim ("Tutup
/// klaim"). The claim keeps its last status and every evaluation; it is not re-evaluated.
pub(super) async fn claims_patch(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/claims/{id}");
    idempotent(&st, &headers, "PATCH", &path, &body, |_| {
        let v = json_body(&body)?;
        if v["closed"] != json!(true) {
            return Err(field_error(
                "closed",
                "literal_error",
                "Input should be true; a claim can be closed, not reopened or edited",
                &v["closed"],
            ));
        }
        let engine = st.engine()?;
        let store = &engine.store;
        let claim = store
            .claim(&id)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Claim not found"))?;
        if claim.closed_at.is_some() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Claim is already closed",
            ));
        }
        store
            .close_claim(&id, &engine.now().to_rfc3339())
            .map_err(store_error)?;
        let closed = store
            .claim(&id)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Claim not found"))?;
        let last = store.last_eval(&id).map_err(store_error)?;
        let evidence = store.evidence_counts(&closed.ticker).map_err(store_error)?;
        Ok((
            StatusCode::OK,
            claim_json(
                &closed,
                None,
                last.as_ref(),
                evidence.get(&id).copied().unwrap_or(0),
            ),
        ))
    })
}

/// `GET /api/research/claims/{id}/evals`: every stored evaluation, oldest first, each with
/// the value and the observations it used.
pub(super) async fn claims_evals(
    State(st): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = &st.engine()?.store;
    let claim = store
        .claim(&id)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Claim not found"))?;
    let items: Vec<Value> = store
        .claim_evals(&id)
        .map_err(store_error)?
        .iter()
        .map(|e| {
            let mut v = status_json(&e.status);
            v["evaluated_at"] = json!(e.evaluated_at);
            v["value"] = json!(e.value);
            v["inputs"] = serde_json::from_str(&e.inputs).unwrap_or(Value::Null);
            v
        })
        .collect();
    Ok(out(json!({
        "claim_id": id, "ticker": claim.ticker, "statement": claim.statement, "items": items,
    })))
}

// ---- journal (append-only: no PUT, no DELETE on entries) ----

const JOURNAL_PAGE: usize = 20;
const JOURNAL_MAX_PAGE: usize = 100;

fn annotation_json(a: &AnnotationRow) -> Value {
    json!({"id": a.id, "at": a.at, "text": a.text})
}

fn journal_json(j: &JournalRow, annotations: &[&AnnotationRow]) -> Value {
    json!({
        "id": j.id, "ticker": j.ticker, "at": j.at, "kind": j.kind, "text": j.text,
        "price_at": j.price_at, "level": j.level,
        "annotations": annotations.iter().map(|a| annotation_json(a)).collect::<Vec<_>>(),
    })
}

fn field_err(e: &journal::FieldError, input: &Value) -> ApiError {
    field_error(e.field, e.kind, e.message, input)
}

#[derive(serde::Deserialize)]
pub(super) struct JournalQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

/// `GET /api/research/{ticker}/journal?limit=&offset=`: newest first, 20 a page, each entry
/// with its annotations.
pub(super) async fn journal_list(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<JournalQuery>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let store = &st.engine()?.store;
    let limit = q.limit.unwrap_or(JOURNAL_PAGE).clamp(1, JOURNAL_MAX_PAGE);
    let offset = q.offset.unwrap_or(0);
    let entries = store.journal(Some(&ticker)).map_err(store_error)?;
    let annotations = store.annotations(Some(&ticker)).map_err(store_error)?;
    let items: Vec<Value> = entries
        .iter()
        .skip(offset)
        .take(limit)
        .map(|j| {
            let own: Vec<&AnnotationRow> = annotations
                .iter()
                .filter(|a| a.journal_id == j.id)
                .collect();
            journal_json(j, &own)
        })
        .collect();
    Ok(out(json!({
        "ticker": ticker, "items": items, "total": entries.len(), "limit": limit, "offset": offset,
    })))
}

/// `GET /api/research/journal/{id}`: one entry with its annotations.
pub(super) async fn journal_get(
    State(st): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = &st.engine()?.store;
    let entry = store
        .journal_entry(&id)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Journal entry not found"))?;
    let annotations = store
        .annotations(Some(&entry.ticker))
        .map_err(store_error)?;
    let own: Vec<&AnnotationRow> = annotations.iter().filter(|a| a.journal_id == id).collect();
    Ok(out(journal_json(&entry, &own)))
}

/// `POST /api/research/{ticker}/journal` with `{kind: catatan|level|keputusan, text,
/// level?}`. The price is stamped from the latest stored close at save time and the entry
/// cannot be changed afterwards (A4.2).
pub(super) async fn journal_add(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/journal");
    idempotent(&st, &headers, "POST", &path, &body, |key| {
        let v = json_body(&body)?;
        let level = match &v["level"] {
            Value::Null => None,
            x => Some(x.as_f64().ok_or_else(|| {
                field_error("level", "float_parsing", "Input should be a number", x)
            })?),
        };
        let entry = journal::check_entry(
            v["kind"].as_str().unwrap_or_default(),
            v["text"].as_str().unwrap_or_default(),
            level,
        )
        .map_err(|e| field_err(&e, &v[e.field]))?;
        let engine = st.engine()?;
        let store = &engine.store;
        let price_at = store
            .last_closes(&ticker, 1)
            .map_err(store_error)?
            .last()
            .map(|(_, close)| *close);
        // Derived from the key, so a retry after a restart cannot create a second entry; a
        // retry that finds the entry answers with what was stored, not with the new text.
        let id = format!("j-{}", &body_hash("journal", &ticker, key.as_bytes())[..16]);
        if let Some(stored) = store.journal_entry(&id).map_err(store_error)? {
            return Ok((StatusCode::CREATED, journal_json(&stored, &[])));
        }
        let row = JournalRow {
            id,
            ticker: ticker.clone(),
            at: engine.now().to_rfc3339(),
            kind: entry.kind.as_str().into(),
            text: entry.text,
            price_at,
            level: entry.level,
        };
        store.add_journal(&row).map_err(store_error)?;
        Ok((StatusCode::CREATED, journal_json(&row, &[])))
    })
}

/// `POST /api/research/journal/{id}/annotations` with `{text}`: a note below an entry. The
/// entry itself stays as written.
pub(super) async fn journal_annotate(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/journal/{id}/annotations");
    idempotent(&st, &headers, "POST", &path, &body, |key| {
        let v = json_body(&body)?;
        let text = journal::check_text(v["text"].as_str().unwrap_or_default(), "text")
            .map_err(|e| field_err(&e, &v["text"]))?;
        let engine = st.engine()?;
        let store = &engine.store;
        store
            .journal_entry(&id)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Journal entry not found"))?;
        let row = AnnotationRow {
            id: format!("a-{}", &body_hash("annotation", &id, key.as_bytes())[..16]),
            journal_id: id.clone(),
            at: engine.now().to_rfc3339(),
            text,
        };
        store.add_annotation(&row).map_err(store_error)?;
        let mut out = annotation_json(&row);
        out["journal_id"] = json!(id);
        Ok((StatusCode::CREATED, out))
    })
}

// ---- calendar ----

const MAX_CALENDAR_NOTE: usize = 200;

/// Every calendar item of one ticker inside the horizon.
fn ticker_calendar(
    st: &ApiState,
    ticker: &str,
    horizon_days: i64,
) -> Result<Vec<calendar::Item>, ApiError> {
    let engine = st.engine()?;
    react::calendar_for(
        &engine.store,
        &st.rules().rules.pojk_offsets,
        ticker,
        engine.today(),
        horizon_days,
    )
    .map_err(store_error)
}

#[derive(serde::Deserialize)]
pub(super) struct CalendarQuery {
    days: Option<i64>,
}

/// `GET /api/research/{ticker}/calendar?days=90`.
pub(super) async fn calendar_list(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<CalendarQuery>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let days = q.days.unwrap_or(calendar::HORIZON_DAYS).clamp(1, 366);
    let items = ticker_calendar(&st, &ticker, days)?;
    Ok(out(json!({"ticker": ticker, "days": days, "items": items})))
}

/// `POST /api/research/{ticker}/calendar` with `{date, kind, note?}`: an owner-entered
/// date (RUPS, a presentation, a report date the filing calendar cannot know). The same
/// `(date, kind)` again replaces the note.
pub(super) async fn calendar_add(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/calendar");
    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let v = json_body(&body)?;
        let date = parse_day(&v["date"]).ok_or_else(|| {
            field_error(
                "date",
                "date_parsing",
                "Input should be a date, YYYY-MM-DD",
                &v["date"],
            )
        })?;
        let kind = v["kind"]
            .as_str()
            .filter(|k| calendar::manual_label(k).is_some())
            .ok_or_else(|| {
                field_error(
                    "kind",
                    "enum",
                    "Input should be 'rups', 'dividen', 'laporan', 'paparan' or 'lainnya'",
                    &v["kind"],
                )
            })?;
        let note = match &v["note"] {
            Value::Null => None,
            Value::String(s) if s.trim().chars().count() <= MAX_CALENDAR_NOTE => {
                Some(s.trim().to_string()).filter(|s| !s.is_empty())
            }
            x => {
                return Err(field_error(
                    "note",
                    "string_length",
                    "Note must be at most 200 characters",
                    x,
                ))
            }
        };
        let row = CalendarRow {
            ticker: ticker.clone(),
            date: date.to_string(),
            kind: kind.into(),
            note,
        };
        st.engine()?.store.set_calendar(&row).map_err(store_error)?;
        Ok((StatusCode::CREATED, json!(row)))
    })
}

/// `DELETE /api/research/{ticker}/calendar/{date}/{kind}`: removes an owner-entered date.
pub(super) async fn calendar_delete(
    State(st): State<ApiState>,
    Path((raw, date, kind)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/calendar/{date}/{kind}");
    idempotent(&st, &headers, "DELETE", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        let exists = engine
            .store
            .calendar_manual(Some(&ticker))
            .map_err(store_error)?
            .iter()
            .any(|c| c.date == date && c.kind == kind);
        if !exists {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "Calendar entry not found",
            ));
        }
        engine
            .store
            .delete_calendar(&ticker, &date, &kind, &engine.now().to_rfc3339())
            .map_err(store_error)?;
        Ok((StatusCode::NO_CONTENT, Value::Null))
    })
}

// ---- desk and inbox ----

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeskQuery {
    ticker: Option<String>,
}

/// `GET /api/research/desk[?ticker=T]`: `{inbox, coverage, calendar, ages}` (design.md §4.1);
/// with `ticker` every list is that ticker's (R5 right pane).
pub(super) async fn desk(
    State(st): State<ApiState>,
    Query(q): Query<DeskQuery>,
) -> Result<Json<Value>, ApiError> {
    // An empty value is "no ticker", as for `/filings?ticker=`.
    let only = q
        .ticker
        .filter(|t| !t.trim().is_empty())
        .map(|t| canon_ticker(&st, &t))
        .transpose()?;
    let engine = st.engine()?;
    react::desk(
        &engine.store,
        &st.pack_set(),
        &st.rules().rules.pojk_offsets,
        engine.now(),
        engine.today(),
        only.as_deref(),
    )
    .map(out)
    .map_err(store_error)
}

/// `POST /api/research/events/{id}/dismiss`: **Abaikan**. The dismissal is logged with its
/// time; a dismissed event is gone from the inbox and is never produced again.
pub(super) async fn event_dismiss(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/events/{id}/dismiss");
    idempotent(&st, &headers, "POST", &path, &Bytes::new(), |_| {
        let engine = st.engine()?;
        let store = &engine.store;
        let event = store
            .event(&id)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Event not found"))?;
        if event.dismissed_at.is_some() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Event is already dismissed",
            ));
        }
        store
            .dismiss_event(&id, &engine.now().to_rfc3339())
            .map_err(store_error)?;
        let done = store
            .event(&id)
            .map_err(store_error)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Event not found"))?;
        Ok((
            StatusCode::OK,
            crate::research::events::item_json(&done, engine.today()),
        ))
    })
}

// ---- documents and evidence ----

pub(super) fn document_json(d: &DocumentRow, evidence: &[&EvidenceRow]) -> Value {
    json!({
        "id": d.id, "ticker": d.ticker, "kind": d.kind, "kind_label": documents::kind_label(&d.kind),
        "url": d.url, "text": d.text, "doc_date": d.doc_date, "added_at": d.added_at,
        "evidence": evidence
            .iter()
            .map(|e| json!({"claim_id": e.claim_id, "kpi_key": e.kpi_key}))
            .collect::<Vec<_>>(),
    })
}

fn document_error(e: &documents::FieldError, input: &Value) -> ApiError {
    field_error(e.field, e.kind, e.message, input)
}

/// Checks that `claim_id` (a claim of the document's ticker) or `kpi_key` (a KPI of the
/// ticker's pack in effect, `<metric>:<FY>Q<q>`) names something real, and returns the
/// link in its normal form. Exactly one target is required.
fn evidence_target(
    st: &ApiState,
    doc: &DocumentRow,
    claim_id: &Value,
    kpi_key: &Value,
) -> Result<EvidenceRow, ApiError> {
    let store = &st.engine()?.store;
    match (claim_id.as_str(), kpi_key.as_str()) {
        (Some(id), None) => {
            let claim = store
                .claim(id)
                .map_err(store_error)?
                .filter(|c| c.ticker == doc.ticker)
                .ok_or_else(|| {
                    field_error(
                        "claim_id",
                        "value_error",
                        "No such claim for this ticker",
                        claim_id,
                    )
                })?;
            Ok(EvidenceRow {
                claim_id: Some(claim.id),
                kpi_key: None,
                document_id: doc.id.clone(),
            })
        }
        (None, Some(key)) => {
            let bad = |msg: &str| field_error("kpi_key", "value_error", msg, kpi_key);
            let (metric, fy, q) = documents::parse_kpi_key(key)
                .ok_or_else(|| bad("Input should look like production_mt:2026Q2"))?;
            let (_, _, metric_id) = kpi_target(st, &doc.ticker, &metric)
                .map_err(|_| bad("Not a KPI of this ticker's pack"))?;
            Ok(EvidenceRow {
                claim_id: None,
                kpi_key: Some(documents::kpi_key(&metric_id, fy, q)),
                document_id: doc.id.clone(),
            })
        }
        _ => Err(field_error(
            "claim_id",
            "value_error",
            "Give exactly one of claim_id and kpi_key",
            &json!({"claim_id": claim_id, "kpi_key": kpi_key}),
        )),
    }
}

fn link_json(link: &EvidenceRow, created: bool) -> Value {
    json!({
        "document_id": link.document_id, "claim_id": link.claim_id, "kpi_key": link.kpi_key,
        "created": created,
    })
}

fn store_link(st: &ApiState, ticker: &str, link: &EvidenceRow) -> Result<bool, ApiError> {
    let store = &st.engine()?.store;
    let existed = store
        .evidence(ticker)
        .map_err(store_error)?
        .iter()
        .any(|e| e == link);
    if !existed {
        store.link_evidence(link).map_err(store_error)?;
    }
    Ok(!existed)
}

/// `GET /api/research/{ticker}/documents`: the owner's documents, newest first, each with
/// the claims and KPI cells it is evidence for.
pub(super) async fn documents_list(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let store = &st.engine()?.store;
    let docs = store.documents(&ticker).map_err(store_error)?;
    let links = store.evidence(&ticker).map_err(store_error)?;
    let items: Vec<Value> = docs
        .iter()
        .map(|d| {
            let own: Vec<&EvidenceRow> = links.iter().filter(|e| e.document_id == d.id).collect();
            document_json(d, &own)
        })
        .collect();
    Ok(out(json!({
        "ticker": ticker, "items": items,
        "kinds": documents::KINDS.iter().map(|(k, l)| json!({"id": k, "label": l})).collect::<Vec<_>>(),
    })))
}

/// `POST /api/research/{ticker}/documents` with `{kind, url?, text?, doc_date?}` and,
/// optionally, one target (`claim_id` or `kpi_key`) to attach it to in the same save
/// ("paste URL or text, pick target, save").
pub(super) async fn documents_add(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/documents");
    idempotent(&st, &headers, "POST", &path, &body, |key| {
        let v = json_body(&body)?;
        let engine = st.engine()?;
        let doc_date = match &v["doc_date"] {
            Value::Null => None,
            x => Some(parse_day(x).ok_or_else(|| {
                field_error(
                    "doc_date",
                    "date_parsing",
                    "Input should be a date, YYYY-MM-DD",
                    x,
                )
            })?),
        };
        let new = documents::check_document(
            v["kind"].as_str().unwrap_or_default(),
            v["url"].as_str(),
            v["text"].as_str(),
            doc_date,
            engine.today(),
        )
        .map_err(|e| document_error(&e, &v[e.field]))?;
        let id = format!(
            "d-{}",
            &body_hash("document", &ticker, key.as_bytes())[..16]
        );
        if let Some(stored) = engine.store.document(&id).map_err(store_error)? {
            let links = engine.store.evidence(&ticker).map_err(store_error)?;
            let own: Vec<&EvidenceRow> = links.iter().filter(|e| e.document_id == id).collect();
            return Ok((StatusCode::CREATED, document_json(&stored, &own)));
        }
        let row = DocumentRow {
            id,
            ticker: ticker.clone(),
            kind: new.kind,
            url: new.url,
            text: new.text,
            doc_date: new.doc_date.map(|d| d.to_string()),
            added_at: engine.now().to_rfc3339(),
        };
        // Validate the optional target before anything is written.
        let wants_target = !v["claim_id"].is_null() || !v["kpi_key"].is_null();
        let link = wants_target
            .then(|| evidence_target(&st, &row, &v["claim_id"], &v["kpi_key"]))
            .transpose()?;
        engine.store.add_document(&row).map_err(store_error)?;
        let links: Vec<EvidenceRow> = match link {
            Some(l) => {
                store_link(&st, &ticker, &l)?;
                vec![l]
            }
            None => Vec::new(),
        };
        let refs: Vec<&EvidenceRow> = links.iter().collect();
        Ok((StatusCode::CREATED, document_json(&row, &refs)))
    })
}

/// `POST /api/research/evidence` with `{document_id, claim_id | kpi_key}`: attaches a filed
/// document to a claim or a KPI cell. Linking twice is a no-op (`created: false`).
pub(super) async fn evidence_add(
    State(st): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    idempotent(
        &st,
        &headers,
        "POST",
        "/api/research/evidence",
        &body,
        |_| {
            let v = json_body(&body)?;
            let store = &st.engine()?.store;
            let doc = v["document_id"]
                .as_str()
                .and_then(|id| store.document(id).transpose())
                .transpose()
                .map_err(store_error)?
                .ok_or_else(|| {
                    field_error(
                        "document_id",
                        "value_error",
                        "No such document",
                        &v["document_id"],
                    )
                })?;
            let link = evidence_target(&st, &doc, &v["claim_id"], &v["kpi_key"])?;
            let created = store_link(&st, &doc.ticker, &link)?;
            Ok((StatusCode::CREATED, link_json(&link, created)))
        },
    )
}

/// `GET /api/research/claims/{id}/evidence`: the documents attached to a claim (**Bukti**).
pub(super) async fn claim_evidence(
    State(st): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = &st.engine()?.store;
    let claim = store
        .claim(&id)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Claim not found"))?;
    let links = store.evidence(&claim.ticker).map_err(store_error)?;
    let mut items = Vec::new();
    for link in links.iter().filter(|e| e.claim_id.as_deref() == Some(&id)) {
        if let Some(d) = store.document(&link.document_id).map_err(store_error)? {
            items.push(document_json(&d, &[link]));
        }
    }
    items.sort_by(|a, b| {
        b["doc_date"]
            .as_str()
            .cmp(&a["doc_date"].as_str())
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    Ok(out(
        json!({"claim_id": id, "ticker": claim.ticker, "items": items}),
    ))
}

// ---- calibration ----

#[derive(serde::Deserialize)]
pub(super) struct CalibrationQuery {
    ticker: Option<String>,
    r#type: Option<String>,
}

/// `GET /api/research/calibration?ticker=&type=klaim|level|keputusan`: every journal level
/// and decision and every claim with the price move since the day it was written, shown once
/// 30 days have passed (A4.4); younger rows are listed without a return and the day they
/// mature. The footer is the claims that held against the claims that broke, per ticker.
pub(super) async fn calibration_get(
    State(st): State<ApiState>,
    Query(q): Query<CalibrationQuery>,
) -> Result<Json<Value>, ApiError> {
    let kind = match q.r#type.as_deref() {
        None | Some("") => None,
        Some(s) => Some(calibration::Kind::parse(s).ok_or_else(|| {
            ApiError::invalid(
                &["query", "type"],
                "enum",
                "Input should be 'klaim', 'level' or 'keputusan'",
                json!(s),
            )
        })?),
    };
    let engine = st.engine()?;
    let store = &engine.store;
    let today = engine.today();
    let tickers: Vec<String> = match q.ticker.as_deref().filter(|t| !t.is_empty()) {
        Some(raw) => vec![canon_ticker(&st, raw)?],
        None => store
            .universe()
            .map_err(store_error)?
            .into_iter()
            .map(|e| e.ticker)
            .collect(),
    };
    let day = journal::wib_day;
    let mut rows = Vec::new();
    let mut statuses = Vec::new();
    for t in &tickers {
        let mut series: Vec<(NaiveDate, f64)> = store
            .prices(t)
            .map_err(store_error)?
            .into_iter()
            .filter_map(|b| {
                Some((
                    NaiveDate::parse_from_str(&b.date, "%Y-%m-%d").ok()?,
                    b.close,
                ))
            })
            .filter(|(d, _)| *d <= today)
            .collect();
        series.sort_by_key(|(d, _)| *d);
        let splits = split_ratios(store, t).map_err(store_error)?;
        let mut entries = Vec::new();
        for j in store.journal(Some(t)).map_err(store_error)? {
            let kind = match j.kind.as_str() {
                "level" => calibration::Kind::Level,
                "keputusan" => calibration::Kind::Decision,
                _ => continue,
            };
            let Some(date) = day(&j.at) else { continue };
            entries.push(calibration::Entry {
                id: j.id,
                ticker: t.clone(),
                kind,
                date,
                text: j.text,
                price_then: j.price_at,
                level: j.level,
                status: None,
            });
        }
        for c in store.claims(Some(t)).map_err(store_error)? {
            statuses.push((t.clone(), c.status.clone()));
            let Some(date) = day(&c.created_at) else {
                continue;
            };
            entries.push(calibration::Entry {
                id: c.id,
                ticker: t.clone(),
                kind: calibration::Kind::Claim,
                date,
                text: c.statement,
                price_then: None,
                level: None,
                status: Status::parse(&c.status).map(Status::label),
            });
        }
        rows.extend(
            entries
                .iter()
                .filter(|e| kind.is_none_or(|k| e.kind == k))
                .map(|e| calibration::row(e, &series, &splits, today)),
        );
    }
    let rows = calibration::sorted(rows);
    let matured = rows.iter().filter(|r| r.matured).count();
    Ok(out(json!({
        "as_of": engine.now().to_rfc3339(),
        "ticker": q.ticker.as_deref().filter(|t| !t.is_empty()).map(str::to_uppercase),
        "type": kind.map(calibration::Kind::as_str),
        "maturity_days": calibration::MATURITY_DAYS,
        "total": rows.len(), "matured": matured, "pending": rows.len() - matured,
        "items": rows,
        "hit_rates": calibration::hit_rates(&statuses),
    })))
}
