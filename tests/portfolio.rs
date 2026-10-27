//! `GET /api/finance/stock-portfolio` against a mock Sheets API with synthetic data.

#[path = "support/mock_sheets.rs"]
mod mock_sheets;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use investing_app::config::Config;
use investing_app::portfolio::{router_with, PortfolioState};
use investing_app::sheets::{SheetsClient, TokenSource};
use mock_sheets::{rows, Mock, MockData};
use serde_json::Value;
use tower::ServiceExt;

/// Only the portfolio range: this app reads no other sheet.
fn data() -> MockData {
    let mut d = MockData::default();
    d.ranges.insert(
        "PORTOFOLIO!A1:AE35".into(),
        rows(&[
            &[""],
            &[""],
            &[""],
            &[""],
            &[""],
            &[""],
            &[""],
            &["", "No"],
            &[
                "", "1", "CORE", "IDX:TLKM", "Telkom", "200", "3,000", "3,100", "600,000",
                "620,000", "20,000", "3.33%", "10%", "10.5%", "10%",
            ],
        ]),
    );
    d
}

async fn app_with(d: MockData) -> (Router, Mock) {
    let (base, mock) = mock_sheets::start(d).await;
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::for_tests(dir.path());
    let state = PortfolioState::new(
        &cfg,
        SheetsClient::with_parts(&base, TokenSource::Static("t".into())),
    );
    (router_with(Arc::new(state)), mock)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn stock_portfolio_reads_portfolio_spreadsheet() {
    let (app, mock) = app_with(data()).await;
    let (s, body) = get(&app, "/api/finance/stock-portfolio").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["positions"][0]["ticker"], "TLKM");
    assert_eq!(body["summary"]["total_cost"], 600_000);
    assert_eq!(body["spreadsheet_id"], "test-portfolio");
    assert!(body["summary"]["updated_at"]
        .as_str()
        .unwrap()
        .contains('T'));
    assert!(mock
        .lock()
        .unwrap()
        .log
        .contains(&"GET PORTOFOLIO!A1:AE35".to_string()));

    let mut d = data();
    d.failing = vec!["PORTOFOLIO!A1:AE35".into()];
    let (app, _) = app_with(d).await;
    let (s, body) = get(&app, "/api/finance/stock-portfolio").await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .starts_with("Failed to fetch stock portfolio: "));
}
