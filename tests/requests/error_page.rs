use app::{app::App, middleware::error_page::ErrorPages};
use axum::{Router, http::HeaderMap, routing::get};
use loco_rs::{TestServer, controller::middleware::MiddlewareLayer, testing::prelude::*};
use serial_test::serial;

const BROWSER: &str = "text/html,application/xhtml+xml,*/*;q=0.8";

fn content_type(headers: &HeaderMap) -> String {
    headers
        .get("content-type")
        .and_then(|value| value.to_str().ok().map(str::to_owned))
        .unwrap_or_default()
}

/// A wrong method on a page used to be a 405 with an empty body, which a
/// browser may save as an empty download.
#[tokio::test]
#[serial]
async fn a_browser_gets_a_page_for_a_wrong_method() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.post("/").add_header("accept", BROWSER).await;

        assert_eq!(res.status_code(), 405);
        assert!(
            content_type(res.headers()).starts_with("text/html"),
            "{res:?}"
        );
        let body = res.text();
        assert!(
            body.starts_with("<!DOCTYPE html>"),
            "not a document:\n{body}"
        );
        assert!(
            body.contains("This page can&#x27;t do that")
                || body.contains("This page can't do that"),
            "{body}"
        );
        assert!(res.maybe_header("allow").is_some(), "Allow header lost");
        assert_eq!(res.header("cache-control"), "no-store");
        assert!(
            res.header("content-security-policy")
                .to_str()
                .unwrap_or("")
                .contains("'nonce-"),
            "the page CSP, with its nonce"
        );
    })
    .await;
}

/// Anything that doesn't ask for HTML keeps the original answer.
#[tokio::test]
#[serial]
async fn a_non_browser_keeps_the_original_answer() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.post("/").await;

        assert_eq!(res.status_code(), 405);
        assert!(
            !content_type(res.headers()).starts_with("text/html"),
            "{res:?}"
        );
        assert!(res.text().is_empty());
    })
    .await;
}

/// The JSON API answers in JSON, even to a browser.
#[tokio::test]
#[serial]
async fn the_api_keeps_its_json() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request
            .post("/api/auth/login")
            .add_header("accept", BROWSER)
            .json(&serde_json::json!({ "email": 1 }))
            .await;

        assert!(res.status_code().is_client_error(), "{res:?}");
        assert!(
            content_type(res.headers()).starts_with("application/json"),
            "{res:?}"
        );
    })
    .await;
}

/// A failing handler's 500, JSON from Loco, becomes the page for a
/// browser. No route of the app fails on purpose, so the layer is mounted on
/// a router with one that does.
#[tokio::test]
#[serial]
async fn a_failing_handler_gives_a_browser_the_page() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    let ctx = boot.app_context;
    let router = Router::new().route(
        "/boom",
        get(|| async { Err::<(), loco_rs::Error>(loco_rs::Error::InternalServerError) }),
    );
    let router = ErrorPages::from_context(&ctx)
        .apply(router)
        .expect("layer applies")
        .with_state(ctx.clone());
    let server = TestServer::new(router).expect("test server");

    let page = server.get("/boom").add_header("accept", BROWSER).await;
    assert_eq!(page.status_code(), 500);
    assert!(
        content_type(page.headers()).starts_with("text/html"),
        "{page:?}"
    );
    assert!(page.text().contains("Something went wrong on our side"));

    let json = server.get("/boom").await;
    assert_eq!(json.status_code(), 500);
    assert!(json.text().contains("internal_server_error"));
}
