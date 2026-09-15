use app::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Every `nonce="..."` attribute value in the document, in order.
fn nonces_in(body: &str) -> Vec<&str> {
    body.split("nonce=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect()
}

/// A page carries the nonce CSP from `settings.security`, with the same
/// nonce Leptos stamped on its inline scripts, plus the preset headers and
/// overrides from `secure_headers`.
#[tokio::test]
#[serial]
async fn page_has_nonce_csp_and_preset_headers() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/").await;
        assert_eq!(res.status_code(), 200);

        let csp = res.header("content-security-policy");
        let csp = csp.to_str().expect("ascii header");
        let body = res.text();

        let nonces = nonces_in(&body);
        let nonce = *nonces.first().expect("inline scripts must carry a nonce");
        assert!(!nonce.is_empty());
        assert!(
            nonces.iter().all(|n| *n == nonce),
            "inline scripts carry different nonces: {nonces:?}"
        );
        assert!(
            csp.contains(&format!("'nonce-{nonce}'")),
            "CSP does not allow the page's nonce:\n{csp}"
        );
        assert!(csp.starts_with("default-src 'none';"), "{csp}");
        assert!(csp.contains("'wasm-unsafe-eval'"), "{csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
        assert!(!csp.contains("unsafe-inline"), "{csp}");
        assert!(!csp.contains("{nonce}"), "placeholder left in:\n{csp}");
        // style-src 'self' only holds while nothing renders a style attribute.
        assert!(!body.contains(" style=\""), "inline style found:\n{body}");

        let header = |name: &str| res.header(name).to_str().unwrap_or_default().to_owned();
        assert_eq!(header("x-frame-options"), "DENY");
        assert_eq!(header("referrer-policy"), "strict-origin-when-cross-origin");
        assert_eq!(
            header("permissions-policy"),
            "camera=(), microphone=(), geolocation=()"
        );
        assert_eq!(header("cross-origin-opener-policy"), "same-origin");
        assert_eq!(header("cross-origin-resource-policy"), "same-origin");
        assert_eq!(header("cross-origin-embedder-policy"), "require-corp");
        assert_eq!(header("x-content-type-options"), "nosniff");
        assert_eq!(
            header("strict-transport-security"),
            "max-age=31536000; includeSubDomains; preload"
        );
        assert!(res.headers().get("x-powered-by").is_none());
    })
    .await;
}

/// A response that sets no CSP of its own gets the preset one: Loco only
/// adds headers that are not already present.
#[tokio::test]
#[serial]
async fn other_responses_get_the_preset_csp() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/robots.txt").await;
        assert_eq!(res.status_code(), 200);

        let csp = res.header("content-security-policy");
        let csp = csp.to_str().expect("ascii header");
        assert!(csp.starts_with("default-src 'self' https:"), "{csp}");
        assert!(!csp.contains("nonce"), "{csp}");
        assert_eq!(
            res.header("referrer-policy"),
            "strict-origin-when-cross-origin"
        );
    })
    .await;
}
