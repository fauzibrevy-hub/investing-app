//! Structure map (Riset redesign G1) and the driver-to-metric link (G2): migration 011, the
//! limits and field errors, each route's happy path and 4xx, the draft import (validated as
//! a whole, proposed nodes replaced, accepted ones kept), replay from the log, and
//! `compare_metric` validation at pack load. Every ticker, label and page is invented.

mod common;
#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use finance_app::research::packs::def::Pack;
use finance_app::research::store::{latest_version, Store, LOG_FILE};
use r4_env::{call, env, Env};
use serde_json::{json, Value};

const UNPROCESSABLE: StatusCode = StatusCode::UNPROCESSABLE_ENTITY;

fn base(t: &str) -> String {
    format!("/api/research/{t}/structure")
}

async fn add(e: &Env, t: &str, key: &str, body: Value) -> (StatusCode, Value) {
    e.post(&format!("{}/nodes", base(t)), key, body).await
}

async fn add_ok(e: &Env, t: &str, key: &str, body: Value) -> String {
    let (s, v) = add(e, t, key, body).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v["node"]["id"].as_str().unwrap().to_string()
}

async fn nodes(e: &Env, t: &str) -> Vec<Value> {
    let (s, v) = e.get(&base(t)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["nodes"].as_array().unwrap().clone()
}

fn find<'a>(ns: &'a [Value], label: &str) -> &'a Value {
    ns.iter().find(|n| n["label"] == label).unwrap()
}

fn path_of(d: &Value) -> String {
    d["loc"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.to_string().trim_matches('"').to_string())
        .collect::<Vec<_>>()
        .join(".")
}

/// First violation's location, as a dotted path.
fn loc(v: &Value) -> String {
    path_of(&v["detail"][0])
}

fn put_draft(e: &Env, ticker: &str, text: &str) {
    let dir = e.dir.path().join("research/structure");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{ticker}.draft.json")), text).unwrap();
}

fn node(id: &str, parent: Option<&str>, label: &str) -> Value {
    json!({
        "id": id, "parent": parent, "label": label, "kind": "segmen",
        "source": {"document": "ar-synthetic.pdf", "page": 12, "snippet": "Cuplikan sintetis."},
    })
}

fn draft_of(ticker: &str, ns: Vec<Value>) -> String {
    json!({"version": 1, "ticker": ticker, "nodes": ns}).to_string()
}

async fn import(e: &Env, t: &str, key: &str) -> (StatusCode, Value) {
    e.post(&format!("{}/draft", base(t)), key, json!({})).await
}

// ---- migration ----

#[test]
fn migration_011_creates_the_table_and_upgrades_a_v10_store() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    let old = Store::open_at_version(&db, 10).unwrap();
    assert_eq!(old.user_version().unwrap(), 10);
    old.with_conn(|c| {
        assert!(c.prepare("SELECT doc FROM structure_maps").is_err());
        Ok(())
    })
    .unwrap();
    drop(old);
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    assert_eq!(latest_version(), 12);
    assert!(store.structure("COAL").unwrap().is_none());
}

// ---- reads ----

