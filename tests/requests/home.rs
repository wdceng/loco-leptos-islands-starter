use app::{app::App, views::layout::APP_NAME};
use chrono::Datelike;
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn home_renders_html() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/").await;

        assert_eq!(res.status_code(), 200);
        let content_type = res.header("content-type");
        assert!(
            content_type.to_str().unwrap_or("").starts_with("text/html"),
            "unexpected content type: {content_type:?}"
        );

        let body = res.text();
        assert!(
            body.starts_with("<!DOCTYPE html>"),
            "not a document:\n{body}"
        );
        assert!(body.contains("<html lang=\"en\">"));
        assert!(body.contains(&format!("<title>{APP_NAME}</title>")));
        // Footer year is computed at render time, not hardcoded.
        let year = chrono::Utc::now().year();
        assert!(
            body.contains(&format!("© {year} {APP_NAME}")),
            "footer year missing or stale:\n{body}"
        );
        // The shell must wire up the islands bundle and the stylesheet. No
        // hash file sits next to the test binary, so the names are plain.
        assert!(
            body.contains("/pkg/app.js"),
            "hydration scripts missing:\n{body}"
        );
        assert!(
            body.contains(r#"<link rel="stylesheet" href="/pkg/app.css">"#),
            "stylesheet link missing or hashed:\n{body}"
        );
        // Leptos names the wasm file at compile time from LEPTOS_OUTPUT_NAME
        // (set in .cargo/config.toml). Without it the loader asks for
        // `app_bg.wasm`, which cargo-leptos never writes.
        assert!(
            body.contains(r#"("", "pkg", "app", "app")"#),
            "loader script does not name app.wasm:\n{body}"
        );
    })
    .await;
}
