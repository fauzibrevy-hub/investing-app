mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::{json_of, test_app};
use serde_json::json;

#[tokio::test]
async fn health_is_public() {
    let t = test_app();
    let (status, body) = json_of(
        t.send(Request::get("/api/health").body(Body::empty()).unwrap())
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"status": "ok"}));
}

#[tokio::test]
async fn deep_health_reports_sources() {
    let t = test_app();
    let (status, body) = t.get("/api/health?deep=1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["sources"]["sheets"]["ok"].is_boolean());
    assert_eq!(body["status"], "degraded");
}

#[tokio::test]
async fn api_requires_auth() {
    let t = test_app();
    for uri in ["/api/finance/summary", "/api/nope", "/api/idx/summary"] {
        let res = t.send(Request::get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    let res = t
        .send(
            Request::get("/api/finance/summary")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_passes_auth() {
    let t = test_app();
    let (status, body) = t.get("/api/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "Not Found"}));
}

#[tokio::test]
async fn session_cookie_flow() {
    let t = test_app();
    let bad = t
        .send(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token": "nope"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    let res = t
        .send(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token": t.cfg.token}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let set_cookie = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    for attr in [
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
        "Path=/",
        "Max-Age=2592000",
    ] {
        assert!(set_cookie.contains(attr), "{attr} missing");
    }
    let cookie = set_cookie.split(';').next().unwrap().to_string();

    let get = t
        .send(
            Request::get("/api/nope")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        get.status(),
        StatusCode::NOT_FOUND,
        "cookie GET is authenticated"
    );

    let write = t
        .send(
            Request::post("/api/nope")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        write.status(),
        StatusCode::FORBIDDEN,
        "cookie write needs X-Investing-Client"
    );

    let write = t
        .send(
            Request::post("/api/nope")
                .header("cookie", &cookie)
                .header("x-investing-client", "web")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(write.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn root_is_404_without_web_dist() {
    let t = test_app();
    let res = t.send(Request::get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_session_reports_cookie_validity() {
    let t = test_app();
    let check = |cookie: Option<&str>| {
        let mut req = Request::get("/api/session");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        req.body(Body::empty()).unwrap()
    };

    // No cookie, or a forged/foreign one → 401.
    assert_eq!(t.send(check(None)).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        t.send(check(Some("investing_session=v1.1.deadbeef")))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let other =
        investing_app::auth::Auth::new(&"z".repeat(32)).issue_session(investing_app::auth::now_secs());
    assert_eq!(
        t.send(check(Some(&format!("investing_session={other}"))))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // Cookie from POST /api/session → 204.
    let res = t
        .send(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token": t.cfg.token}).to_string()))
                .unwrap(),
        )
        .await;
    let cookie = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let ok = t.send(check(Some(&cookie))).await;
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);
}