#[tokio::test]
async fn empty_map_lists_limits_kinds_and_the_metrics_a_node_may_link() {
    let e = env();
    let (s, v) = e.get(&base("COAL")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["nodes"], json!([]));
    assert_eq!(
        v["counts"],
        json!({"proposed": 0, "accepted": 0, "rejected": 0})
    );
    assert_eq!(v["limits"]["max_nodes"], 40);
    assert_eq!(v["limits"]["max_depth"], 4);
    assert_eq!(v["limits"]["max_label"], 80);
    assert_eq!(v["pack"]["id"], "coal_mining");
    assert_eq!(
        v["draft"],
        json!({"file": "COAL.draft.json", "available": false})
    );
    let kinds: Vec<&str> = v["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "segmen",
            "produk",
            "pelanggan",
            "pendorong_harga",
            "biaya",
            "kepemilikan",
            "lainnya"
        ]
    );
    let ids: Vec<&str> = v["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"asp_usd_t"), "pack metric listed");
    assert!(
        !ids.contains(&"financing_yield"),
        "another pack's metric is not"
    );
    // A ticker without a pack still links to core metrics.
    let (_, none) = e.get(&base("NONE")).await;
    assert!(none["pack"].is_null());
    assert!(!none["metrics"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_ticker_is_404() {
    let e = env();
    let (s, _) = e.get(&base("ZZZZ")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = add(&e, "ZZZZ", "k1", json!({"label": "x"})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ---- owner nodes ----

#[tokio::test]
async fn owner_node_is_accepted_at_once_nested_logged_and_replayed() {
    let e = env();
    let root = add_ok(
        &e,
        "COAL",
        "k1",
        json!({"label": "Tambang utama", "kind": "segmen"}),
    )
    .await;
    assert!(root.starts_with("o-"));
    let (s, v) = add(
        &e,
        "COAL",
        "k2",
        json!({"label": "Batubara ekspor", "kind": "produk", "parent": root,
               "metric": "asp_usd_t", "note": "Dijual ke pembeli sintetis"}),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let child = &v["node"];
    assert_eq!(child["owner"], true);
    assert_eq!(child["state"], "accepted");
    assert_eq!(child["state_label"], "diterima");
    assert_eq!(child["kind_label"], "Produk");
    assert!(child["metric_label"].as_str().is_some());
    assert!(child["source"].is_null());
    assert_eq!(v["counts"]["accepted"], 2);
    // The kind defaults to `lainnya`.
    let id3 = add_ok(&e, "COAL", "k3", json!({"label": "Lain-lain"})).await;
    assert!(id3.starts_with("o-"));
    let ns = nodes(&e, "COAL").await;
    assert_eq!(ns.len(), 3);
    assert_eq!(find(&ns, "Lain-lain")["kind"], "lainnya");
    assert_eq!(find(&ns, "Batubara ekspor")["parent"], json!(root));
    // One log line per write; a rebuild from the log restores the last document.
    let log = std::fs::read_to_string(e.dir.path().join(LOG_FILE)).unwrap();
    assert_eq!(
        log.lines()
            .filter(|l| l.contains("\"op\":\"structure_set\""))
            .count(),
        3
    );
    e.store.rebuild().unwrap();
    assert_eq!(nodes(&e, "COAL").await, ns);
}

#[tokio::test]
async fn a_retry_with_the_same_key_adds_one_node_and_a_missing_key_is_422() {
    let e = env();
    let body = json!({"label": "Sekali saja"});
    let uri = format!("{}/nodes", base("COAL"));
    let (s1, v1, _) = call(&e.app, Method::POST, &uri, Some("same"), Some(body.clone())).await;
    let (s2, v2, h2) = call(&e.app, Method::POST, &uri, Some("same"), Some(body.clone())).await;
    assert_eq!((s1, s2), (StatusCode::CREATED, StatusCode::CREATED));
    assert_eq!(v1["node"]["id"], v2["node"]["id"]);
    assert_eq!(h2.get("idempotent-replay").unwrap(), "true");
    assert_eq!(nodes(&e, "COAL").await.len(), 1);
    let (s, v, _) = call(&e.app, Method::POST, &uri, None, Some(body)).await;
    assert_eq!(s, UNPROCESSABLE);
    assert_eq!(loc(&v), "header.idempotency-key");
}

#[tokio::test]
async fn owner_node_rules_label_kind_metric_note_parent_depth_and_count() {
    let e = env();
    let long = "x".repeat(81);
    let (s, v) = add(&e, "COAL", "a1", json!({"label": long})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.label"));
    // 80 characters is allowed, and counted in characters (not bytes).
    let (s, _) = add(&e, "COAL", "a2", json!({"label": "é".repeat(80)})).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, v) = add(&e, "COAL", "a3", json!({"label": "  "})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.label"));
    let (s, v) = add(&e, "COAL", "a4", json!({"label": "k", "kind": "tesis"})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.kind"));
    let (s, v) = add(&e, "COAL", "a5", json!({"label": "k", "metric": "nope"})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.metric"));
    assert_eq!(v["detail"][0]["type"], "unknown_metric");
    // A metric of another pack is unknown for this ticker.
    let other = json!({"label": "k", "metric": "financing_yield"});
    let (s, v) = add(&e, "COAL", "a6", other).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.metric"));
    let (s, v) = add(
        &e,
        "COAL",
        "a7",
        json!({"label": "k", "note": "n".repeat(201)}),
    )
    .await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.note"));
    let (s, v) = add(&e, "COAL", "a8", json!({"label": "k", "parent": "o-none"})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.parent"));
    let (s, v) = add(&e, "COAL", "a9", json!({"label": "k", "colour": "red"})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.colour"));

    // Depth 4 is the limit: a chain of four is fine, a fifth level is refused.
    let mut parent: Option<String> = None;
    for level in 1..=4 {
        let mut body = json!({"label": format!("Tingkat {level}")});
        if let Some(p) = &parent {
            body["parent"] = json!(p);
        }
        parent = Some(add_ok(&e, "COAL", &format!("d{level}"), body).await);
    }
    let (s, v) = add(
        &e,
        "COAL",
        "d5",
        json!({"label": "Tingkat 5", "parent": parent}),
    )
    .await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_deep");

    // 40 nodes is the limit (5 so far: the é node and the chain of four).
    for i in 0..35 {
        let body = json!({"label": format!("Cabang {i}")});
        add_ok(&e, "COAL", &format!("c{i}"), body).await;
    }
    assert_eq!(nodes(&e, "COAL").await.len(), 40);
    let (s, v) = add(&e, "COAL", "c40", json!({"label": "Terlalu banyak"})).await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");
    assert_eq!(
        nodes(&e, "COAL").await.len(),
        40,
        "nothing written on a 422"
    );
}

#[tokio::test]
async fn owner_node_can_be_deleted_only_when_leaf_and_owned() {
    let e = env();
    let a = add_ok(&e, "COAL", "k1", json!({"label": "Induk"})).await;
    let b = add_ok(&e, "COAL", "k2", json!({"label": "Anak", "parent": a})).await;
    let uri = |id: &str| format!("{}/nodes/{id}", base("COAL"));
    let (s, v) = e.send(Method::DELETE, &uri(&a), "x1", Value::Null).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    let (s, v) = e.send(Method::DELETE, &uri(&b), "x2", Value::Null).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["nodes"].as_array().unwrap().len(), 1);
    let (s, _) = e.send(Method::DELETE, &uri(&b), "x3", Value::Null).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e.send(Method::DELETE, &uri(&a), "x4", Value::Null).await;
    assert_eq!(s, StatusCode::OK);
    assert!(nodes(&e, "COAL").await.is_empty());
    // Only rejected branches below (hidden on the page): the node goes, and they with it.
    let a = add_ok(&e, "COAL", "k3", json!({"label": "Induk"})).await;
    let b = add_ok(&e, "COAL", "k4", json!({"label": "Anak", "parent": a})).await;
    let reject = json!({"state": "rejected"});
    let (s, v) = e.send(Method::PUT, &uri(&b), "r1", reject).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = e.send(Method::DELETE, &uri(&a), "x6", Value::Null).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(nodes(&e, "COAL").await.is_empty());
    // A drafted node is rejected, never deleted.
    put_draft(
        &e,
        "COAL",
        &draft_of("COAL", vec![node("d1", None, "Usulan")]),
    );
    assert_eq!(import(&e, "COAL", "i1").await.0, StatusCode::OK);
    let (s, v) = e.send(Method::DELETE, &uri("d1"), "x5", Value::Null).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
}

// ---- edit, accept, reject ----

#[tokio::test]
async fn node_edit_accept_and_reject() {
    let e = env();
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![
                node("n1", None, "Tambang"),
                node("n2", Some("n1"), "Ekspor"),
                node("n3", Some("n2"), "Pembeli"),
            ],
        ),
    );
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["imported"], 3);
    assert_eq!(v["counts"]["proposed"], 3);
    let uri = |id: &str| format!("{}/nodes/{id}", base("COAL"));

    // Edit and accept in one call; a proposed node keeps its source.
    let (s, v) = e
        .send(
            Method::PUT,
            &uri("n1"),
            "p1",
            json!({"label": "Tambang batubara", "kind": "segmen", "metric": "asp_usd_t",
                   "note": "Catatan", "state": "accepted"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let n1 = find(v["nodes"].as_array().unwrap(), "Tambang batubara");
    assert_eq!(n1["state"], "accepted");
    assert_eq!(n1["metric"], "asp_usd_t");
    assert_eq!(n1["note"], "Catatan");
    assert_eq!(n1["source"]["page"], 12);
    assert_eq!(n1["owner"], false);

    // `null` clears a link and a note; absent fields stay.
    let clear = json!({"metric": null, "note": ""});
    let (s, v) = e.send(Method::PUT, &uri("n1"), "p2", clear).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let n1 = find(v["nodes"].as_array().unwrap(), "Tambang batubara");
    assert!(n1["metric"].is_null() && n1["note"].is_null());
    assert_eq!(n1["state"], "accepted");

    // Accept the leaf below a node that is then rejected: the leaf is lifted, the proposed
    // branch between them goes down with the rejection.
    let accept = json!({"state": "accepted"});
    let (s, _) = e.send(Method::PUT, &uri("n3"), "p3", accept).await;
    assert_eq!(s, StatusCode::OK);
    let reject = json!({"state": "rejected"});
    let (s, v) = e.send(Method::PUT, &uri("n2"), "p4", reject).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let ns = v["nodes"].as_array().unwrap();
    assert_eq!(find(ns, "Ekspor")["state"], "rejected");
    assert_eq!(find(ns, "Ekspor")["state_label"], "ditolak");
    assert_eq!(find(ns, "Pembeli")["state"], "accepted");
    assert_eq!(
        find(ns, "Pembeli")["parent"],
        json!("n1"),
        "lifted over the rejected node"
    );
    assert_eq!(
        v["counts"],
        json!({"proposed": 0, "accepted": 2, "rejected": 1})
    );

    // The decision survives a rebuild from the log.
    e.store.rebuild().unwrap();
    assert_eq!(nodes(&e, "COAL").await, ns.clone());
}

#[tokio::test]
async fn node_put_errors() {
    let e = env();
    let a = add_ok(&e, "COAL", "k1", json!({"label": "Satu"})).await;
    let uri = |id: &str| format!("{}/nodes/{id}", base("COAL"));
    let (s, _) = e
        .send(Method::PUT, &uri("nope"), "p1", json!({"label": "x"}))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v) = e.send(Method::PUT, &uri(&a), "p2", json!({})).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body"));
    let proposed = json!({"state": "proposed"});
    let (s, v) = e.send(Method::PUT, &uri(&a), "p3", proposed).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.state"));
    let long = json!({"label": "y".repeat(81)});
    let (s, v) = e.send(Method::PUT, &uri(&a), "p4", long).await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.label"));
    let (s, v) = e
        .send(Method::PUT, &uri(&a), "p5", json!({"metric": "nope"}))
        .await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.metric"));
    let (s, v) = e
        .send(Method::PUT, &uri(&a), "p6", json!({"parent": "x"}))
        .await;
    assert_eq!((s, loc(&v).as_str()), (UNPROCESSABLE, "body.parent"));
    assert_eq!(
        nodes(&e, "COAL").await[0]["label"],
        "Satu",
        "no 422 wrote anything"
    );
}

// ---- draft import ----

#[tokio::test]
async fn draft_import_loads_proposed_nodes_with_their_sources() {
    let e = env();
    let (s, v) = import(&e, "COAL", "i0").await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    assert!(v["detail"].as_str().unwrap().contains("COAL"));

    let mut kpc = node("kpc", None, "KPC");
    kpc["metric"] = json!("asp_usd_t");
    kpc["note"] = json!("Anak usaha sintetis");
    put_draft(
        &e,
        "COAL",
        &draft_of("COAL", vec![kpc, node("exp", Some("kpc"), "Ekspor")]),
    );
    let (_, before) = e.get(&base("COAL")).await;
    assert_eq!(before["draft"]["available"], true);
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["imported"].as_u64(), v["replaced"].as_u64()),
        (Some(2), Some(0))
    );
    let ns = v["nodes"].as_array().unwrap();
    let kpc = find(ns, "KPC");
    assert_eq!(kpc["state"], "proposed");
    assert_eq!(kpc["state_label"], "usulan");
    assert_eq!(kpc["owner"], false);
    assert_eq!(kpc["source"]["document"], "ar-synthetic.pdf");
    assert_eq!(kpc["source"]["snippet"], "Cuplikan sintetis.");
    assert_eq!(find(ns, "Ekspor")["parent"], "kpc");
}

#[tokio::test]
async fn importing_again_replaces_proposed_and_keeps_accepted_and_owner_nodes() {
    let e = env();
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![
                node("a", None, "Diterima"),
                node("b", None, "Diusulkan"),
                node("c", None, "Ditolak"),
            ],
        ),
    );
    assert_eq!(import(&e, "COAL", "i1").await.0, StatusCode::OK);
    let uri = |id: &str| format!("{}/nodes/{id}", base("COAL"));
    let accept = json!({"state": "accepted"});
    e.send(Method::PUT, &uri("a"), "p1", accept).await;
    let reject = json!({"state": "rejected"});
    e.send(Method::PUT, &uri("c"), "p2", reject).await;
    add_ok(&e, "COAL", "o1", json!({"label": "Milik saya"})).await;

    // The new draft reuses id `a` and brings `d`; its `d` hangs under its own `a`.
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![
                node("a", None, "Usulan baru"),
                node("d", Some("a"), "Cabang baru"),
            ],
        ),
    );
    let (s, v) = import(&e, "COAL", "i2").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["imported"], 2);
    assert_eq!(v["replaced"], 2, "the proposed and the rejected node went");
    let ns = v["nodes"].as_array().unwrap();
    assert_eq!(ns.len(), 4);
    assert_eq!(find(ns, "Diterima")["state"], "accepted");
    assert_eq!(find(ns, "Diterima")["id"], "a");
    assert_eq!(find(ns, "Milik saya")["owner"], true);
    assert!(ns
        .iter()
        .all(|n| n["label"] != "Diusulkan" && n["label"] != "Ditolak"));
    let new_a = find(ns, "Usulan baru");
    assert_eq!(new_a["id"], "a~2", "an id a kept node uses is renamed");
    assert_eq!(new_a["state"], "proposed");
    assert_eq!(find(ns, "Cabang baru")["parent"], "a~2");
}

#[tokio::test]
async fn an_accepted_node_below_a_replaced_one_is_lifted() {
    let e = env();
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![node("p", None, "Induk"), node("q", Some("p"), "Anak")],
        ),
    );
    import(&e, "COAL", "i1").await;
    let uri = format!("{}/nodes/q", base("COAL"));
    let accept = json!({"state": "accepted"});
    e.send(Method::PUT, &uri, "p1", accept).await;
    put_draft(&e, "COAL", &draft_of("COAL", vec![node("z", None, "Lain")]));
    let (s, v) = import(&e, "COAL", "i2").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let ns = v["nodes"].as_array().unwrap();
    assert!(find(ns, "Anak")["parent"].is_null());
}

#[tokio::test]
async fn an_invalid_draft_is_refused_whole_with_field_errors_and_writes_nothing() {
    let e = env();
    add_ok(&e, "COAL", "o1", json!({"label": "Tetap"})).await;
    let before = nodes(&e, "COAL").await;
    let with = |key: &str, value: Value| {
        let mut n = node("a", None, "A");
        n[key] = value;
        draft_of("COAL", vec![n])
    };
    let source = |page: u64, snippet: &str| {
        draft_of(
            "COAL",
            vec![json!({"id": "a", "label": "A", "kind": "segmen",
                        "source": {"document": "d.pdf", "page": page, "snippet": snippet}})],
        )
    };
    let chain = |n: usize| {
        draft_of(
            "COAL",
            (0..n)
                .map(|i| {
                    let parent = (i > 0).then(|| format!("n{}", i - 1));
                    node(&format!("n{i}"), parent.as_deref(), &format!("N{i}"))
                })
                .collect(),
        )
    };
    let cases: Vec<(&str, String, &str, &str)> = vec![
        ("not json", "{".into(), "file", "json_invalid"),
        (
            "wrong ticker",
            draft_of("OTHR", vec![node("a", None, "A")]),
            "file.ticker",
            "ticker_mismatch",
        ),
        (
            "no source",
            draft_of(
                "COAL",
                vec![json!({"id": "a", "parent": null, "label": "A", "kind": "segmen"})],
            ),
            "file.nodes.0.source",
            "missing",
        ),
        (
            "long snippet",
            source(1, &"s".repeat(161)),
            "file.nodes.0.source.snippet",
            "string_length",
        ),
        (
            "page zero",
            source(0, "x"),
            "file.nodes.0.source.page",
            "greater_than_equal",
        ),
        (
            "long label",
            draft_of("COAL", vec![node("a", None, &"l".repeat(81))]),
            "file.nodes.0.label",
            "string_length",
        ),
        (
            "bad kind",
            with("kind", json!("tesis")),
            "file.nodes.0.kind",
            "enum",
        ),
        (
            "unknown metric",
            with("metric", json!("nope")),
            "file.nodes.0.metric",
            "unknown_metric",
        ),
        (
            "metric of another pack",
            with("metric", json!("financing_yield")),
            "file.nodes.0.metric",
            "unknown_metric",
        ),
        (
            "unknown parent",
            draft_of("COAL", vec![node("a", Some("ghost"), "A")]),
            "file.nodes.0.parent",
            "parent_missing",
        ),
        (
            "duplicate id",
            draft_of("COAL", vec![node("a", None, "A"), node("a", None, "B")]),
            "file.nodes.1.id",
            "duplicate",
        ),
        ("too deep", chain(5), "file.nodes.4.parent", "too_deep"),
        (
            "cycle",
            draft_of(
                "COAL",
                vec![node("a", Some("b"), "A"), node("b", Some("a"), "B")],
            ),
            "file.nodes.0.parent",
            "cycle",
        ),
        (
            "too many",
            draft_of(
                "COAL",
                (0..41)
                    .map(|i| node(&format!("n{i}"), None, &format!("N{i}")))
                    .collect(),
            ),
            "file.nodes",
            "too_long",
        ),
        (
            "extra field",
            with("colour", json!("red")),
            "file.nodes.0.colour",
            "extra_forbidden",
        ),
        (
            "accepted state",
            with("state", json!("accepted")),
            "file.nodes.0.state",
            "enum",
        ),
    ];
    for (i, (name, text, want_loc, want_type)) in cases.iter().enumerate() {
        put_draft(&e, "COAL", text);
        let (s, v) = import(&e, "COAL", &format!("bad{i}")).await;
        assert_eq!(s, UNPROCESSABLE, "{name}: {v}");
        let hit = v["detail"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["type"] == *want_type && path_of(d) == *want_loc);
        assert!(hit, "{name}: expected {want_type} at {want_loc} in {v}");
        assert_eq!(nodes(&e, "COAL").await, before, "{name}: nothing imported");
    }
    // Every violation of a file is reported, not only the first.
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![
                json!({"id": "a", "label": "A", "kind": "segmen", "colour": "red",
                       "source": {"document": "d.pdf", "page": 1, "snippet": "x"}}),
                json!({"id": "b", "label": "B", "kind": "segmen"}),
            ],
        ),
    );
    let (_, v) = import(&e, "COAL", "many").await;
    assert!(v["detail"].as_array().unwrap().len() >= 2, "{v}");
}

