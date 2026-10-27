#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::Router;
use investing_app::config::Config;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub struct TestApp {
    pub app: Router,
    pub cfg: Config,
    pub dir: tempfile::TempDir,
}

pub fn test_app() -> TestApp {
    test_app_with(|_| {})
}

pub fn test_app_with(tweak: impl FnOnce(&mut Config)) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::for_tests(dir.path());
    tweak(&mut cfg);
    TestApp {
        app: investing_app::build_app(cfg.clone()),
        cfg,
        dir,
    }
}

impl TestApp {
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.cfg.token)
    }

    pub async fn send(&self, req: Request<Body>) -> Response<Body> {
        self.app.clone().oneshot(req).await.unwrap()
    }

    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let req = Request::get(uri)
            .header("authorization", self.bearer())
            .body(Body::empty())
            .unwrap();
        json_of(self.send(req).await).await
    }
}

pub async fn json_of(res: Response<Body>) -> (StatusCode, Value) {
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, v)
}
