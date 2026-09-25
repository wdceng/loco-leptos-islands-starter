use app::app::App;
use axum::http::HeaderValue;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// `config/test.yaml` sets `settings.rate_limit.burst` to this and keys on
/// the TCP peer, which the harness provides (it serves over a real loopback
/// port), so every request here shares the `127.0.0.1` bucket. All requests
/// finish in milliseconds, well inside the one-second refill.
const BURST: u32 = 20;

/// `settings.rate_limit.auth.burst` in `config/test.yaml`: the bucket on
/// `/api/auth` alone, inside the site-wide one.
const AUTH_BURST: u32 = 10;

/// A `retry-after` value, in whole seconds.
fn retry_after_secs(value: &HeaderValue) -> u64 {
    value
        .to_str()
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("retry-after is not a number of seconds: {value:?}"))
}

#[tokio::test]
#[serial]
async fn request_past_the_burst_is_rejected_with_a_page() {
    request::<App, _, _>(|request, _ctx| async move {
        // Within the burst: allowed, and the bucket state is visible.
        for remaining in (0..BURST).rev() {
            let res = request.get("/robots.txt").await;
            assert_eq!(res.status_code(), 200);
            assert_eq!(res.header("x-ratelimit-limit"), BURST.to_string());
            assert_eq!(res.header("x-ratelimit-remaining"), remaining.to_string());
        }

        // Bucket empty: 429 with the rendered page and the retry headers.
        let res = request.get("/robots.txt").await;
        assert_eq!(res.status_code(), 429);
        // The bucket refills once a second, which tower_governor rounds down
        // to 0; `Retry-After: 0` would send the client straight back.
        assert_eq!(
            retry_after_secs(&res.header("retry-after")),
            1,
            "a wait under a second is 1, never 0"
        );
        assert_eq!(res.header("x-ratelimit-remaining"), "0");
        assert_eq!(res.header("cache-control"), "no-store");
        assert!(
            res.header("content-type")
                .to_str()
                .unwrap_or("")
                .starts_with("text/html"),
            "429 must be an HTML page"
        );
        let body = res.text();
        assert!(
            body.starts_with("<!doctype html>"),
            "not a document:\n{body}"
        );
        assert!(body.contains("Too many requests"));
        assert!(
            !body.contains("{wait}"),
            "placeholder not replaced:\n{body}"
        );

        // Unmatched paths never reach the limiter (`route_layer`): still 404.
        let res = request.get("/nope").await;
        assert_eq!(res.status_code(), 404);
    })
    .await;
}

/// The auth API has a second, stricter bucket (`controllers::auth::routes`):
/// `register` mails any address and `login` takes a password. A request
/// there spends a token from both buckets, and the site-wide layer, being
/// the outer one, writes the `x-ratelimit-*` headers last.
#[tokio::test]
#[serial]
async fn the_auth_api_has_its_own_stricter_bucket() {
    request::<App, _, _>(|request, _ctx| async move {
        let payload = serde_json::json!({ "email": "nobody@example.com", "password": "wrong" });

        // Within the auth burst: refused as bad credentials, not as too many.
        // The headers show the site-wide bucket, twenty, counting down.
        for attempt in 1..=AUTH_BURST {
            let res = request.post("/api/auth/login").json(&payload).await;
            assert_eq!(res.status_code(), 401, "login attempt {attempt}");
            assert_eq!(res.header("x-ratelimit-limit"), BURST.to_string());
            assert_eq!(
                res.header("x-ratelimit-remaining"),
                (BURST - attempt).to_string()
            );
        }

        // The auth bucket is empty: the 429 page, with the wait until its
        // next token.
        let res = request.post("/api/auth/login").json(&payload).await;
        assert_eq!(res.status_code(), 429);
        // The wait is the auth bucket's, one token per 30 s, not the
        // site-wide bucket's second.
        let wait = retry_after_secs(&res.header("retry-after"));
        assert!(
            (2..=31).contains(&wait),
            "retry-after {wait} is not the auth bucket's wait"
        );
        assert_eq!(res.header("cache-control"), "no-store");
        assert!(res.text().contains("Too many requests"));

        // The site-wide bucket still has tokens (twenty, minus the eleven
        // auth requests and this one), so everything else still answers.
        let res = request.get("/robots.txt").await;
        assert_eq!(res.status_code(), 200);
        assert_eq!(
            res.header("x-ratelimit-remaining"),
            (BURST - AUTH_BURST - 2).to_string()
        );
    })
    .await;
}
