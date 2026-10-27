//! Operational structure map of a company (Riset redesign G1, `docs/research/ui-slicing.md`
//! §7 and §8 D3/D4): how it makes money as a tree of nodes, drafted outside the app by Claude
//! Code from an uploaded document and accepted branch by branch by the owner. This module
//! holds the rules (limits, kinds, validation, draft file parsing, merge on import); the
//! store keeps the document and `api/r9.rs` serves it. No model and no network here.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde_json::{json, Map, Value};

use super::store::{NodeSource, StructureNode};

pub const MAX_DEPTH: usize = 4;
pub const MAX_NODES: usize = 40;
pub const MAX_LABEL_CHARS: usize = 80;
pub const MAX_NOTE_CHARS: usize = 200;
pub const MAX_SNIPPET_CHARS: usize = 160;
pub const MAX_DOCUMENT_CHARS: usize = 200;
pub const MAX_ID_CHARS: usize = 64;
/// A draft file larger than this is refused unread.
pub const MAX_DRAFT_BYTES: u64 = 256 * 1024;

/// `(id, Indonesian label)`.
pub const KINDS: [(&str, &str); 7] = [
    ("segmen", "Segmen"),
    ("produk", "Produk"),
    ("pelanggan", "Pelanggan"),
    ("pendorong_harga", "Pendorong harga"),
    ("biaya", "Biaya"),
    ("kepemilikan", "Kepemilikan"),
    ("lainnya", "Lainnya"),
];

pub const PROPOSED: &str = "proposed";
pub const ACCEPTED: &str = "accepted";
pub const REJECTED: &str = "rejected";

pub fn kind_label(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l)
}

pub fn state_label(state: &str) -> &'static str {
    match state {
        PROPOSED => "usulan",
        ACCEPTED => "diterima",
        REJECTED => "ditolak",
        _ => "",
    }
}

/// One rule the document or a node breaks; rendered as a 422 entry naming the field.
#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    pub loc: Vec<Value>,
    pub kind: &'static str,
    pub msg: String,
    pub input: Value,
}

impl Violation {
    fn new(
        loc: &[Value],
        field: &str,
        kind: &'static str,
        msg: impl Into<String>,
        input: Value,
    ) -> Self {
        let mut loc = loc.to_vec();
        if !field.is_empty() {
            loc.push(json!(field));
        }
        Self {
            loc,
            kind,
            msg: msg.into(),
            input,
        }
    }

    pub fn entry(&self) -> Value {
        json!({"type": self.kind, "loc": self.loc, "msg": self.msg, "input": self.input})
    }
}

fn loc_of(root: &[&str]) -> Vec<Value> {
    root.iter().map(|s| json!(s)).collect()
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && chars(id) <= MAX_ID_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '~'))
}

/// The rules a node obeys on its own: field shapes, lengths, kind, metric, state and source.
pub fn node_violations(
    n: &StructureNode,
    metrics: &BTreeSet<String>,
    loc: &[Value],
) -> Vec<Violation> {
    let mut v = Vec::new();
    if !id_ok(&n.id) {
        v.push(Violation::new(
            loc,
            "id",
            "string_pattern_mismatch",
            "Id must be 1 to 64 letters, digits, `-`, `_` or `~`",
            json!(n.id),
        ));
    }
    let label = n.label.trim();
    if label.is_empty() || chars(label) > MAX_LABEL_CHARS {
        v.push(Violation::new(
            loc,
            "label",
            "string_length",
            format!("Label must be 1 to {MAX_LABEL_CHARS} characters"),
            json!(n.label),
        ));
    }
    if kind_label(&n.kind).is_none() {
        v.push(Violation::new(
            loc,
            "kind",
            "enum",
            format!(
                "Input should be one of {}",
                KINDS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
            ),
            json!(n.kind),
        ));
    }
    if let Some(m) = &n.metric {
        if !metrics.contains(m) {
            v.push(Violation::new(
                loc,
                "metric",
                "unknown_metric",
                format!("`{m}` is not a metric of this ticker's pack or core"),
                json!(m),
            ));
        }
    }
    if let Some(note) = &n.note {
        if chars(note) > MAX_NOTE_CHARS {
            v.push(Violation::new(
                loc,
                "note",
                "string_length",
                format!("Note must be at most {MAX_NOTE_CHARS} characters"),
                json!(note),
            ));
        }
    }
    if ![PROPOSED, ACCEPTED, REJECTED].contains(&n.state.as_str()) {
        v.push(Violation::new(
            loc,
            "state",
            "enum",
            "Input should be 'proposed', 'accepted' or 'rejected'",
            json!(n.state),
        ));
    }
    match &n.source {
        None if n.state == PROPOSED => v.push(Violation::new(
            loc,
            "source",
            "missing",
            "A proposed node needs a source (document, page, snippet)",
            Value::Null,
        )),
        None => {}
        Some(s) => {
            let mut at = loc.to_vec();
            at.push(json!("source"));
            if s.document.trim().is_empty() || chars(&s.document) > MAX_DOCUMENT_CHARS {
                v.push(Violation::new(
                    &at,
                    "document",
                    "string_length",
                    format!("Document must be 1 to {MAX_DOCUMENT_CHARS} characters"),
                    json!(s.document),
                ));
            }
            if s.page < 1 {
                v.push(Violation::new(
                    &at,
                    "page",
                    "greater_than_equal",
                    "Page must be 1 or more",
                    json!(s.page),
                ));
            }
            if s.snippet.trim().is_empty() || chars(&s.snippet) > MAX_SNIPPET_CHARS {
                v.push(Violation::new(
                    &at,
                    "snippet",
                    "string_length",
                    format!("Snippet must be 1 to {MAX_SNIPPET_CHARS} characters"),
                    json!(s.snippet),
                ));
            }
        }
    }
    v
}

