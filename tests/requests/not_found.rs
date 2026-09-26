//! Unmatched paths (src/controllers/not_found.rs): a real 404, the page for
//! a browser, one line for anything else, never cached, and limited like a
//! route.

use app::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn unknown_path_is_a_404_page_for_a_browser_and_a_line_for_a_probe() {
    request::<App, _, _>(|request, _ctx| async move {
        let page = request
            .get("/nope")
            .add_header("accept", "text/html,application/xhtml+xml")
            .await;
        assert_eq!(page.status_code(), 404);
        assert_eq!(
            page.header("cache-control").to_str().expect("text"),
            "no-store",
            "a miss is never cached"
        );
        let body = page.text();
        assert!(body.contains("Page not found"), "{body}");
        assert!(
            body.contains("id=\"content\"") && body.contains("/pkg/app.css"),
            "the page is inside the shell, with the stylesheet:\n{body}"
        );

        // A probe that does not ask for HTML costs a string, not a render.
        let probe = request.get("/wp-login.php").await;
        assert_eq!(probe.status_code(), 404);
        assert_eq!(
            probe.header("cache-control").to_str().expect("text"),
            "no-store"
        );
        let body = probe.text();
        assert!(!body.contains("<html"), "no page for a probe:\n{body}");
    })
    .await;
}

/// The fallback sits inside Loco's middleware stack (`not_found::Site`,
/// first in `Hooks::middlewares`), so a miss gets what a route gets.
#[tokio::test]
#[serial]
async fn a_miss_gets_the_security_headers_and_a_request_id() {
    request::<App, _, _>(|request, _ctx| async move {
        for accept in ["text/html", "*/*"] {
            let res = request.get("/nope").add_header("accept", accept).await;
            assert_eq!(res.status_code(), 404, "{accept}");
            assert_eq!(
                res.header("x-frame-options").to_str().expect("text"),
                "DENY",
                "{accept}"
            );
            assert_eq!(
                res.header("x-content-type-options").to_str().expect("text"),
                "nosniff",
                "{accept}"
            );
            assert!(
                res.maybe_header("strict-transport-security").is_some(),
                "{accept}: HSTS"
            );
            assert!(
                res.maybe_header("x-request-id").is_some(),
                "{accept}: request id"
            );
        }
        // The page keeps its own nonce CSP: the fallback one is only added
        // where a response has none.
        let page = request.get("/nope").add_header("accept", "text/html").await;
        assert!(
            page.header("content-security-policy")
                .to_str()
                .expect("text")
                .contains("'nonce-"),
            "the not-found page's own CSP"
        );
    })
    .await;
}

/// Misses have a bucket of their own with the site-wide numbers: the
/// site-wide limiter covers matched routes only.
#[tokio::test]
#[serial]
async fn misses_past_the_burst_are_refused() {
    request::<App, _, _>(|request, _ctx| async move {
        // The site-wide numbers from config/test.yaml: a burst of 20.
        for n in 1..=20 {
            let response = request.get(&format!("/missing-{n}")).await;
            assert_eq!(response.status_code(), 404, "miss {n} is answered");
        }
        let response = request.get("/missing-21").await;
        assert_eq!(response.status_code(), 429, "the 21st miss is refused");
        assert!(
            response.maybe_header("retry-after").is_some(),
            "retry-after on the 429"
        );
        // A bucket of its own: the routes still answer.
        assert_eq!(request.get("/robots.txt").await.status_code(), 200);
    })
    .await;
}
