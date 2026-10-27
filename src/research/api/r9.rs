//! Structure map handlers (Riset redesign G1, ui-slicing.md §7 and §8 D3/D4): read the map
//! of a ticker, edit or accept or reject a node, add or remove an owner node, and import the
//! draft Claude Code wrote to `<data_dir>/research/structure/<TICKER>.draft.json`. The app
//! only loads and validates the draft; nothing in it becomes part of the map until the owner
//! accepts it. Same conventions as the rest of the API: writes need an `Idempotency-Key`,
//! 422 bodies name the field (a draft file's fields are located under `file`).

use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};

use super::{
    canon_ticker, field_error, idempotent, json_body, out, store_error, ApiError, ApiState,
};
use crate::research::packs::def::CORE_ID;
use crate::research::store::{StructureMap, StructureNode};
use crate::research::structure::{
    self as st_rules, Violation, ACCEPTED, MAX_DEPTH, MAX_DRAFT_BYTES, MAX_LABEL_CHARS, MAX_NODES,
    MAX_NOTE_CHARS, PROPOSED, REJECTED,
};
use crate::research::universe;

struct MetricRef {
    id: String,
    label: String,
    unit: String,
    /// `true` for a metric of the ticker's pack, `false` for a core metric.
    pack: bool,
}

struct Context {
    pack: Option<(String, String)>,
    metrics: Vec<MetricRef>,
}

impl Context {
    fn ids(&self) -> BTreeSet<String> {
        self.metrics.iter().map(|m| m.id.clone()).collect()
    }
}