/// The rules of the document as a whole: node count, unique ids, parents that exist, no
/// cycle, depth, plus every node's own rules. `root` is the loc prefix (`["body"]`).
pub fn map_violations(
    nodes: &[StructureNode],
    metrics: &BTreeSet<String>,
    root: &[&str],
) -> Vec<Violation> {
    let mut v = Vec::new();
    let base = loc_of(root);
    let mut nodes_loc = base.clone();
    nodes_loc.push(json!("nodes"));
    // Rejected nodes stay stored but never count toward the cap, so a full draft with a few
    // rejections still leaves room for the owner's own branches.
    let live = nodes.iter().filter(|n| n.state != REJECTED).count();
    if live > MAX_NODES {
        v.push(Violation::new(
            &base,
            "nodes",
            "too_long",
            format!("At most {MAX_NODES} nodes (this map would have {live})"),
            json!(live),
        ));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for (i, n) in nodes.iter().enumerate() {
        let mut at = nodes_loc.clone();
        at.push(json!(i));
        if !seen.insert(n.id.as_str()) {
            v.push(Violation::new(
                &at,
                "id",
                "duplicate",
                format!("Id `{}` is used twice", n.id),
                json!(n.id),
            ));
        }
        v.extend(node_violations(n, metrics, &at));
    }
    let by_id: HashMap<&str, &StructureNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for (i, n) in nodes.iter().enumerate() {
        let mut at = nodes_loc.clone();
        at.push(json!(i));
        let Some(parent) = &n.parent else { continue };
        if !by_id.contains_key(parent.as_str()) || *parent == n.id {
            v.push(Violation::new(
                &at,
                "parent",
                "parent_missing",
                format!("Parent `{parent}` is not a node of this map"),
                json!(parent),
            ));
            continue;
        }
        // Walk up from the parent: a root child has depth 1.
        let mut depth = 2;
        let mut cur = parent.as_str();
        let mut cycle = false;
        while let Some(next) = by_id.get(cur).and_then(|p| p.parent.as_deref()) {
            depth += 1;
            if depth > nodes.len() + 1 {
                cycle = true;
                break;
            }
            cur = next;
        }
        if cycle {
            v.push(Violation::new(
                &at,
                "parent",
                "cycle",
                "Parents form a cycle",
                json!(parent),
            ));
        } else if depth > MAX_DEPTH {
            v.push(Violation::new(
                &at,
                "parent",
                "too_deep",
                format!("At most {MAX_DEPTH} levels deep"),
                json!(parent),
            ));
        }
    }
    v
}

/// Lifts every node that is not rejected over rejected ancestors (to the nearest
/// non-rejected ancestor, or the root), so a rejected branch never hides a kept node.
pub fn heal(nodes: &mut [StructureNode]) {
    let parent_of: HashMap<String, Option<String>> = nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent.clone()))
        .collect();
    let rejected: HashSet<String> = nodes
        .iter()
        .filter(|n| n.state == REJECTED)
        .map(|n| n.id.clone())
        .collect();
    for n in nodes.iter_mut().filter(|n| n.state != REJECTED) {
        let mut p = n.parent.clone();
        let mut hops = 0;
        while let Some(id) = &p {
            if !rejected.contains(id) || hops > parent_of.len() {
                break;
            }
            p = parent_of.get(id).cloned().flatten();
            hops += 1;
        }
        n.parent = p;
    }
}

