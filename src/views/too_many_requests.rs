//! The 429 page, rendered by Leptos once per limiter at boot
//! (`middleware/rate_limit.rs`). Each refusal only replaces [`WAIT`] with
//! the wait in words, so a refusal costs a string replace, not a render.
//!
//! A document of its own rather than the shell: the shell carries the
//! hydration scripts, whose inline script needs the page CSP's nonce, and a
//! 429 goes out under the fallback CSP (`default-src 'none'`, no scripts).
//! The title, the heading, lead and link are the same `full_title` and
//! `Notice` every other error page uses, and the body and column classes
//! are the shell's constants, so nothing here is copied by hand.

use leptos::prelude::*;

use super::layout::{BODY, COLUMN, Notice, full_title};

/// The page's heading and its name in the title.
const HEADING: &str = "Too many requests";

/// Stands in for the wait in the rendered page, replaced per refusal.
pub const WAIT: &str = "{wait}";

const LEAD: &str = "You have sent too many requests in a short time. Please try again in {wait}.";

/// The page with `stylesheet` linked (the path `Assets` resolved at boot,
/// hashed in production) and [`WAIT`] still in it.
#[must_use]
pub fn document(stylesheet: String) -> String {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>{full_title(HEADING)}</title>
                <meta name="robots" content="noindex" />
                <link rel="stylesheet" href=stylesheet />
            </head>
            <body class=BODY>
                <main class=format!("{COLUMN} py-16")>
                    <Notice heading=HEADING lead=LEAD />
                </main>
            </body>
        </html>
    }
    .to_html()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_links_the_stylesheet_and_keeps_the_wait_placeholder() {
        let page = document("/pkg/app.0123abcd.css".into());
        assert!(page.starts_with("<!DOCTYPE html>"), "{page}");
        assert!(page.contains(r#"href="/pkg/app.0123abcd.css""#), "{page}");
        assert!(
            page.contains(&format!("Please try again in {WAIT}.")),
            "{page}"
        );
        assert!(page.contains("<h1"), "{page}");
        assert!(
            page.contains(&format!("<title>{}</title>", full_title(HEADING))),
            "{page}"
        );
        // No script of any kind: the fallback CSP would block it.
        assert!(!page.contains("<script"), "{page}");
        assert!(!page.contains("<!>"), "{page}");
    }
}
