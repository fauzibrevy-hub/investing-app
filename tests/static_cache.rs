//! Cache policy for the built UI (plan.md §P5).
//!
//! Hashed assets may be cached forever; `index.html`, `sw.js` and the manifest must be revalidated,
//! otherwise a new deploy never reaches an installed PWA.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::test_app_with;

fn app_with_dist() -> common::TestApp {
    let t = test_app_with(|_| {});
    let dist = t.cfg.web_dist.clone();
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(dist.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    std::fs::write(dist.join("sw.js"), "self.addEventListener('fetch',()=>{})").unwrap();
    std::fs::write(dist.join("manifest.webmanifest"), "{\"name\":\"x\"}").unwrap();
    std::fs::write(dist.join("assets/index-abc123.js"), "console.log(1)").unwrap();
    t
}

async fn cache_control(t: &common::TestApp, uri: &str) -> (StatusCode, String) {
    let res = t.send(Request::get(uri).body(Body::empty()).unwrap()).await;
    let status = res.status();
    let value = res
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    (status, value)
}

#[tokio::test]
async fn hashed_assets_are_immutable() {
    let t = app_with_dist();
    let (status, cc) = cache_control(&t, "/assets/index-abc123.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(cc.contains("immutable"), "got {cc:?}");
    assert!(cc.contains("max-age=31536000"), "got {cc:?}");
}

#[tokio::test]
async fn entry_points_must_revalidate() {
    let t = app_with_dist();
    for uri in [
        "/",
        "/index.html",
        "/sw.js",
        "/manifest.webmanifest",
        "/icon-192.png",
    ] {
        let (status, cc) = cache_control(&t, uri).await;
        // icon-192.png does not exist in this fixture; the header still must be no-cache.
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "{uri}: {status}"
        );
        assert_eq!(cc, "no-cache", "{uri}");
    }
}

#[tokio::test]
async fn api_responses_are_not_given_a_static_policy() {
    let t = app_with_dist();
    let (status, cc) = cache_control(&t, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cc, "", "/api/* must not inherit the static cache policy");
}