/// The metric ids a node of `ticker` may link to: its pack's and core's.
fn context(st: &ApiState, ticker: &str) -> Result<Context, ApiError> {
    let store = &st.engine()?.store;
    let entry = store
        .universe_entry(ticker)
        .map_err(store_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown ticker"))?;
    let codes = store.latest_industry_codes().map_err(store_error)?;
    let packs = st.pack_set();
    let choice = universe::pack_choice(&packs, &entry, codes.get(ticker).map(String::as_str));
    let chosen = choice.id.as_deref().and_then(|id| packs.get(id));
    let mut metrics = Vec::new();
    for (pack, is_pack) in [(chosen, true), (packs.get(CORE_ID), false)] {
        for m in pack.into_iter().flat_map(|p| p.metrics()) {
            if metrics.iter().any(|x: &MetricRef| x.id == m.id) {
                continue;
            }
            metrics.push(MetricRef {
                id: m.id.clone(),
                label: m.label.clone(),
                unit: m.unit.clone(),
                pack: is_pack,
            });
        }
    }
    Ok(Context {
        pack: chosen.map(|p| (p.id.clone(), p.label.clone())),
        metrics,
    })
}

impl ApiState {
    /// `<data_dir>/research/structure`, beside the documents directory.
    fn structure_dir(&self) -> PathBuf {
        self.documents_dir
            .parent()
            .unwrap_or(&self.documents_dir)
            .join("structure")
    }
}

fn draft_path(st: &ApiState, ticker: &str) -> PathBuf {
    st.structure_dir().join(format!("{ticker}.draft.json"))
}

fn invalid(violations: Vec<Violation>) -> ApiError {
    ApiError::Validation(violations.iter().map(Violation::entry).collect())
}

fn node_json(n: &StructureNode, ctx: &Context) -> Value {
    json!({
        "id": n.id, "parent": n.parent, "label": n.label,
        "kind": n.kind, "kind_label": st_rules::kind_label(&n.kind),
        "metric": n.metric,
        "metric_label": n.metric.as_deref()
            .and_then(|id| ctx.metrics.iter().find(|m| m.id == id))
            .map(|m| m.label.clone()),
        "note": n.note,
        "state": n.state, "state_label": st_rules::state_label(&n.state),
        "owner": n.owner,
        "source": n.source.as_ref().map(|s| json!({
            "document": s.document, "page": s.page, "snippet": s.snippet,
        })),
    })
}

fn payload(st: &ApiState, ticker: &str, map: Option<&StructureMap>) -> Result<Value, ApiError> {
    let ctx = context(st, ticker)?;
    let nodes: &[StructureNode] = map.map_or(&[], |m| m.nodes.as_slice());
    let n = |s: &str| nodes.iter().filter(|x| x.state == s).count();
    Ok(json!({
        "ticker": ticker,
        "pack": ctx.pack.as_ref().map(|(id, label)| json!({"id": id, "label": label})),
        "metrics": ctx.metrics.iter().map(|m| json!({
            "id": m.id, "label": m.label, "unit": m.unit, "pack": m.pack,
        })).collect::<Vec<_>>(),
        "kinds": st_rules::KINDS.iter().map(|(id, label)| json!({"id": id, "label": label})).collect::<Vec<_>>(),
        "nodes": nodes.iter().map(|x| node_json(x, &ctx)).collect::<Vec<_>>(),
        "counts": {"proposed": n(PROPOSED), "accepted": n(ACCEPTED), "rejected": n(REJECTED)},
        "updated_at": map.map(|m| m.updated_at.clone()),
        "limits": {
            "max_depth": MAX_DEPTH, "max_nodes": MAX_NODES,
            "max_label": MAX_LABEL_CHARS, "max_note": MAX_NOTE_CHARS,
        },
        "draft": {
            "file": format!("{ticker}.draft.json"),
            "available": draft_path(st, ticker).is_file(),
        },
    }))
}

fn load(st: &ApiState, ticker: &str) -> Result<Option<StructureMap>, ApiError> {
    st.engine()?.store.structure(ticker).map_err(store_error)
}

/// Metric ids the document may carry: the ticker's own, plus any an earlier write already
/// stored (a pack switch must not make every later edit fail on an old link).
fn allowed_metrics(ctx: &Context, nodes: &[StructureNode]) -> BTreeSet<String> {
    let mut ids = ctx.ids();
    ids.extend(nodes.iter().filter_map(|n| n.metric.clone()));
    ids
}

fn save(st: &ApiState, ticker: &str, nodes: &[StructureNode]) -> Result<StructureMap, ApiError> {
    let engine = st.engine()?;
    let at = engine.now().to_rfc3339();
    engine
        .store
        .set_structure(ticker, nodes, &at)
        .map_err(store_error)?;
    Ok(StructureMap {
        nodes: nodes.to_vec(),
        updated_at: at,
    })
}

/// `GET /api/research/{ticker}/structure`
pub(super) async fn structure_get(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let map = load(&st, &ticker)?;
    Ok(out(payload(&st, &ticker, map.as_ref())?))
}

const PATCH_KEYS: [&str; 5] = ["label", "kind", "metric", "note", "state"];
const CREATE_KEYS: [&str; 5] = ["label", "kind", "parent", "metric", "note"];

fn reject_extra(v: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    let Some(obj) = v.as_object() else {
        return Err(ApiError::invalid(
            &["body"],
            "model_type",
            "Input should be a JSON object",
            v.clone(),
        ));
    };
    let extra: Vec<Value> = obj
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .map(|k| {
            json!({"type": "extra_forbidden", "loc": ["body", k],
                   "msg": "Extra inputs are not permitted", "input": obj[k]})
        })
        .collect();
    if extra.is_empty() {
        Ok(())
    } else {
        Err(ApiError::Validation(extra))
    }
}

/// `Some(None)` for `null` or `""` (clear), `Some(Some(s))` for text, `None` when absent.
fn nullable_text(v: &Value, key: &str) -> Result<Option<Option<String>>, ApiError> {
    match v.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.trim().to_string()))),
        Some(other) => Err(field_error(
            key,
            "string_type",
            "Input should be a string or null",
            other,
        )),
    }
}

fn required_text(v: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match v.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_string())),
        Some(other) => Err(field_error(
            key,
            "string_type",
            "Input should be a string",
            other,
        )),
    }
}

