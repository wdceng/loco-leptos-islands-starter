//! The HTML document every page is rendered into.
//!
//! Loco controllers own the routes; each one renders a page component inside
//! this shell. The shell is plain server-rendered HTML except for the two
//! Leptos helpers in `<head>`: the hydration scripts that load the islands
//! bundle, and the dev-only auto-reload hook.

use chrono::Datelike;
use leptos::prelude::*;

use crate::paths;

/// Shown in the header, the footer, the browser tab and under the icon on a
/// phone's home screen (the web manifest). One place to change when the
/// app gets its real name.
pub const APP_NAME: &str = "SaaS Starter";

/// The page's surface colour as hex: `--color-surface` in
/// `style/tailwind.css` (Tailwind's white). Used where CSS can't reach: the
/// `theme-color` tag and the manifest's `background_color`. Change it
/// together with the stylesheet.
pub const SURFACE_HEX: &str = "#ffffff";

/// The primary colour as hex: `--color-primary` in `style/tailwind.css`
/// (Tailwind's blue-600, `oklch(54.6% 0.245 262.881)`). The manifest's
/// `theme_color`, the bars of the installed app. Change it together with
/// the stylesheet.
pub const BRAND_HEX: &str = "#155dfc";

/// The centred prose column shared by the landmarks: capped width, centred
/// by auto margins, side padding for narrow screens. Defined once here;
/// Tailwind's scanner picks the class names up from this string. The 429
/// page (views/too_many_requests.rs) uses it too.
pub const COLUMN: &str = "mx-auto max-w-3xl px-6";

/// The <body> classes, shared with the 429 page.
pub const BODY: &str = "min-h-dvh text-ink";

/// Per-page values that go straight into `<head>`. The controller knows
/// the language and title of the page it is serving, so they are passed in
/// as plain data rather than collected from the component tree.
#[derive(Clone, Debug)]
pub struct PageMeta {
    /// BCP 47 tag for `<html lang>`.
    pub lang: &'static str,
    pub title: String,
    pub description: String,
    /// A `<meta name="robots">` value: `Some("noindex")` for pages that have
    /// no business in search results (the not-found page, the error page).
    /// `None` renders no tag: index and follow.
    pub robots: Option<&'static str>,
}

/// Wraps `page` in the full document. `options` comes from
/// `[package.metadata.leptos]` (or the env vars cargo-leptos sets) and
/// tells the hydration scripts where the bundle lives. `stylesheet` is the
/// URL path resolved at boot (`src/assets.rs`): hashed in a release build,
/// plain otherwise.
pub fn shell(
    options: LeptosOptions,
    stylesheet: String,
    meta: PageMeta,
    page: impl IntoView,
) -> impl IntoView {
    // Rendered per request on the server, so the footer never goes stale.
    // UTC rather than the server's local zone: deterministic wherever it runs.
    let year = chrono::Utc::now().year();

    view! {
        <!DOCTYPE html>
        <html lang=meta.lang>
            <head>
                <meta charset="utf-8"/>
                // No viewport-fit=cover: iOS keeps the page out of the notch
                // and the home bar by itself, also right after a rotation.
                // With cover, the page would need env(safe-area-inset-*)
                // padding, which WebKit sometimes updates only on the next
                // scroll after rotating, leaving the page off-centre.
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>{meta.title}</title>
                <meta name="description" content=meta.description/>
                {meta.robots.map(|content| view! { <meta name="robots" content=content/> })}
                // Icons live in public/favicon, which cargo-leptos copies to
                // target/site/favicon. Browsers also probe /favicon.ico on
                // their own, so that one is linked explicitly. Each address
                // carries `?v=<hash>` of its file (src/paths.rs, build.rs),
                // so a changed icon gets past the year-long cache.
                <link rel="icon" href=paths::FAVICON sizes="any"/>
                <link rel="icon" r#type="image/png" sizes="32x32" href=paths::FAVICON_32/>
                <link rel="icon" r#type="image/png" sizes="16x16" href=paths::FAVICON_16/>
                <link rel="apple-touch-icon" sizes="180x180" href=paths::APPLE_TOUCH_ICON/>
                // `use-credentials`: a browser fetches the manifest without
                // cookies unless its link asks for them. Behind a Cloudflare
                // challenge (staging) that cookie-less fetch gets the
                // challenge page instead, which Chrome reports as a manifest
                // syntax error. Seen on wdc's staging, same setup as ours.
                <link rel="manifest" href=paths::MANIFEST crossorigin="use-credentials"/>
                // Tints the browser's own bars (Safari, Chrome on Android)
                // in the page's surface colour (SURFACE_HEX), so the bars
                // blend with the page; the manifest keeps the primary colour
                // as theme_color for the installed app. `type` is a Rust
                // keyword, hence `r#type` on the icon links above.
                <meta name="theme-color" content=SURFACE_HEX/>
                // Home-screen install, full-screen launch: the manifest's
                // `display: standalone` (Android, and iOS since 11.3) and
                // Chrome's current tag. Apple's `apple-mobile-web-app-capable`
                // is left out: Chrome reports it as deprecated, and iOS needs
                // it only for startup images, which this template does not
                // have. The label under the icon on an iPhone is the Apple
                // tag below; Android takes the manifest's.
                <meta name="mobile-web-app-capable" content="yes"/>
                <meta name="apple-mobile-web-app-title" content=APP_NAME/>
                <link rel="stylesheet" href=stylesheet/>
                // Live reload while `cargo leptos watch` runs; renders
                // nothing in production.
                <AutoReload options=options.clone()/>
                // Loads the WASM bundle and calls our `hydrate()`, which
                // wakes up only the `#[island]` components on the page.
                <HydrationScripts options islands=true/>
            </head>
            <body class=BODY>
                // First focusable element: keyboard and screen-reader users
                // jump past the navigation. Invisible until it has focus.
                <a
                    href="#content"
                    class="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-20 focus:rounded-md focus:bg-primary focus:px-4 focus:py-2 focus:text-white"
                >
                    "Skip to content"
                </a>
                // The landmarks live here, once. Pages supply only what goes
                // inside <main>. All three share the prose column for now;
                // header and footer can take a wider frame later.
                <header class=format!("{COLUMN} py-6")>
                    <a href="/" class="font-semibold">{APP_NAME}</a>
                </header>
                <main id="content" class=format!("{COLUMN} py-16")>
                    {page}
                </main>
                <footer class=format!("{COLUMN} py-6 text-sm text-ink-muted")>
                    {format!("© {year} {APP_NAME}")}
                </footer>
            </body>
        </html>
    }
}
