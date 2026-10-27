//! R8 handlers (research plan §5, §6 R8; prd.md A8.1 to A8.3): upload a PDF or presentation
//! to a dossier, read the rule-based KPI proposals with their page references, and accept or
//! reject each. Nothing reaches `kpi_values` without an accept; an accept writes through the
//! same KPI store as `PUT /{ticker}/kpi/...` with the document as provenance and an evidence
//! link, and a reject stores nothing. No model and no network: the extraction is
//! `research::extract`. Same conventions as the rest of the API: writes need an
//! `Idempotency-Key`, 422 bodies name the field.

use std::collections::{BTreeMap, HashMap};
use std::path::Path as FsPath;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::r4::document_json;
use super::{
    body_hash, canon_ticker, field_error, idempotent, json_body, kpi_target, map_kpi_field, out,
    reevaluate_claims, store_error, ApiError, ApiState,
};
use crate::research::documents;
use crate::research::extract::doc::{self, DocError, Format};
use crate::research::extract::isolated;
use crate::research::extract::rules::{self, Target};
use crate::research::extract::{self, clean_filename};
use crate::research::kpi::{self, KpiValue};
use crate::research::packs::def::CORE_ID;
use crate::research::packs::{Cadence, PackSet};
use crate::research::store::{DocumentRow, EvidenceRow, ProposalRow};
use crate::research::universe;

/// Multipart framing around the largest file.
pub(super) const UPLOAD_BODY_LIMIT: usize = doc::MAX_UPLOAD_BYTES + 256 * 1024;

const STATUSES: [&str; 4] = ["pending", "accepted", "rejected", "superseded"];

struct Form {
    file: Option<(String, Bytes)>,
    kind: Option<String>,
    doc_date: Option<String>,
    fy: Option<String>,
    q: Option<String>,
}

fn bad_form(msg: &str) -> ApiError {
    ApiError::invalid(&["body"], "multipart_invalid", msg, Value::Null)
}

/// Reads `file`, `kind`, `doc_date`, `fy` and `q` from a `multipart/form-data` body.
async fn read_form(headers: &HeaderMap, body: Bytes) -> Result<Form, ApiError> {
    let boundary = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| multer::parse_boundary(ct).ok())
        .ok_or_else(|| bad_form("Send multipart/form-data with a file field"))?;
    let stream = futures::stream::once(async move { Ok::<Bytes, std::io::Error>(body) });
    let mut mp = multer::Multipart::new(stream, boundary);
    let mut form = Form {
        file: None,
        kind: None,
        doc_date: None,
        fy: None,
        q: None,
    };
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|_| bad_form("The multipart body could not be read"))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or_default().to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|_| bad_form("The file part could not be read"))?;
            form.file = Some((filename, bytes));
            continue;
        }
        let slot = match name.as_str() {
            "kind" => &mut form.kind,
            "doc_date" => &mut form.doc_date,
            "fy" => &mut form.fy,
            "q" => &mut form.q,
            _ => continue,
        };
        *slot = Some(
            field
                .text()
                .await
                .map_err(|_| bad_form("A form field could not be read"))?,
        );
    }
    Ok(form)
}

/// The KPIs of the pack in effect and of core, as extraction targets, with each one's
/// cadence. `None` for the pack when the ticker has none.
struct Targets {
    pack: Option<(String, String)>,
    targets: Vec<Target>,
    cadence: HashMap<String, Cadence>,
}

fn targets_of(st: &ApiState, ticker: &str) -> Result<Targets, ApiError> {
    let store = &st.engine()?.store;
    let entry = store
        .universe_entry(ticker)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let packs: std::sync::Arc<PackSet> = st.pack_set();
    let choice = universe::pack_choice(&packs, &entry, codes.get(ticker).map(String::as_str));
    let mut out = Targets {
        pack: None,
        targets: Vec::new(),
        cadence: HashMap::new(),
    };
    let chosen = choice.id.as_deref().and_then(|id| packs.get(id));
    if let Some(p) = chosen {
        out.pack = Some((p.id.clone(), p.label.clone()));
    }
    for pack in chosen.into_iter().chain(packs.get(CORE_ID)) {
        for m in pack.kpi_metrics() {
            if out.cadence.contains_key(&m.id) {
                continue;
            }
            out.cadence
                .insert(m.id.clone(), m.cadence.unwrap_or(Cadence::Quarterly));
            out.targets.push(Target {
                metric: m.id.clone(),
                label: m.label.clone(),
                aliases: m.extract_aliases.clone(),
                unit: m.unit.clone(),
            });
        }
    }
    Ok(out)
}

