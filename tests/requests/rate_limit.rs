use app::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// `config/test.yaml` sets `settings.rate_limit.burst` to this and keys on
/// the TCP peer, which the harness provides (it serves over a real loopback
/// port), so every request here shares the `127.0.0.1` bucket. All requests
/// finish in milliseconds, well inside the one-second refill.
const BURST: u32 = 20;

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
        let retry_after = res.header("retry-after");
        assert!(
            retry_after
                .to_str()
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .is_some(),
            "retry-after is not a number of seconds: {retry_after:?}"
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