fn own_violations(n: &StructureNode, ctx: &Context, nodes: &[StructureNode]) -> Vec<Violation> {
    let loc = [json!("body")];
    st_rules::node_violations(n, &allowed_metrics(ctx, nodes), &loc)
}

/// `PUT /api/research/{ticker}/structure/nodes/{id}`: edit label, kind, metric or note, and
/// accept or reject the node. A field that is absent stays as it is.
pub(super) async fn node_put(
    State(st): State<ApiState>,
    Path((raw, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/structure/nodes/{id}");
    idempotent(&st, &headers, "PUT", &path, &body, |_| {
        let v = json_body(&body)?;
        reject_extra(&v, &PATCH_KEYS)?;
        let mut nodes = load(&st, &ticker)?.map(|m| m.nodes).unwrap_or_default();
        let idx = nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown structure node"))?;
        let ctx = context(&st, &ticker)?;
        let mut node = nodes[idx].clone();
        let mut changed = false;
        if let Some(label) = required_text(&v, "label")? {
            node.label = label;
            changed = true;
        }
        if let Some(kind) = required_text(&v, "kind")? {
            node.kind = kind;
            changed = true;
        }
        if let Some(metric) = nullable_text(&v, "metric")? {
            node.metric = metric;
            changed = true;
        }
        if let Some(note) = nullable_text(&v, "note")? {
            node.note = note;
            changed = true;
        }
        let state = match v.get("state") {
            None => None,
            Some(Value::String(s)) if s == ACCEPTED || s == REJECTED => Some(s.clone()),
            Some(other) => {
                return Err(field_error(
                    "state",
                    "enum",
                    "Input should be 'accepted' or 'rejected'",
                    other,
                ))
            }
        };
        if !changed && state.is_none() {
            return Err(ApiError::invalid(
                &["body"],
                "missing",
                "Send at least one of label, kind, metric, note or state",
                Value::Null,
            ));
        }
        // The state is applied below; validate the fields as they will be stored.
        let mut check = node.clone();
        if let Some(s) = &state {
            check.state.clone_from(s);
        }
        let problems = own_violations(&check, &ctx, &nodes);
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        nodes[idx] = node;
        if let Some(s) = &state {
            st_rules::set_state(&mut nodes, &id, s);
        }
        // Accepting a rejected node makes it count again: the whole map keeps the cap.
        let problems = st_rules::map_violations(&nodes, &allowed_metrics(&ctx, &nodes), &["body"]);
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        let map = save(&st, &ticker, &nodes)?;
        Ok((StatusCode::OK, payload(&st, &ticker, Some(&map))?))
    })
}

/// `POST /api/research/{ticker}/structure/nodes`: a branch the owner adds (accepted at once,
/// no source needed).
pub(super) async fn node_post(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/structure/nodes");
    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let v = json_body(&body)?;
        reject_extra(&v, &CREATE_KEYS)?;
        let mut nodes = load(&st, &ticker)?.map(|m| m.nodes).unwrap_or_default();
        let ctx = context(&st, &ticker)?;
        let label = required_text(&v, "label")?.unwrap_or_default();
        let kind = required_text(&v, "kind")?.unwrap_or_else(|| "lainnya".to_string());
        let parent = nullable_text(&v, "parent")?.flatten();
        if let Some(p) = &parent {
            match nodes.iter().find(|n| n.id == *p) {
                None => {
                    return Err(field_error(
                        "parent",
                        "parent_missing",
                        &format!("Parent `{p}` is not a node of this map"),
                        &v["parent"],
                    ))
                }
                Some(n) if n.state == REJECTED => {
                    return Err(field_error(
                        "parent",
                        "parent_rejected",
                        "A branch cannot be added under a rejected node",
                        &v["parent"],
                    ))
                }
                Some(_) => {}
            }
        }
        let node = StructureNode {
            id: format!("o-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            parent,
            label,
            kind,
            metric: nullable_text(&v, "metric")?.flatten(),
            note: nullable_text(&v, "note")?.flatten(),
            state: ACCEPTED.into(),
            owner: true,
            source: None,
        };
        let problems = own_violations(&node, &ctx, &nodes);
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        nodes.push(node.clone());
        let problems = st_rules::map_violations(&nodes, &allowed_metrics(&ctx, &nodes), &["body"]);
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        let map = save(&st, &ticker, &nodes)?;
        let mut res = payload(&st, &ticker, Some(&map))?;
        res["node"] = node_json(&node, &ctx);
        Ok((StatusCode::CREATED, res))
    })
}

/// `DELETE /api/research/{ticker}/structure/nodes/{id}`: only a node the owner added, and
/// only when no branch that is not rejected hangs under it. A drafted node is rejected, not
/// deleted.
pub(super) async fn node_delete(
    State(st): State<ApiState>,
    Path((raw, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/structure/nodes/{id}");
    idempotent(&st, &headers, "DELETE", &path, &Bytes::new(), |_| {
        let mut nodes = load(&st, &ticker)?.map(|m| m.nodes).unwrap_or_default();
        let node = nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Unknown structure node"))?;
        if !node.owner {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Only a node you added can be deleted; reject a drafted node instead",
            ));
        }
        if nodes
            .iter()
            .any(|n| n.parent.as_deref() == Some(id.as_str()) && n.state != REJECTED)
        {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "This node has branches under it; remove them first",
            ));
        }
        // Rejected branches below it go with it: the page hides them, so they could not be
        // removed first (a kept node under a rejected one was lifted by `heal`).
        let gone = st_rules::subtree(&nodes, &id);
        nodes.retain(|n| !gone.contains(&n.id));
        let map = save(&st, &ticker, &nodes)?;
        Ok((StatusCode::OK, payload(&st, &ticker, Some(&map))?))
    })
}