/// The format an uploaded document was stored as, read back from its `text`.
fn format_of(d: &DocumentRow) -> Option<Format> {
    let text = d.text.as_deref()?;
    if text.starts_with("Unggahan PDF") {
        Some(Format::Pdf)
    } else if text.starts_with("Unggahan PPTX") {
        Some(Format::Pptx)
    } else {
        None
    }
}

fn proposal_json(p: &ProposalRow, locator: Option<Format>, label: Option<&str>) -> Value {
    let (metric, fy, q) = documents::parse_kpi_key(&p.kpi_key)
        .map_or((None, None, None), |(m, fy, q)| {
            (Some(m), Some(fy), Some(q))
        });
    json!({
        "id": p.id, "document_id": p.document_id, "ticker": p.ticker,
        "kpi_key": p.kpi_key, "metric": metric, "metric_label": label, "fy": fy, "q": q,
        "value": p.value, "unit": p.unit,
        "page": p.page, "locator": locator.map(Format::locator),
        "snippet": p.snippet, "rank": p.rank, "status": p.status,
        "created_at": p.created_at, "decided_at": p.decided_at,
    })
}

fn counts(items: &[ProposalRow]) -> Value {
    let n = |s: &str| items.iter().filter(|p| p.status == s).count();
    json!({
        "pending": n("pending"), "accepted": n("accepted"), "rejected": n("rejected"),
        "superseded": n("superseded"),
    })
}

/// Proposals with their labels and locators, as a list payload.
fn proposals_payload(
    st: &ApiState,
    ticker: &str,
    items: &[ProposalRow],
) -> Result<Vec<Value>, ApiError> {
    let store = &st.engine()?.store;
    let targets = targets_of(st, ticker)?;
    let labels: HashMap<&str, &str> = targets
        .targets
        .iter()
        .map(|t| (t.metric.as_str(), t.label.as_str()))
        .collect();
    let mut formats: HashMap<String, Option<Format>> = HashMap::new();
    let mut out = Vec::new();
    for p in items {
        let locator = match formats.get(&p.document_id) {
            Some(f) => *f,
            None => {
                let f = store
                    .document(&p.document_id)
                    .map_err(store_error)?
                    .and_then(|d| format_of(&d));
                formats.insert(p.document_id.clone(), f);
                f
            }
        };
        let label = documents::parse_kpi_key(&p.kpi_key)
            .and_then(|(m, _, _)| labels.get(m.as_str()).copied());
        out.push(proposal_json(p, locator, label));
    }
    Ok(out)
}

