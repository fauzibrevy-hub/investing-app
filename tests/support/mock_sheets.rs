//! Minimal mock of the Sheets v4 REST API serving synthetic data.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

#[derive(Default)]
pub struct MockData {
    /// A1 range → rows.
    pub ranges: HashMap<String, Value>,
    pub named_ranges: Value,
    pub sheets: Value,
    /// Ranges that should fail with 500.
    pub failing: Vec<String>,
    /// Every request as "METHOD range" for assertions.
    pub log: Vec<String>,
    /// Bodies of values.update calls: (range, body).
    pub updates: Vec<(String, Value)>,
    pub batch_updates: Vec<Value>,
}

pub type Mock = Arc<Mutex<MockData>>;

async fn values_get(
    State(m): State<Mock>,
    Path((_id, range)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let mut d = m.lock().unwrap();
    // Unformatted reads are served from "<range>|UNFORMATTED_VALUE".
    let range = match q.get("valueRenderOption") {
        Some(opt) => format!("{range}|{opt}"),
        None => range,
    };
    d.log.push(format!("GET {range}"));
    if d.failing.contains(&range) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"message": "boom"}})),
        );
    }
    let values = match range.as_str() {
        // Live view of the ledger, so reads after writes see them.
        "'Transaction Log'!B12:H" => {
            let ledger = d
                .ranges
                .get("'Transaction Log'!A1:Z")
                .cloned()
                .unwrap_or(json!([]));
            let rows: Vec<Value> = ledger
                .as_array()
                .unwrap()
                .iter()
                .skip(11)
                .map(|r| {
                    let cells = r.as_array().cloned().unwrap_or_default();
                    json!(cells.into_iter().skip(1).take(7).collect::<Vec<_>>())
                })
                .collect();
            json!(rows)
        }
        _ => d.ranges.get(&range).cloned().unwrap_or(json!([])),
    };
    (
        StatusCode::OK,
        Json(json!({"range": range, "values": values})),
    )
}

async fn values_put(
    State(m): State<Mock>,
    Path((_id, range)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut d = m.lock().unwrap();
    d.log.push(format!("PUT {range}"));
    d.updates.push((range.clone(), body.clone()));
    // Reflect the write so later reads see it (B column / D:H on the ledger).
    if let Some(rows) = d
        .ranges
        .get_mut("'Transaction Log'!A1:Z")
        .and_then(Value::as_array_mut)
    {
        let (cols, row) = parse_range(&range);
        let row_idx = row - 1;
        while rows.len() <= row_idx {
            rows.push(json!([]));
        }
        let r = rows[row_idx].as_array_mut().unwrap();
        for (i, v) in body["values"][0].as_array().unwrap().iter().enumerate() {
            let c = cols + i;
            while r.len() <= c {
                r.push(json!(""));
            }
            r[c] = json!(match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        }
    }
    Json(json!({"updatedRange": range}))
}

fn parse_range(range: &str) -> (usize, usize) {
    let a1 = range.rsplit('!').next().unwrap().split(':').next().unwrap();
    let col = (a1.as_bytes()[0] - b'A') as usize;
    let row: usize = a1[1..].parse().unwrap();
    (col, row)
}

async fn spreadsheet(
    State(m): State<Mock>,
    Path(_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let d = m.lock().unwrap();
    match q.get("fields").map(String::as_str) {
        Some("namedRanges") => Json(json!({"namedRanges": d.named_ranges})),
        _ => Json(json!({"sheets": d.sheets})),
    }
}

async fn batch(
    State(m): State<Mock>,
    Path(_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut d = m.lock().unwrap();
    d.log.push("BATCH".into());
    d.batch_updates.push(body);
    Json(json!({"replies": []}))
}

/// Starts the mock; returns the API base URL to pass to `SheetsClient::with_parts`.
pub async fn start(data: MockData) -> (String, Mock) {
    let mock: Mock = Arc::new(Mutex::new(data));
    let app = Router::new()
        .route(
            "/v4/spreadsheets/{id}/values/{range}",
            get(values_get).put(values_put),
        )
        .route("/v4/spreadsheets/{id}", get(spreadsheet).post(batch))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/v4/spreadsheets"), mock)
}

pub fn rows(data: &[&[&str]]) -> Value {
    json!(data)
}