/// `POST /api/research/{ticker}/structure/draft`: validates `<TICKER>.draft.json` completely
/// and imports it as proposed nodes. Proposed and rejected nodes of the old map are replaced;
/// accepted nodes (the owner's own among them) stay. 404 when there is no file, 422 with
/// every violation (located under `file`) when it breaks a rule; nothing is imported then.
pub(super) async fn draft_import(
    State(st): State<ApiState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let ticker = canon_ticker(&st, &raw)?;
    let path = format!("/api/research/{ticker}/structure/draft");
    idempotent(&st, &headers, "POST", &path, &body, |_| {
        let file = draft_path(&st, &ticker);
        let meta = std::fs::metadata(&file).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    format!(
                        "No draft for {ticker}: {ticker}.draft.json is not in the structure folder"
                    ),
                )
            } else {
                store_error(e)
            }
        })?;
        if meta.len() > MAX_DRAFT_BYTES {
            return Err(ApiError::invalid(
                &["file"],
                "too_long",
                format!("The draft is larger than {} KiB", MAX_DRAFT_BYTES / 1024),
                json!(meta.len()),
            ));
        }
        let text = std::fs::read_to_string(&file).map_err(store_error)?;
        let ctx = context(&st, &ticker)?;
        let draft = st_rules::parse_draft(&text, &ticker, &ctx.ids()).map_err(invalid)?;
        let existing = load(&st, &ticker)?.map(|m| m.nodes).unwrap_or_default();
        let kept = st_rules::kept_for_import(&existing);
        let replaced = existing.len() - kept.len();
        let imported = draft.len();
        let merged = st_rules::merge_draft(kept, draft);
        // The draft's own metrics were checked against the pack by `parse_draft`; a kept node
        // may still link a metric of an earlier pack, as on every other write.
        let problems =
            st_rules::map_violations(&merged, &allowed_metrics(&ctx, &existing), &["file"]);
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        let map = save(&st, &ticker, &merged)?;
        let mut res = payload(&st, &ticker, Some(&map))?;
        res["imported"] = json!(imported);
        res["replaced"] = json!(replaced);
        Ok((StatusCode::OK, res))
    })
}