/// `id` and its descendants.
pub fn subtree(nodes: &[StructureNode], id: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    out.insert(id.to_string());
    loop {
        let before = out.len();
        for n in nodes {
            if n.parent.as_ref().is_some_and(|p| out.contains(p)) {
                out.insert(n.id.clone());
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// Sets `id` to `state`. Rejecting also rejects the branch's proposed descendants; nodes the
/// owner already accepted below it are lifted by `heal`.
pub fn set_state(nodes: &mut [StructureNode], id: &str, state: &str) {
    let branch = subtree(nodes, id);
    for n in nodes.iter_mut() {
        if n.id == id {
            n.state = state.to_string();
        } else if state == REJECTED && branch.contains(&n.id) && n.state == PROPOSED {
            n.state = REJECTED.to_string();
        }
    }
    heal(nodes);
}

/// The nodes a draft import keeps: the accepted ones (owner-added nodes among them). Every
/// other node of the old map goes, and the kept ones below a removed node are lifted.
pub fn kept_for_import(existing: &[StructureNode]) -> Vec<StructureNode> {
    let mut all: Vec<StructureNode> = existing
        .iter()
        .map(|n| {
            let mut n = n.clone();
            if n.state != ACCEPTED {
                n.state = REJECTED.into();
            }
            n
        })
        .collect();
    heal(&mut all);
    all.into_iter().filter(|n| n.state == ACCEPTED).collect()
}

/// Appends the draft nodes to the kept ones. A draft id that a kept node already uses gets a
/// `~2`, `~3` suffix (its draft children follow it).
pub fn merge_draft(kept: Vec<StructureNode>, draft: Vec<StructureNode>) -> Vec<StructureNode> {
    let mut used: HashSet<String> = kept.iter().map(|n| n.id.clone()).collect();
    let draft_ids: HashSet<String> = draft.iter().map(|n| n.id.clone()).collect();
    let mut rename: HashMap<String, String> = HashMap::new();
    for n in &draft {
        let mut id = n.id.clone();
        let mut k = 2;
        // A renamed id must not meet another draft id either.
        while used.contains(&id) || (id != n.id && draft_ids.contains(&id)) {
            id = format!("{}~{k}", n.id);
            k += 1;
        }
        used.insert(id.clone());
        rename.insert(n.id.clone(), id);
    }
    let mut out = kept;
    for mut n in draft {
        n.parent = n.parent.as_ref().and_then(|p| rename.get(p).cloned());
        n.id = rename[&n.id].clone();
        out.push(n);
    }
    out
}

// ---- the draft file ----

const NODE_KEYS: [&str; 8] = [
    "id", "parent", "label", "kind", "metric", "note", "source", "state",
];

fn str_field(
    obj: &Map<String, Value>,
    key: &str,
    loc: &[Value],
    required: bool,
    v: &mut Vec<Violation>,
) -> Option<String> {
    match obj.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) | None if !required => None,
        None => {
            v.push(Violation::new(
                loc,
                key,
                "missing",
                "Field required",
                Value::Null,
            ));
            None
        }
        Some(other) => {
            v.push(Violation::new(
                loc,
                key,
                "string_type",
                "Input should be a string",
                other.clone(),
            ));
            None
        }
    }
}

fn extra_keys(obj: &Map<String, Value>, allowed: &[&str], loc: &[Value], v: &mut Vec<Violation>) {
    for k in obj.keys().filter(|k| !allowed.contains(&k.as_str())) {
        v.push(Violation::new(
            loc,
            k,
            "extra_forbidden",
            "Extra inputs are not permitted",
            Value::Null,
        ));
    }
}

fn parse_source(
    s: &Map<String, Value>,
    loc: &[Value],
    v: &mut Vec<Violation>,
) -> Option<NodeSource> {
    extra_keys(s, &["document", "page", "snippet"], loc, v);
    let document = str_field(s, "document", loc, true, v);
    let snippet = str_field(s, "snippet", loc, true, v);
    let page = match s.get("page") {
        Some(p) => {
            let page = p.as_u64().and_then(|p| u32::try_from(p).ok());
            if page.is_none() {
                v.push(Violation::new(
                    loc,
                    "page",
                    "int_parsing",
                    "Page must be a whole number, 1 or more",
                    p.clone(),
                ));
            }
            page
        }
        None => {
            v.push(Violation::new(
                loc,
                "page",
                "missing",
                "Field required",
                Value::Null,
            ));
            None
        }
    };
    Some(NodeSource {
        document: document?,
        page: page?,
        snippet: snippet?,
    })
}

fn parse_node(item: &Value, loc: &[Value], v: &mut Vec<Violation>) -> Option<StructureNode> {
    let Some(obj) = item.as_object() else {
        v.push(Violation::new(
            loc,
            "",
            "model_type",
            "A node must be a JSON object",
            item.clone(),
        ));
        return None;
    };
    extra_keys(obj, &NODE_KEYS, loc, v);
    if let Some(s) = obj.get("state") {
        if s != &json!(PROPOSED) {
            v.push(Violation::new(
                loc,
                "state",
                "enum",
                "A draft node can only be 'proposed'",
                s.clone(),
            ));
        }
    }
    let id = str_field(obj, "id", loc, true, v);
    let label = str_field(obj, "label", loc, true, v);
    let kind = str_field(obj, "kind", loc, true, v);
    let parent = str_field(obj, "parent", loc, false, v);
    let metric = str_field(obj, "metric", loc, false, v);
    let note = str_field(obj, "note", loc, false, v);
    let mut sloc = loc.to_vec();
    sloc.push(json!("source"));
    let source = match obj.get("source") {
        Some(Value::Object(s)) => parse_source(s, &sloc, v),
        Some(Value::Null) | None => {
            v.push(Violation::new(
                loc,
                "source",
                "missing",
                "A draft node needs a source (document, page, snippet)",
                Value::Null,
            ));
            None
        }
        Some(other) => {
            v.push(Violation::new(
                loc,
                "source",
                "model_type",
                "source must be an object",
                other.clone(),
            ));
            None
        }
    };
    Some(StructureNode {
        id: id?,
        parent,
        label: label?.trim().to_string(),
        kind: kind?,
        metric,
        note,
        state: PROPOSED.into(),
        owner: false,
        source: Some(source?),
    })
}

/// Parses `<TICKER>.draft.json`: `{"version": 1, "ticker": "BUMI", "nodes": [...]}` where each
/// node is `{id, parent, label, kind, metric?, note?, source: {document, page, snippet}}`.
/// The whole file is refused with a field error for every violation; nothing is imported
/// partly. Nodes come out `proposed`, not owner-added.
pub fn parse_draft(
    text: &str,
    ticker: &str,
    metrics: &BTreeSet<String>,
) -> Result<Vec<StructureNode>, Vec<Violation>> {
    let root = vec![json!("file")];
    let value: Value = serde_json::from_str(text).map_err(|e| {
        vec![Violation::new(
            &root,
            "",
            "json_invalid",
            format!("JSON decode error: {e}"),
            Value::Null,
        )]
    })?;
    let Some(top) = value.as_object() else {
        return Err(vec![Violation::new(
            &root,
            "",
            "model_type",
            "The draft must be a JSON object",
            Value::Null,
        )]);
    };
    let mut v: Vec<Violation> = Vec::new();
    extra_keys(top, &["version", "ticker", "nodes"], &root, &mut v);
    if let Some(ver) = top.get("version") {
        if ver != &json!(1) {
            v.push(Violation::new(
                &root,
                "version",
                "enum",
                "Version must be 1",
                ver.clone(),
            ));
        }
    }
    match top.get("ticker").and_then(Value::as_str) {
        Some(t) if t.trim().eq_ignore_ascii_case(ticker) => {}
        other => v.push(Violation::new(
            &root,
            "ticker",
            "ticker_mismatch",
            format!("The draft must name ticker {ticker}"),
            other.map_or(Value::Null, |t| json!(t)),
        )),
    }
    let Some(items) = top.get("nodes").and_then(Value::as_array) else {
        v.push(Violation::new(
            &root,
            "nodes",
            "list_type",
            "nodes must be a list",
            Value::Null,
        ));
        return Err(v);
    };
    let mut nodes_loc = root.clone();
    nodes_loc.push(json!("nodes"));
    let mut nodes = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let mut loc = nodes_loc.clone();
        loc.push(json!(i));
        nodes.extend(parse_node(item, &loc, &mut v));
    }
    // Document rules only make sense on a fully read file.
    if v.is_empty() {
        v.extend(map_violations(&nodes, metrics, &["file"]));
    }
    if v.is_empty() {
        Ok(nodes)
    } else {
        Err(v)
    }
}