fn store_file(dir: &FsPath, sha: &str, ext: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{sha}.{ext}"));
    if path.exists() {
        return Ok(());
    }
    let tmp = dir.join(format!(".{sha}.{ext}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)
}

/// Result of reading an upload: the text per page and the proposals found in it.
struct Extracted {
    pages: usize,
    pages_with_text: usize,
    proposals: Vec<rules::Proposal>,
}

/// `POST /api/research/{ticker}/documents/upload`, `multipart/form-data` with `file` (PDF or
/// PPTX, at most 20 MB, recognised by its first bytes), `kind`, `doc_date` and the reported
/// period `fy` and `q`. The file is kept as `<sha256>.<ext>` (the same bytes are stored once),
/// a document row is filed through the same path as `POST /{ticker}/documents`, and the
/// rule-based proposals are stored `pending`. 201 the first time, 200 with `created: false`
/// for a file already filed for that ticker and period.
pub(super) async fn upload(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/documents/upload");
    // Fail on a missing key before reading 20 MB.
    if headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .is_none_or(|k| k.trim().is_empty())
    {
        return idempotent(&st, &headers, "POST", &path, &body, |_| {
            Err(ApiError::new(StatusCode::BAD_REQUEST, "unreachable"))
        });
    }
    let form = read_form(&headers, body.clone()).await?;
    let engine = st.engine()?.clone();
    let up = extract::check_upload(
        form.kind.as_deref(),
        form.doc_date.as_deref(),
        form.fy.as_deref(),
        form.q.as_deref(),
        engine.today(),
    )
    .map_err(|e| {
        let input = match e.field {
            "kind" => json!(form.kind),
            "doc_date" => json!(form.doc_date),
            "fy" => json!(form.fy),
            _ => json!(form.q),
        };
        field_error(e.field, e.kind, e.message, &input)
    })?;
    let Some((filename, file)) = form.file else {
        return Err(field_error(
            "file",
            "missing",
            "Field required",
            &Value::Null,
        ));
    };
    if file.len() > doc::MAX_UPLOAD_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The file is larger than 20 MB",
        ));
    }
    let filename = clean_filename(&filename);
    let format = doc::sniff(&file).ok_or_else(|| {
        field_error(
            "file",
            "file_type",
            "Only PDF and PPTX files are accepted",
            &json!(filename),
        )
    })?;
    let sha = hex::encode(Sha256::digest(&file));
    let targets = targets_of(&st, &ticker)?;
    let (annual_q4, cadence) = (up.q == 4, targets.cadence.clone());
    let to_read = targets.targets.clone();
    let unreadable =
        |e: DocError| field_error("file", "file_unreadable", e.message(), &json!(filename));
    // The file is parsed by a child process under a time, memory and output budget: a
    // crash or a runaway there is one refused upload (QA H1, H2).
    let pages = match isolated::pages(format, file.to_vec(), &isolated::Limits::default()).await {
        Ok(p) => p,
        Err(DocError::Unavailable(m)) => return Err(store_error(m.to_string())),
        Err(e) => return Err(unreadable(e)),
    };
    // The rules are pure and linear, but a panic in them is still this file's problem only.
    let extracted = tokio::task::spawn_blocking(move || {
        let proposals = rules::propose(&to_read, &pages, &|t: &Target| {
            annual_q4 || cadence.get(&t.metric) != Some(&Cadence::Annual)
        });
        Extracted {
            pages: pages.len(),
            pages_with_text: pages.iter().filter(|p| !p.trim().is_empty()).count(),
            proposals,
        }
    })
    .await
    .map_err(|_| {
        unreadable(DocError::Unreadable(
            "The file could not be read (the text rules failed on it)",
        ))
    })?;

    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let id = format!(
            "u-{}",
            &body_hash(
                "upload",
                &ticker,
                format!("{sha}:{}Q{}", up.fy, up.q).as_bytes()
            )[..24]
        );
        let store = &engine.store;
        let now = engine.now().to_rfc3339();
        let existing = store.document(&id).map_err(store_error)?;
        let created = existing.is_none();
        let row = match existing {
            Some(d) => d,
            None => {
                store_file(&st.documents_dir, &sha, format.ext(), &file).map_err(store_error)?;
                let unit = if format == Format::Pdf {
                    "halaman"
                } else {
                    "slide"
                };
                let row = DocumentRow {
                    id: id.clone(),
                    ticker: ticker.clone(),
                    kind: up.kind.clone(),
                    url: None,
                    text: Some(format!(
                        "Unggahan {}: {filename} \u{b7} {} {unit} \u{b7} sha256 {sha}",
                        format.label(),
                        extracted.pages
                    )),
                    doc_date: Some(up.doc_date.to_string()),
                    added_at: now.clone(),
                };
                store.add_document(&row).map_err(store_error)?;
                row
            }
        };
        // The same ids every time: a retry after a crash fills in what is missing and never
        // reopens a decided proposal.
        let rows: Vec<ProposalRow> = extracted
            .proposals
            .iter()
            .map(|p| ProposalRow {
                id: format!(
                    "p-{}",
                    &body_hash(
                        "proposal",
                        &id,
                        format!("{}:{}", p.metric, p.rank).as_bytes()
                    )[..16]
                ),
                document_id: id.clone(),
                ticker: ticker.clone(),
                kpi_key: documents::kpi_key(&p.metric, up.fy, up.q),
                value: p.value,
                unit: p.unit.clone(),
                page: p.page,
                snippet: p.snippet.clone(),
                rank: p.rank,
                status: "pending".into(),
                created_at: now.clone(),
                decided_at: None,
            })
            .collect();
        store.add_proposals(&rows).map_err(store_error)?;
        let stored = store
            .proposals(&ticker, Some(&id), None)
            .map_err(store_error)?;
        let links = store.evidence(&ticker).map_err(store_error)?;
        let own: Vec<&EvidenceRow> = links.iter().filter(|e| e.document_id == id).collect();
        let items = proposals_payload(&st, &ticker, &stored)?;
        let status = if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        Ok((
            status,
            json!({
                "created": created,
                "document": document_json(&row, &own),
                "upload": {
                    "sha256": sha, "format": format.ext(), "bytes": file.len(),
                    "pages": extracted.pages, "pages_with_text": extracted.pages_with_text,
                    "locator": format.locator(),
                },
                "period": {"fy": up.fy, "q": up.q, "label": format!("{}Q{}", up.fy, up.q)},
                "pack": targets.pack.as_ref().map(|(id, label)| json!({"id": id, "label": label})),
                "proposals": items,
                "counts": counts(&stored),
            }),
        ))
    })
}

