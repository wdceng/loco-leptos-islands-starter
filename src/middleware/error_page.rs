//! A Leptos error page for a browser, in place of Loco's raw error answers.
//!
//! Left alone, a browser asking for a page could get a 500 as JSON
//! (`{"error":"internal_server_error"}`), a 405 or a 408 with an empty body
//! (which some browsers save as an empty download), or a 413 or a bad form
//! as a line of JSON. [`ErrorPages`] looks at every answer on its way out:
//! when the request accepts HTML, isn't for the JSON API (`/api/`), and the
//! answer is an error without an HTML body of its own, it renders
//! `ErrorPage` inside the shell instead, with the original status, the
//! original headers (`Allow` on a 405, for one), the page CSP, and
//! `Cache-Control: no-store`.
//!
//! The 404 page and the 429 page are already HTML and pass through. API
//! callers, scanners and anything else that doesn't ask for HTML keep the
//! original answer, which costs nothing to build. If the page itself can't
//! be rendered, the original answer goes out and the reason is logged.
//!
//! In the stack it sits right after `timeout_request` (src/app.rs): outside
//! the timeout, the payload limit and panic catching, so it sees their
//! answers too, and inside `secure_headers` and the logger, so the page
//! gets the same headers and log line as any other.

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode, Uri,
        header::{ACCEPT, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::Response,
};
// Named import on purpose: `leptos::prelude::*` also exports an `Error`
// type that would shadow Loco's.
use leptos::view;
use loco_rs::{Result, app::AppContext, controller::middleware::MiddlewareLayer};

use crate::{
    render::render_page,
    views::{
        error::{ErrorPage, wording},
        layout::{APP_NAME, PageMeta},
    },
};

/// The layer, see the module doc. Added in `Hooks::middlewares` (app.rs).
pub struct ErrorPages {
    ctx: AppContext,
}

impl ErrorPages {
    #[must_use]
    pub fn from_context(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
}

impl MiddlewareLayer for ErrorPages {
    fn name(&self) -> &'static str {
        "error_page"
    }

    /// What `cargo loco middleware -c` prints.
    fn config(&self) -> serde_json::Result<serde_json::Value> {
        Ok(serde_json::json!({ "for": "requests that accept text/html", "except": "/api/" }))
    }

    fn apply(&self, app: Router<AppContext>) -> Result<Router<AppContext>> {
        Ok(app.layer(middleware::from_fn_with_state(self.ctx.clone(), swap)))
    }
}

/// A browser asking for a page: HTML accepted, and not the JSON API.
fn wants_page(path: &str, headers: &HeaderMap) -> bool {
    !path.starts_with("/api/")
        && headers
            .get(ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
}

/// An error answer that isn't a page already.
fn needs_page(res: &Response) -> bool {
    let status = res.status();
    let is_html = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("text/html"));
    (status.is_client_error() || status.is_server_error()) && !is_html
}

async fn swap(State(ctx): State<AppContext>, req: Request, next: Next) -> Response {
    if !wants_page(req.uri().path(), req.headers()) {
        return next.run(req).await;
    }
    let (uri, headers) = (req.uri().clone(), req.headers().clone());
    let res = next.run(req).await;
    if !needs_page(&res) {
        return res;
    }
    let status = res.status();
    match page(&ctx, status, uri, headers).await {
        Ok(page) => merge(res, page),
        Err(err) => {
            tracing::error!(error = %err, %status, "error page could not be rendered, sent the original answer");
            res
        }
    }
}

/// The page for `status`, rendered for a fresh GET of the same address.
async fn page(
    ctx: &AppContext,
    status: StatusCode,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response> {
    let mut req = Request::new(Body::empty());
    *req.uri_mut() = uri;
    *req.headers_mut() = headers;
    let (heading, lead) = wording(status);
    let meta = PageMeta {
        lang: "en",
        title: format!("{heading} | {APP_NAME}"),
        description: lead.into(),
        robots: Some("noindex"),
    };
    render_page(ctx, req, meta, move || view! { <ErrorPage heading lead /> }).await
}

/// The page with the original status, plus every header of the original
/// answer the page doesn't set itself (`Allow` on a 405, `Retry-After`),
/// never cached.
fn merge(original: Response, mut page: Response) -> Response {
    *page.status_mut() = original.status();
    let own: Vec<HeaderName> = page.headers().keys().cloned().collect();
    for (name, value) in original.headers() {
        if name != CONTENT_TYPE && name != CONTENT_LENGTH && !own.contains(name) {
            page.headers_mut().append(name.clone(), value.clone());
        }
    }
    page.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    page
}

#[cfg(test)]
mod tests {
    use axum::http::header::{ALLOW, CONTENT_SECURITY_POLICY};

    use super::*;

    fn headers(accept: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_str(accept).expect("header"));
        headers
    }

    #[test]
    fn only_a_browser_asking_for_a_page_gets_one() {
        let browser = "text/html,application/xhtml+xml,*/*;q=0.8";
        assert!(wants_page("/", &headers(browser)));
        assert!(!wants_page("/api/auth/login", &headers(browser)));
        assert!(!wants_page("/", &headers("application/json")));
        assert!(!wants_page("/", &HeaderMap::new()));
    }

    #[test]
    fn only_errors_without_html_are_replaced() {
        let answer = |status: StatusCode, content_type: Option<&'static str>| {
            let mut res = Response::new(Body::empty());
            *res.status_mut() = status;
            if let Some(content_type) = content_type {
                res.headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
            }
            res
        };
        assert!(needs_page(&answer(StatusCode::METHOD_NOT_ALLOWED, None)));
        assert!(needs_page(&answer(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some("application/json")
        )));
        assert!(!needs_page(&answer(
            StatusCode::NOT_FOUND,
            Some("text/html; charset=utf-8")
        )));
        assert!(!needs_page(&answer(StatusCode::OK, None)));
        assert!(!needs_page(&answer(StatusCode::SEE_OTHER, None)));
    }

    #[test]
    fn the_page_keeps_the_status_and_the_original_headers() {
        let mut original = Response::new(Body::empty());
        *original.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        original
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET,HEAD"));
        original
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut page = Response::new(Body::empty());
        page.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        page.headers_mut().insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'"),
        );

        let merged = merge(original, page);
        assert_eq!(merged.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(merged.headers()[ALLOW], "GET,HEAD");
        assert_eq!(merged.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(
            merged.headers()[CONTENT_SECURITY_POLICY],
            "default-src 'none'"
        );
        assert_eq!(merged.headers()[CACHE_CONTROL], "no-store");
    }
}
