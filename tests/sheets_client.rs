//! SheetsClient against a local mock of the Sheets v4 REST surface.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use finance_app::sheets::{SheetsClient, SheetsError, TokenSource};
use serde_json::{json, Value};

#[derive(Default)]
struct Seen {
    calls: Vec<(String, String, Value)>,
}

type Shared = Arc<Mutex<Seen>>;

fn authed(h: &HeaderMap) -> bool {
    h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer test-token")
}

async fn values(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((id, range)): Path<(String, String)>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    if !authed(&headers) {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"message": "bad token"}})),
        ));
    }
    s.lock()
        .unwrap()
        .calls
        .push(("get".into(), format!("{id}|{range}"), Value::Null));
    Ok(Json(
        json!({"range": range, "values": [["Date", "Amount"], ["01/10/2026", "Rp1.000"], []]}),
    ))
}

async fn update(
    State(s): State<Shared>,
    Path((id, range)): Path<(String, String)>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    s.lock().unwrap().calls.push((
        format!("put:{}", q["valueInputOption"]),
        format!("{id}|{range}"),
        body,
    ));
    Json(json!({"updatedCells": 1}))
}

async fn batch(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    s.lock().unwrap().calls.push(("batch".into(), id, body));
    Json(json!({"replies": []}))
}

async fn spawn_mock() -> (String, Shared) {
    let seen: Shared = Arc::default();
    let app = Router::new()
        .route(
            "/v4/spreadsheets/{id}/values/{range}",
            get(values).put(update),
        )
        .route("/v4/spreadsheets/{id}", post(batch))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/v4/spreadsheets"), seen)
}

#[tokio::test]
async fn values_get_returns_string_rows() {
    let (base, seen) = spawn_mock().await;
    let client = SheetsClient::with_parts(&base, TokenSource::Static("test-token".into()));
    let rows = client
        .values_get("sheet1", "'Transaction Log'!A1:Z")
        .await
        .unwrap();
    assert_eq!(rows[1], vec!["01/10/2026".to_string(), "Rp1.000".into()]);
    assert!(rows[2].is_empty());
    assert_eq!(
        seen.lock().unwrap().calls[0].1,
        "sheet1|'Transaction Log'!A1:Z"
    );
}

#[tokio::test]
async fn values_update_uses_user_entered_on_explicit_range() {
    let (base, seen) = spawn_mock().await;
    let client = SheetsClient::with_parts(&base, TokenSource::Static("test-token".into()));
    client
        .values_update(
            "s",
            "'Transaction Log'!D20:H20",
            vec![vec![json!("Cash"), json!(1234)]],
        )
        .await
        .unwrap();
    let calls = &seen.lock().unwrap().calls;
    assert_eq!(calls[0].0, "put:USER_ENTERED");
    assert_eq!(calls[0].1, "s|'Transaction Log'!D20:H20");
    assert_eq!(calls[0].2["values"], json!([["Cash", 1234]]));
}

#[tokio::test]
async fn http_errors_surface_status_and_message() {
    let (base, _) = spawn_mock().await;
    let client = SheetsClient::with_parts(&base, TokenSource::Static("wrong".into()));
    match client.values_get("s", "A1").await {
        Err(SheetsError::Status { status, message }) => {
            assert_eq!(status, 401);
            assert_eq!(message, "bad token");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn missing_credentials_file_is_a_credentials_error() {
    let client = SheetsClient::new("/nonexistent/sa.json".into());
    assert!(matches!(
        client.values_get("s", "A1").await,
        Err(SheetsError::Credentials(_))
    ));
}
