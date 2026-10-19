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
        // One <h1> per page, and the skip link points at an element that
        // exists.
        assert_eq!(
            body.matches("<h1").count(),
            1,
            "a page has exactly one <h1>:\n{body}"
        );
        assert!(
            body.contains("href=\"#content\"") && body.contains("id=\"content\""),
            "skip link or its target missing:\n{body}"
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
        // The home page is meant to be found: no robots tag.
        assert!(
            !body.contains(r#"name="robots""#),
            "the home page must stay indexable:\n{body}"
        );
        // The manifest is fetched with cookies, or a Cloudflare challenge on
        // staging answers it with an HTML page.
        assert!(
            body.contains(r#"crossorigin="use-credentials""#),
            "manifest link without use-credentials:\n{body}"
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

/// The lazy `IslandsCheck` (src/islands.rs) is rendered on the home page,
/// wrapped as an island, in its server state. The browser switches it to
/// "running" once its split chunk has loaded (docs/testing.md, "Islands in
/// the browser").
#[tokio::test]
#[serial]
async fn home_renders_the_lazy_islands_check() {
    request::<App, _, _>(|request, _ctx| async move {
        let body = request.get("/").await.text();
        let island = body
            .find("<leptos-island")
            .map(|start| &body[start..])
            .unwrap_or_else(|| panic!("no island on the home page:\n{body}"));
        assert!(
            island.contains("IslandsCheck"),
            "the island is not IslandsCheck:\n{island}"
        );
        assert!(
            island.contains("Islands: not running."),
            "the island's server state is missing:\n{island}"
        );
    })
    .await;
}

/// The shell moves between pages in place (src/views/layout.rs): htmx's
/// settings and the scripts in the order they must run, the page's region
/// boosted with every page status, the live region, the `Navigation`
/// island outside the region, and the build's name in the head and the
/// header alike.
#[tokio::test]
#[serial]
async fn the_shell_moves_between_pages_in_place() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/").await;
        let build = res.header("x-build").to_str().expect("ascii").to_string();
        assert_eq!(build.len(), 16, "x-build: {build}");
        let body = res.text();
        assert!(
            body.contains(&format!(r#"<meta name="build" content="{build}">"#)),
            "the head's build differs from the header's:\n{body}"
        );
        assert!(body.contains(r#"<meta name="htmx-config""#), "{body}");

        // The idle timeout first, htmx, its extensions and the glue after it,
        // Alpine last.
        let order = [
            "/js/island-loader-timeout.js",
            "/vendor/htmx-4.0.0.min.js",
            "/vendor/hx-alpine-compat-4.0.0.min.js",
            "/vendor/hx-preload-4.0.0.min.js",
            "/js/htmx-islands.js",
            "/vendor/alpinejs-csp-3.17.4.min.js",
        ];
        let at: Vec<usize> = order
            .iter()
            .map(|script| {
                body.find(&format!(r#"<script src="{script}?v="#))
                    .unwrap_or_else(|| panic!("{script} not loaded:\n{body}"))
            })
            .collect();
        assert!(at.is_sorted(), "scripts out of order: {order:?} at {at:?}");
        assert!(
            body.matches(" defer></script>").count() >= order.len(),
            "every script defer:\n{body}"
        );
        // The idle timeout before Leptos's island loader, a module script,
        // which waits for idle time (public/js/island-loader-timeout.js).
        let loader = body
            .find(r#"<script type="module""#)
            .unwrap_or_else(|| panic!("no island loader:\n{body}"));
        assert!(
            at[0] < loader,
            "island-loader-timeout.js after the island loader"
        );

        let region = body
            .find(r#"<div id="page""#)
            .map(|start| {
                &body[start
                    ..body[start..]
                        .find('>')
                        .map_or(body.len(), |end| start + end)]
            })
            .unwrap_or_else(|| panic!("no page region:\n{body}"));
        assert!(region.contains("hx-boost:inherited"), "{region}");
        assert!(region.contains("hx-history-elt"), "{region}");
        for status in [400, 404, 405, 408, 413, 415, 500] {
            assert!(
                region.contains(&format!("hx-status:{status}:inherited")),
                "{status} is not shown in the region: {region}"
            );
        }
        assert!(
            body.contains(r#"<main id="content" tabindex="-1""#),
            "{body}"
        );
        assert!(
            body.contains(r#"<p id="page-announce" role="status""#),
            "{body}"
        );
        let navigation = body
            .find(r#"data-component="Navigation"#)
            .unwrap_or_else(|| panic!("no Navigation island:\n{body}"));
        let region_end = body.rfind("</footer></div>").expect("the region closes");
        assert!(navigation > region_end, "Navigation is inside the region");
    })
    .await;
}