#[tokio::test]
async fn a_draft_that_would_push_the_map_over_the_limit_is_refused() {
    let e = env();
    for i in 0..39 {
        let body = json!({"label": format!("Milik {i}")});
        add_ok(&e, "COAL", &format!("o{i}"), body).await;
    }
    put_draft(
        &e,
        "COAL",
        &draft_of(
            "COAL",
            vec![node("a", None, "Satu"), node("b", None, "Dua")],
        ),
    );
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");
    assert_eq!(nodes(&e, "COAL").await.len(), 39);
}

fn full_draft(n: usize) -> String {
    let ns = (0..n)
        .map(|i| node(&format!("n{i}"), None, &format!("Usulan {i}")))
        .collect();
    draft_of("COAL", ns)
}

#[tokio::test]
async fn a_rejected_node_frees_a_slot_for_an_owner_branch() {
    let e = env();
    put_draft(&e, "COAL", &full_draft(40));
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["imported"], 40);

    // 40 non-rejected: the owner cannot add.
    let (s, v) = add(&e, "COAL", "a1", json!({"label": "Milik"})).await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");
    assert_eq!(
        v["detail"][0]["msg"],
        "At most 40 nodes (this map would have 41)"
    );

    // Reject one proposal: 39 non-rejected, the add succeeds and the rejected node stays.
    let uri = format!("{}/nodes/n0", base("COAL"));
    let reject = json!({"state": "rejected"});
    let (s, v) = e.send(Method::PUT, &uri, "r1", reject).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = add(&e, "COAL", "a2", json!({"label": "Milik"})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(nodes(&e, "COAL").await.len(), 41);

    // Back at 40 non-rejected: refused again.
    let (s, v) = add(&e, "COAL", "a3", json!({"label": "Lagi"})).await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");

    // Accepting the rejected node again would make 41: refused, and it stays rejected.
    let accept = json!({"state": "accepted"});
    let (s, v) = e.send(Method::PUT, &uri, "r2", accept).await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");
    assert_eq!(
        find(&nodes(&e, "COAL").await, "Usulan 0")["state"],
        "rejected"
    );
}

