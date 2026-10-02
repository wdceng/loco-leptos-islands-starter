//! `/llms.txt`: the site described for language models (llmstxt.org): a
//! title, one sentence, and links to the pages that matter, in Markdown.
//! A route rather than a file under `public/`, like `robots.txt`, so it's
//! never cached for a year and the links follow the environment's host.
//!
//! Add a line under "Pages" for every public page you add; leave out pages
//! a visitor can't use without logging in.

use axum::{http::header::CONTENT_TYPE, response::IntoResponse};
use loco_rs::prelude::*;

use crate::{controllers::home::DESCRIPTION, views::layout::APP_NAME};

/// The Markdown for a site whose public origin is `host`.
#[must_use]
pub fn body_for(host: &str) -> String {
    let host = host.trim_end_matches('/');
    format!(
        "# {APP_NAME}\n\n\
         > {DESCRIPTION}\n\n\
         ## Pages\n\n\
         - [Home]({host}/): what the site is\n"
    )
}

#[debug_handler]
async fn llms(State(ctx): State<AppContext>) -> Result<Response> {
    let body = body_for(&ctx.config.server.host);
    Ok(([(CONTENT_TYPE, "text/markdown; charset=utf-8")], body).into_response())
}

pub fn routes() -> Routes {
    Routes::new().add("/llms.txt", get(llms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_start_with_the_host_without_a_double_slash() {
        let body = body_for("https://example.com/");
        assert!(body.starts_with(&format!("# {APP_NAME}\n")), "{body}");
        assert!(body.contains(&format!("> {DESCRIPTION}")), "{body}");
        assert!(body.contains("(https://example.com/)"), "{body}");
        assert!(!body.contains("example.com//"), "{body}");
    }
}
