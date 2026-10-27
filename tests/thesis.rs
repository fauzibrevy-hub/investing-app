//! Area 3 against a synthetic vault. Expected JSON in tests/fixtures/thesis_golden
//! was produced by MC's own parsers (scripts/golden/thesis_golden.py), so these
//! tests pin byte-for-byte parity of the read endpoints.

mod common;

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{json_of, test_app_with, TestApp};
use serde_json::{json, Value};

fn fixture_vault() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault")
}

fn golden(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/thesis_golden/{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// Same normalization as scripts/parity.sh (integral floats == ints), plus
/// dropping fields that depend on the current time.
fn normalize(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| !matches!(k.as_str(), "generated_at" | "days_left" | "days_ago"))
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
            _ => v.clone(),
        },
        other => other.clone(),
    }
}

fn assert_parity(actual: &Value, expected: &Value, what: &str) {
    let (a, e) = (normalize(actual), normalize(expected));
    if a != e {
        panic!(
            "{what} differs from MC golden\n--- expected\n{}\n--- actual\n{}",
            serde_json::to_string_pretty(&e).unwrap(),
            serde_json::to_string_pretty(&a).unwrap()
        );
    }
}

fn app() -> TestApp {
    test_app_with(|cfg| cfg.vault_root = fixture_vault())
}

#[tokio::test]
async fn summary_matches_mc() {
    let (s, body) = app().get("/api/thesis/summary").await;
    assert_eq!(s, StatusCode::OK);
    assert_parity(&body, &golden("summary"), "summary");
    assert!(body["generated_at"].is_u64());
}

#[tokio::test]
async fn details_match_mc() {
    let t = app();
    for ticker in ["AAAA", "BBBB", "CCCC"] {
        let (s, body) = t
            .get(&format!("/api/thesis/{}", ticker.to_lowercase()))
            .await;
        assert_eq!(s, StatusCode::OK, "{ticker}");
        assert_parity(&body, &golden(&format!("detail_{ticker}")), ticker);
    }
    let (s, body) = t.get("/api/thesis/ZZZZ").await;
    assert_eq!(
        (s, body),
        (
            StatusCode::NOT_FOUND,
            json!({"detail": "Dossier ZZZZ tidak ditemukan di finance/stock"})
        )
    );
}

#[tokio::test]
async fn macro_lens_and_vault_drafts_match_mc() {
    let t = app();
    let (_, body) = t.get("/api/thesis/macro").await;
    assert_parity(&body, &golden("macro"), "macro");
    let (_, body) = t.get("/api/thesis/drafts/vault").await;
    assert_parity(&body, &golden("drafts_vault"), "drafts/vault");
}

#[tokio::test]
async fn days_are_relative_to_today() {
    let (_, body) = app().get("/api/thesis/summary").await;
    let aaaa = &body["issuers"][0];
    let today = chrono::Local::now().date_naive();
    let expect_left = (chrono::NaiveDate::from_ymd_opt(2099, 6, 30).unwrap() - today).num_days();
    assert_eq!(aaaa["falsifiers"]["nearest"]["days_left"], expect_left);
    assert_eq!(aaaa["falsifiers"]["past_count"], 1);
    let expect_ago = (today - chrono::NaiveDate::from_ymd_opt(2025, 9, 15).unwrap()).num_days();
    assert_eq!(aaaa["review"]["days_ago"], expect_ago);
}

#[tokio::test]
async fn missing_vault_root() {
    let t = test_app_with(|cfg| cfg.vault_root = PathBuf::from("/nonexistent/vault"));
    let (s, body) = t.get("/api/thesis/summary").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["root_ok"], false);
    let (_, body) = t.get("/api/thesis/macro").await;
    assert_eq!(body["errors"].as_array().unwrap().len(), 2);
}

/// Every file's mtime and size under `dir`, to prove nothing was written.
fn snapshot(dir: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else {
                out.push((e.path(), m.len(), m.modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[tokio::test]
async fn drafts_crud_never_touches_the_vault() {
    let before = snapshot(&fixture_vault());
    let t = app();
    let post = |body: Value| {
        Request::post("/api/thesis/drafts")
            .header("authorization", t.bearer())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let (s, created) = json_of(t.send(post(json!({"ticker": "test", "title": "ACCEPTANCE", "thesis_statement": "x", "falsifiers": ["y"]}))).await).await;
    assert_eq!(s, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("draft_test_"), "{id}");
    assert_eq!(created["ticker"], "TEST");
    assert!(created["created_at"].as_str().unwrap().ends_with("+00:00"));

    let on_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(&t.cfg.thesis_drafts_path).unwrap()).unwrap();
    assert_eq!(on_disk[&id], created);

    let (_, list) = t.get("/api/thesis/drafts?ticker=test").await;
    assert_eq!(list["drafts"], json!([created.clone()]));
    assert_eq!(
        list["total"],
        1 + list["vault_drafts"].as_array().unwrap().len()
    );
    let (_, one) = t.get(&format!("/api/thesis/drafts/{id}")).await;
    assert_eq!(one, created);
    let (s, vault_one) = t.get("/api/thesis/drafts/T01").await;
    assert_eq!(
        (s, vault_one["title"].as_str()),
        (StatusCode::OK, Some("Minyak naik"))
    );

    let (s, body) = json_of(
        t.send(post(
            json!({"ticker": "X", "title": "t", "thesis_statement": "s"}),
        ))
        .await,
    )
    .await;
    assert_eq!(
        (s, body),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"detail": "Format ticker emiten tidak valid (2-10 karakter alfanumerik)."})
        )
    );

    let del = |id: &str| {
        Request::delete(format!("/api/thesis/drafts/{id}"))
            .header("authorization", t.bearer())
            .body(Body::empty())
            .unwrap()
    };
    let res = t.send(del(&id)).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (s, body) = json_of(t.send(del(&id)).await).await;
    assert_eq!(
        (s, body),
        (
            StatusCode::NOT_FOUND,
            json!({"detail": format!("Draf {id} tidak ditemukan.")})
        )
    );
    let on_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(&t.cfg.thesis_drafts_path).unwrap()).unwrap();
    assert_eq!(on_disk, json!({}));

    assert_eq!(
        snapshot(&fixture_vault()),
        before,
        "vault must stay untouched (prd A3.4)"
    );
}

#[tokio::test]
async fn corrupted_drafts_store_is_503() {
    let t = app();
    std::fs::write(&t.cfg.thesis_drafts_path, "{not json").unwrap();
    let (s, body) = t.get("/api/thesis/drafts").await;
    assert_eq!(
        (s, body),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"detail": "Drafts store corrupted or unavailable"})
        )
    );
}