#[derive(serde::Deserialize)]
pub(super) struct ProposalsQuery {
    document_id: Option<String>,
    status: Option<String>,
}

/// `GET /api/research/{ticker}/proposals?document_id=&status=`: the extraction proposals of
/// the ticker (of one document with `document_id`), grouped by document and KPI cell with the
/// best candidate first.
pub(super) async fn proposals_list(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    Query(q): Query<ProposalsQuery>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    if let Some(s) = q.status.as_deref() {
        if !STATUSES.contains(&s) {
            return Err(ApiError::invalid(
                &["query", "status"],
                "enum",
                "Input should be 'pending', 'accepted', 'rejected' or 'superseded'",
                json!(s),
            ));
        }
    }
    let store = &st.engine()?.store;
    let all = store
        .proposals(&ticker, q.document_id.as_deref(), None)
        .map_err(store_error)?;
    let shown: Vec<ProposalRow> = all
        .iter()
        .filter(|p| q.status.as_deref().is_none_or(|s| p.status == s))
        .cloned()
        .collect();
    let items = proposals_payload(&st, &ticker, &shown)?;
    let mut docs: BTreeMap<String, Value> = BTreeMap::new();
    for p in &all {
        if docs.contains_key(&p.document_id) {
            continue;
        }
        if let Some(d) = store.document(&p.document_id).map_err(store_error)? {
            docs.insert(
                d.id.clone(),
                json!({
                    "id": d.id, "kind": d.kind, "kind_label": documents::kind_label(&d.kind),
                    "doc_date": d.doc_date, "added_at": d.added_at,
                    "format": format_of(&d).map(Format::ext),
                    "locator": format_of(&d).map(Format::locator),
                }),
            );
        }
    }
    Ok(out(json!({
        "ticker": ticker,
        "document_id": q.document_id,
        "items": items,
        "counts": counts(&all),
        "documents": docs.into_values().collect::<Vec<_>>(),
    })))
}

/// The pending proposal `id` and its ticker, or 404 / 409.
fn pending(st: &ApiState, id: &str) -> Result<ProposalRow, ApiError> {
    let engine = st.engine()?;
    let p = engine
        .store
        .proposal(id)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "No such proposal"))?;
    canon_ticker(st, &p.ticker)?;
    if p.status != "pending" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("The proposal was already {}", p.status),
        ));
    }
    Ok(p)
}

fn replace_flag(body: &Bytes) -> Result<bool, ApiError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(false);
    }
    let v = json_body(body)?;
    match v.get("replace") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(field_error(
            "replace",
            "bool_type",
            "Input should be a boolean",
            other,
        )),
    }
}

