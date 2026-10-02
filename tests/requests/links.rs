//! Every address the app writes as a string leads somewhere: the links and
//! asset paths in the pages, the files the manifest and the stylesheet
//! name, and the links in the mails. The compiler checks none of these.
//!
//! The test environment serves no files (`config/test.yaml` has no `static`
//! block), so asset paths are checked against `public/` on disk and only
//! routes are requested. `/pkg/…` is cargo-leptos's output, not in
//! `public/`; `home.rs` pins those names.

use std::path::Path;

use app::{app::App, middleware::rate_limit};
use loco_rs::testing::prelude::*;
use regex::Regex;
use serial_test::serial;

const BROWSER: &str = "text/html,application/xhtml+xml,*/*;q=0.8";

/// A file in `public/`, which cargo-leptos copies to the site root. A
/// `?v=<hash>` (src/paths.rs) is part of the URL, not of the file name.
fn in_public(path: &str) -> bool {
    let file = path.split('?').next().unwrap_or_default();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("public")
        .join(file.trim_start_matches('/'))
        .is_file()
}

/// Every `href` and `src` value in a page.
fn links(page: &str) -> Vec<String> {
    let re = Regex::new(r#"(?:href|src)="([^"]*)""#).expect("regex");
    re.captures_iter(page).map(|c| c[1].to_string()).collect()
}

#[tokio::test]
#[serial]
async fn every_link_in_the_pages_leads_somewhere() {
    request::<App, _, _>(|request, _ctx| async move {
        let pages = vec![
            ("home", request.get("/").add_header("accept", BROWSER).await.text()),
            (
                "not found",
                request.get("/nope").add_header("accept", BROWSER).await.text(),
            ),
            (
                "error page",
                request.post("/").add_header("accept", BROWSER).await.text(),
            ),
            ("429", rate_limit::page_for("/pkg/app.css")),
        ];

        let mut broken = Vec::new();
        for (name, page) in &pages {
            let found = links(page);
            assert!(!found.is_empty(), "no links found in the {name} page:\n{page}");
            for link in found {
                if let Some(id) = link.strip_prefix('#') {
                    if !page.contains(&format!("id=\"{id}\"")) {
                        broken.push(format!("{name}: {link} has no element with that id"));
                    }
                } else if link.starts_with("/pkg/") || !link.starts_with('/') {
                    // cargo-leptos output (home.rs), or another origin.
                } else if !in_public(&link) {
                    let status = request
                        .get(&link)
                        .add_header("accept", BROWSER)
                        .await
                        .status_code();
                    if status != 200 {
                        broken.push(format!(
                            "{name}: {link} is neither a file in public/ nor a route (status {status})"
                        ));
                    }
                }
            }
        }
        assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
    })
    .await;
}

/// Files named outside the pages: the manifest's icons and the
/// stylesheet's fonts.
#[test]
fn every_file_the_manifest_and_the_stylesheet_name_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("public/favicon/site.webmanifest"))
            .expect("the manifest"),
    )
    .expect("the manifest is JSON");
    let icons = manifest["icons"].as_array().expect("manifest icons");
    let mut named: Vec<String> = icons
        .iter()
        .map(|icon| icon["src"].as_str().expect("icon src").to_string())
        .collect();

    let css = std::fs::read_to_string(root.join("style/tailwind.css")).expect("the stylesheet");
    let url = Regex::new(r#"url\("?([^")]+)"?\)"#).expect("regex");
    let fonts: Vec<String> = url.captures_iter(&css).map(|c| c[1].to_string()).collect();
    assert!(!fonts.is_empty(), "no url() found in style/tailwind.css");
    named.extend(fonts);

    let missing: Vec<&String> = named.iter().filter(|path| !in_public(path)).collect();
    assert!(missing.is_empty(), "missing from public/: {missing:?}");
}

/// The links in the three auth mails, requested the way a person clicking
/// them would.
#[tokio::test]
#[serial]
async fn every_link_in_the_mails_leads_somewhere() {
    request::<App, _, _>(|request, ctx| async move {
        // `@example.com`: one of the two domains magic-link login accepts.
        let email = "links@example.com";
        request
            .post("/api/auth/register")
            .json(&serde_json::json!({ "name": "Links", "email": email, "password": "12341234" }))
            .await;
        request
            .post("/api/auth/forgot")
            .json(&serde_json::json!({ "email": email }))
            .await;
        request
            .post("/api/auth/magic-link")
            .json(&serde_json::json!({ "email": email }))
            .await;

        let deliveries = ctx.mailer.unwrap().deliveries();
        assert_eq!(deliveries.count, 3, "welcome, reset and magic-link mails");

        // Quoted-printable may wrap a long line ("=" + line break), so the
        // breaks come out before the links are read.
        let link = Regex::new(r"http://localhost:5150(/[A-Za-z0-9_./#?&%-]+)").expect("regex");
        let mut paths: Vec<String> = deliveries
            .messages
            .iter()
            .map(|mail| mail.replace("=\r\n", "").replace("=\n", ""))
            .flat_map(|mail| {
                link.captures_iter(&mail)
                    .map(|c| c[1].to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 3, "one link per mail: {paths:?}");

        for path in paths {
            // The part after `#` never reaches the server.
            let route = path.split('#').next().unwrap_or_default();
            let status = request
                .get(route)
                .add_header("accept", BROWSER)
                .await
                .status_code();
            if route == "/reset" {
                // KNOWN GAP (README.md, docs/architecture.md): the reset mail
                // links to a page the template doesn't have yet. Once a
                // `/reset` page exists, this fails: delete this branch and
                // the known gap.
                assert_eq!(status, 404, "/reset now answers: remove the known gap");
            } else {
                assert_eq!(status, 200, "mail link {path} answers {status}");
            }
        }
    })
    .await;
}