#[tokio::test]
async fn a_kept_node_linking_a_metric_of_an_earlier_pack_does_not_block_an_import() {
    let e = env();
    let kept = finance_app::research::store::StructureNode {
        id: "o-1".into(),
        parent: None,
        label: "Milik lama".into(),
        kind: "segmen".into(),
        metric: Some("retired_metric".into()),
        note: None,
        state: "accepted".into(),
        owner: true,
        source: None,
    };
    e.store
        .set_structure("COAL", &[kept], "2026-10-01T00:00:00+00:00")
        .unwrap();
    put_draft(&e, "COAL", &draft_of("COAL", vec![node("a", None, "Satu")]));
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        find(&nodes(&e, "COAL").await, "Milik lama")["metric"],
        "retired_metric"
    );
}

#[tokio::test]
async fn a_draft_file_itself_may_hold_at_most_40_nodes() {
    let e = env();
    put_draft(&e, "COAL", &full_draft(41));
    let (s, v) = import(&e, "COAL", "i1").await;
    assert_eq!(s, UNPROCESSABLE, "{v}");
    assert_eq!(v["detail"][0]["type"], "too_long");
}

// ---- G2: compare_metric ----

#[test]
fn packs_link_each_driver_to_a_metric_of_the_same_pack() {
    let packs = finance_app::research::packs::Packs::embedded().set();
    let link = |pack: &str, driver: &str| {
        let p = packs.get(pack).unwrap();
        let d = p.drivers.iter().find(|d| d.id == driver).unwrap();
        let m = d.compare_metric.clone();
        if let Some(m) = &m {
            assert!(
                p.metric(m).is_some(),
                "{pack}.{driver} -> {m} is a metric of the pack"
            );
        }
        m
    };
    assert_eq!(
        link("coal_mining", "newcastle").as_deref(),
        Some("asp_usd_t")
    );
    assert_eq!(
        link("heavy_equipment_contracting", "newcastle").as_deref(),
        Some("coal_asp_usd_t")
    );
    assert_eq!(
        link("shipping", "charter_index").as_deref(),
        Some("day_rate_usd")
    );
    assert_eq!(
        link("bank_sharia", "bi_rate").as_deref(),
        Some("financing_yield")
    );
    assert_eq!(link("coal_mining", "usd_idr"), None);
    for d in &packs.get("it_distribution").unwrap().drivers {
        assert_eq!(d.compare_metric, None);
    }
}

