//! No element whose click sends a request flips Alpine's state: it sets the
//! state the page it asks for will draw (`open = true`, never
//! `open = !open`). A mouse's press preloads the page (`PRELOAD`,
//! src/views/layout.rs), so the answer can be there before the click, and
//! htmx can swap it in before Alpine's click runs, in either order by
//! browser: a flip then turns back what the new page drew. Found in an
//! islands app on 2026-10-21 (Safari: a category chosen, its fold closed).
//!
//! The starter's pages have no click handlers yet; the test reads every
//! page kind, so an app's first one is checked the day it's written.

use app::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// The click handlers in `body` that flip a state on an element that sends
/// a request: each start tag's `x-on:click`, an assignment whose value
/// reads its own name negated (`open = !open`, `open = hovered ? true :
/// !open`), on a tag that asks for something (one of htmx's methods, a
/// link, a form's submit button). Each one found, cut to 200 characters.
fn flips_on_a_request(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for tag in body.split('<').skip(1) {
        let tag = tag.split('>').next().unwrap_or_default();
        let Some(click) = tag
            .split(r#"x-on:click=""#)
            .nth(1)
            .and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        let Some((name, value)) = click.split_once(" = ") else {
            continue;
        };
        let flips = value.contains(&format!("!{}", name.trim()));
        let asks = ["hx-get=", "hx-post=", "hx-put=", "hx-patch=", "hx-delete="]
            .iter()
            .any(|method| tag.contains(method))
            || (tag.starts_with("a ") && tag.contains("href="))
            || (tag.starts_with("button ") && tag.contains(r#"type="submit""#));
        if flips && asks {
            found.push(tag.chars().take(200).collect());
        }
    }
    found
}

#[tokio::test]
#[serial]
async fn no_click_that_asks_for_a_page_flips_alpines_state() {
    request::<App, _, _>(|request, _ctx| async move {
        let mut found = Vec::new();
        for path in ["/", "/reset?token=x", "/no-such-page"] {
            let res = request.get(path).add_header("accept", "text/html").await;
            let body = res.text();
            assert!(body.contains("<main"), "{path}: not a page");
            found.extend(
                flips_on_a_request(&body)
                    .into_iter()
                    .map(|tag| format!("{path}: <{tag}")),
            );
        }
        assert!(
            found.is_empty(),
            "a click that asks for a page flips Alpine's state; set the state the page \
             draws instead:\n{}",
            found.join("\n")
        );
    })
    .await;
}

/// The check finds a flip on a link, on an htmx request and on a form's
/// button, and lets a set state and a flip on a plain button be.
#[test]
fn a_flip_on_a_request_is_found_and_a_set_state_is_not() {
    let flagged = [
        r#"<a href="/?category=x" x-on:click="open = !open">"#,
        r#"<div hx-get="/x" x-on:click="open = hovered ? true : !open">"#,
        r#"<button type="submit" x-on:click="shown = !shown">"#,
    ];
    for tag in flagged {
        assert_eq!(flips_on_a_request(tag).len(), 1, "{tag}");
    }
    let let_be = [
        r#"<a href="/?category=x" x-on:click="open = true">"#,
        r#"<button type="button" x-on:click="open = !open">"#,
        r#"<a href="/x" x-on:click="open = open &amp;&amp; !$el.href.startsWith($el.baseURI)">"#,
    ];
    for tag in let_be {
        assert!(flips_on_a_request(tag).is_empty(), "{tag}");
    }
}
