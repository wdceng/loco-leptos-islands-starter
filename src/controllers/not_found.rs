//! The router's fallback: files from the site folder, and a real 404 for
//! everything else.
//!
//! Loco's `static` middleware serves the site folder as the router's
//! fallback and answers a miss with the static `404.html`, status 200,
//! which a search engine reads as a page and production would cache for a
//! year. [`Site`] takes its place in the middleware stack with the same
//! file serving and cache header plus a miss that renders `NotFoundPage`
//! with status 404 and `Cache-Control: no-store`. It is a Loco middleware,
//! first in the stack, so every middleware after it wraps the fallback too:
//! files and misses get the security headers, a request id, a line in the
//! request log, the request timeout and panic catching, like routes. The
//! `static` block in the config stays the source of the folder, the cache
//! header and the boot-time existence checks, which is why `public/404.html`
//! still exists: the check wants it there, even though nothing serves it.
//!
//! A miss renders the page only for a request that asks for HTML. A
//! scanner probing `/wp-login.php` and friends gets one line of text, so a
//! probe costs a string, not a page render. Misses are also limited per
//! visitor IP with the site-wide numbers, in a bucket of their own
//! (`rate_limit::site_bucket`): the site-wide limiter is a `route_layer`
//! and never sees a miss, so without this one machine could ask for
//! made-up addresses as fast as it liked, every one answered. The files
//! have a generous bucket of their own where `settings.file_rate_limit`
//! switches it on (`rate_limit::files_bucket`): staging and production. One
//! page load fetches several files, so no person reaches it, only a script
//! pulling the same files over and over.
//!
//! The folder is always served at the root, which is what every config
//! has (`static.folder.uri: "/"`, pinned by `tests/config.rs`).

use axum::{
    Router,
    extract::Request,
    http::{
        HeaderValue, StatusCode,
        header::{ACCEPT, CACHE_CONTROL},
    },
    response::IntoResponse,
};
// Named import on purpose: `leptos::prelude::*` also exports an `Error`
// type that would shadow Loco's.
use leptos::view;
use loco_rs::{controller::middleware::MiddlewareLayer, prelude::*};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

use crate::{
    middleware::rate_limit,
    render::render_page,
    views::{
        layout::{APP_NAME, PageMeta},
        not_found::NotFoundPage,
    },
};

/// A miss: the page for a browser, one line for anything else, never cached.
async fn miss(State(ctx): State<AppContext>, req: Request) -> Result<Response> {
    let wants_html = req
        .headers()
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("text/html"));
    if !wants_html {
        return Ok((
            StatusCode::NOT_FOUND,
            [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            "Not Found",
        )
            .into_response());
    }
    let meta = PageMeta {
        lang: "en",
        title: format!("Page not found | {APP_NAME}"),
        description: "The page you asked for does not exist.".into(),
    };
    let mut res = render_page(&ctx, req, meta, || view! { <NotFoundPage/> }).await?;
    *res.status_mut() = StatusCode::NOT_FOUND;
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

/// The miss handler behind its own bucket with the site-wide numbers
/// (`settings.rate_limit`), and unlimited when that block is switched off,
/// like the routes. `Router::layer`, unlike `route_layer`, wraps the
/// fallback too.
fn limited_miss(ctx: &AppContext) -> Result<Router> {
    let miss = Router::<AppContext>::new().fallback(miss);
    let miss = match rate_limit::site_bucket(ctx)? {
        Some(bucket) => miss.layer(bucket),
        None => miss,
    };
    Ok(miss.with_state(ctx.clone()))
}

/// The fallback itself: the files, and the miss handler behind its bucket.
/// Without a `static` block in the config (the test environment) only the
/// miss handler.
///
/// # Errors
/// The site folder or its fallback file is missing where `must_exist` asks
/// for them (Loco's own check, with Loco's own message, which the deploy
/// docs quote), `static.cache_control` is not a valid header value, the
/// settings are missing from the shared store, or the limiter cannot be
/// built.
fn site(ctx: &AppContext) -> Result<Router> {
    let miss = limited_miss(ctx)?;
    let Some(assets) = ctx
        .config
        .server
        .middlewares
        .static_assets
        .as_ref()
        .filter(|assets| assets.enable)
    else {
        return Ok(Router::new().fallback_service(miss));
    };
    if assets.must_exist && (!assets.folder.path.exists() || !assets.fallback.exists()) {
        return Err(Error::Message(format!(
            "one of the static path are not found, Folder `{}` fallback: `{}`",
            assets.folder.path.display(),
            assets.fallback.display(),
        )));
    }
    let files = ServeDir::new(&assets.folder.path).fallback(miss);
    let files = if assets.precompressed {
        files.precompressed_gzip()
    } else {
        files
    };
    let mut site = Router::new().fallback_service(files);
    if let Some(cache_control) = &assets.cache_control {
        // Loco falls back to a year on a value that is not a header; this
        // refuses the boot instead.
        let value = HeaderValue::from_str(cache_control).map_err(|e| {
            Error::Message(format!(
                "static.cache_control is not a valid header value: {e}"
            ))
        })?;
        // `if_not_present`: a file carries no Cache-Control of its own and
        // gets the configured one; the miss handler sets `no-store` itself.
        site = site.layer(SetResponseHeaderLayer::if_not_present(CACHE_CONTROL, value));
    }
    // The files behind a generous bucket of their own
    // (`settings.file_rate_limit`, off where the files are rechecked on
    // every view), outside the cache header, so its 429 keeps `no-store`.
    // A miss passes through here on its way to the handler above and
    // spends a token here as well as one of its own.
    if let Some(limit) = rate_limit::files_bucket(ctx)? {
        site = site.layer(limit);
    }
    Ok(site)
}

/// The router's fallback as a Loco middleware, see the module doc.
/// `Hooks::middlewares` (app.rs) deletes Loco's own `static` entry and puts
/// this one first: the stack is applied in order and every middleware
/// wraps what the router holds by then, its fallback included.
pub struct Site {
    ctx: AppContext,
}

impl Site {
    #[must_use]
    pub fn from_context(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
}

impl MiddlewareLayer for Site {
    fn name(&self) -> &'static str {
        "site"
    }

    /// What `cargo loco middleware -c` prints: the `static` block the
    /// files are served from.
    fn config(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::to_value(&self.ctx.config.server.middlewares.static_assets)
    }

    fn apply(&self, app: Router<AppContext>) -> Result<Router<AppContext>> {
        Ok(app.fallback_service(site(&self.ctx)?))
    }
}