#[test]
fn a_compare_metric_that_is_not_a_metric_of_the_pack_is_refused_at_load() {
    let text = std::fs::read_to_string("research/packs/coal_mining.toml").unwrap();
    assert!(Pack::parse(&text).is_ok());
    let wrong = text.replace(
        "compare_metric = \"asp_usd_t\"",
        "compare_metric = \"no_such\"",
    );
    assert_ne!(wrong, text);
    let err = Pack::parse(&wrong).unwrap_err().to_string();
    assert!(err.contains("compare_metric `no_such`"), "{err}");
    // A metric that exists in another pack is still not a metric of this one.
    let other = text.replace(
        "compare_metric = \"asp_usd_t\"",
        "compare_metric = \"financing_yield\"",
    );
    assert!(Pack::parse(&other).is_err());
}

#[tokio::test]
async fn compare_metric_reaches_the_packs_and_prices_payloads() {
    let e = env();
    let (s, v) = e.get("/api/research/packs").await;
    assert_eq!(s, StatusCode::OK);
    let list = v["packs"].as_array().or_else(|| v.as_array()).unwrap();
    let coal = list.iter().find(|p| p["id"] == "coal_mining").unwrap();
    let driver = |id: &str| {
        coal["drivers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(driver("newcastle")["compare_metric"], "asp_usd_t");
    assert!(driver("usd_idr")["compare_metric"].is_null());

    let (s, v) = e.get("/api/research/COAL/prices").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d = v["drivers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "newcastle")
        .unwrap();
    assert_eq!(d["compare_metric"], "asp_usd_t");
}

#[tokio::test]
async fn every_structure_route_needs_the_bearer_token() {
    let t = common::test_app();
    let base = "/api/research/COAL/structure";
    let routes = [
        (Method::GET, base.to_string()),
        (Method::POST, format!("{base}/nodes")),
        (Method::PUT, format!("{base}/nodes/n1")),
        (Method::DELETE, format!("{base}/nodes/n1")),
        (Method::POST, format!("{base}/draft")),
    ];
    for (method, uri) in routes {
        let req = Request::builder()
            .method(method.clone())
            .uri(&uri)
            .header("content-type", "application/json")
            .header("idempotency-key", "k-auth")
            .body(Body::from("{}"))
            .unwrap();
        let res = t.send(req).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}