/// `POST /api/research/proposals/{id}/accept` with an optional `{"replace": true}`: writes
/// the KPI value through the KPI store (document as provenance, the document linked as
/// evidence of the cell) and marks the proposal `accepted`. 409 when the cell already holds
/// a value, unless `replace` is true; 409 when the proposal was already decided or its KPI
/// left the pack in effect.
pub(super) async fn proposal_accept(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/proposals/{id}/accept");
    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let replace = replace_flag(&body)?;
        let p = pending(&st, &id)?;
        let engine = st.engine()?;
        let store = &engine.store;
        let conflict = |msg: &str| ApiError::new(StatusCode::CONFLICT, msg.to_string());
        let (metric, fy, q) = documents::parse_kpi_key(&p.kpi_key)
            .ok_or_else(|| conflict("The proposal has no readable KPI cell"))?;
        let (packs, pack_id, metric_id) = kpi_target(&st, &p.ticker, &metric)
            .map_err(|_| conflict("The KPI is no longer in the pack in effect"))?;
        let m = packs
            .get(&pack_id)
            .and_then(|pack| pack.metric(&metric_id))
            .ok_or_else(|| conflict("The KPI is no longer in the pack in effect"))?;
        if m.unit != p.unit {
            return Err(conflict(
                "The pack's unit for this KPI changed since extraction",
            ));
        }
        let cadence = m.cadence.unwrap_or(Cadence::Quarterly);
        let (fy_n, q_n) = kpi::parse_period(cadence, &fy.to_string(), &q.to_string())
            .map_err(|e| map_kpi_field(&e))?;
        let existing = store
            .kpi(&p.ticker, &metric_id, fy_n, q_n)
            .map_err(store_error)?;
        if let (Some(old), false) = (&existing, replace) {
            let held = match (old.unknown, old.value) {
                (false, Some(v)) => format!("{v} {}", old.unit),
                _ => "unknown".to_string(),
            };
            return Err(conflict(&format!(
                "{metric_id} {fy}Q{q} already holds {held} ({}); send {{\"replace\": true}} to replace it",
                old.document
            )));
        }
        let doc = store
            .document(&p.document_id)
            .map_err(store_error)?
            .ok_or_else(|| conflict("The proposal's document is gone"))?;
        let locator = match format_of(&doc) {
            Some(Format::Pptx) => "slide",
            _ => "hlm",
        };
        let how = format_of(&doc).map_or("unggahan", |f| f.label());
        let reference = format!(
            "{} {} (unggahan {how}), {locator} {}",
            documents::kind_label(&doc.kind).unwrap_or("Dokumen"),
            doc.doc_date.clone().unwrap_or_default(),
            p.page
        );
        let document_date = doc
            .doc_date
            .clone()
            .unwrap_or_else(|| engine.today().to_string());
        let row = KpiValue {
            ticker: p.ticker.clone(),
            metric: metric_id,
            fy: fy_n,
            q: q_n,
            value: Some(p.value),
            unknown: false,
            unit: m.unit.clone(),
            document: reference,
            document_date,
            entered_at: engine.now().to_rfc3339(),
        };
        store.set_kpi(&row).map_err(store_error)?;
        let link = EvidenceRow {
            claim_id: None,
            kpi_key: Some(documents::kpi_key(&row.metric, fy_n, q_n)),
            document_id: doc.id.clone(),
        };
        store.link_evidence(&link).map_err(store_error)?;
        let at = engine.now().to_rfc3339();
        store
            .decide_proposal(&p.id, "accepted", &at)
            .map_err(store_error)?;
        // One decision per cell: the siblings of this candidate (and a value this one
        // replaced) are done, so a later replace cannot leave two "accepted" rows that
        // disagree with the cell.
        let superseded = store
            .supersede_around(&p, existing.is_some(), &at)
            .map_err(store_error)?;
        reevaluate_claims(&st, &p.ticker);
        let decided = store
            .proposal(&p.id)
            .map_err(store_error)?
            .unwrap_or(ProposalRow {
                status: "accepted".into(),
                ..p
            });
        let mut kpi_json = row.to_json();
        kpi_json["ticker"] = json!(row.ticker);
        kpi_json["metric"] = json!(row.metric);
        kpi_json["pack"] = json!(pack_id);
        Ok((
            StatusCode::OK,
            json!({
                "proposal": proposal_json(&decided, format_of(&doc), Some(&m.label)),
                "kpi": kpi_json,
                "evidence": {"document_id": link.document_id, "kpi_key": link.kpi_key},
                "replaced": existing.is_some(),
                "superseded": superseded,
            }),
        ))
    })
}

/// `POST /api/research/proposals/{id}/reject`: marks the proposal `rejected` and stores
/// nothing else (no KPI value, no evidence link). 409 when it was already decided.
pub(super) async fn proposal_reject(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = format!("/api/research/proposals/{id}/reject");
    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let p = pending(&st, &id)?;
        let engine = st.engine()?;
        engine
            .store
            .decide_proposal(&p.id, "rejected", &engine.now().to_rfc3339())
            .map_err(store_error)?;
        let decided = engine
            .store
            .proposal(&p.id)
            .map_err(store_error)?
            .unwrap_or(ProposalRow {
                status: "rejected".into(),
                ..p
            });
        let format = engine
            .store
            .document(&decided.document_id)
            .map_err(store_error)?
            .and_then(|d| format_of(&d));
        Ok((
            StatusCode::OK,
            json!({"proposal": proposal_json(&decided, format, None)}),
        ))
    })
}
