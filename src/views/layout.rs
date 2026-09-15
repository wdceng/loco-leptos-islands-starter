//! The HTML document every page is rendered into.
//!
//! Loco controllers own the routes; each one renders a page component inside
//! this shell. The shell is plain server-rendered HTML except for the two
//! Leptos helpers in `<head>`: the hydration scripts that load the islands
//! bundle, and the dev-only auto-reload hook.

use chrono::Datelike;
use leptos::prelude::*;

/// Shown in the header, the footer and the browser tab. One place to
/// change when the app gets its real name.
pub const APP_NAME: &str = "SaaS Starter";

/// The centred prose column shared by the landmarks: capped width, centred
/// by auto margins, side padding for narrow screens. Defined once here;
/// Tailwind's scanner picks the class names up from this string.
const COLUMN: &str = "mx-auto max-w-3xl px-6";

/// Per-page values that go straight into `<head>`. The controller knows
/// the language and title of the page it is serving, so they are passed in
/// as plain data rather than collected from the component tree.
#[derive(Clone, Debug)]
pub struct PageMeta {
    /// BCP 47 tag for `<html lang>`.
    pub lang: &'static str,
    pub title: String,
    pub description: String,
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
                // viewport-fit=cover lets the page extend under a phone's
                // notch and rounded corners; the safe-area padding on <body>
                // (style/tailwind.css) keeps content out of them.
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
                <title>{meta.title}</title>
                <meta name="description" content=meta.description/>
                // Body font fetched alongside the stylesheet, not after it:
                // no flash of fallback font on first visit. `as` and `type`
                // are Rust keywords, hence the r# prefix.
                <link rel="preload" href="/fonts/Inter-Regular.woff2" r#as="font" r#type="font/woff2" crossorigin="anonymous"/>
                // Icons live in public/favicon, which cargo-leptos copies to
                // target/site/favicon. Browsers also probe /favicon.ico on
                // their own, so that one is linked explicitly.
                <link rel="icon" href="/favicon/favicon.ico" sizes="any"/>
                <link rel="icon" r#type="image/png" sizes="32x32" href="/favicon/favicon-32x32.png"/>
                <link rel="icon" r#type="image/png" sizes="16x16" href="/favicon/favicon-16x16.png"/>
                <link rel="apple-touch-icon" sizes="180x180" href="/favicon/apple-touch-icon.png"/>
                <link rel="manifest" href="/favicon/site.webmanifest"/>
                // Tints the browser's own bars (Safari, Chrome on Android).
                // The page's surface colour, so the bars blend with the
                // page instead of framing it in brand blue; the manifest
                // keeps the blue theme_color for the installed app.
                <meta name="theme-color" content="#f1f3f5"/>
                // iOS home-screen install: full-screen launch and the label
                // under the icon. Android takes both from the manifest.
                <meta name="apple-mobile-web-app-capable" content="yes"/>
                <meta name="apple-mobile-web-app-title" content=APP_NAME/>
                <link rel="stylesheet" href=stylesheet/>
                // Live reload while `cargo leptos watch` runs; renders
                // nothing in production.
                <AutoReload options=options.clone()/>
                // Loads the WASM bundle and calls our `hydrate()`, which
                // wakes up only the `#[island]` components on the page.
                <HydrationScripts options islands=true/>
            </head>
            <body class="min-h-dvh bg-surface text-ink">
                // The landmarks live here, once. Pages supply only what goes
                // inside <main>. All three share the prose column for now;
                // header and footer can take a wider frame later.
                <header class=format!("{COLUMN} py-6")>
                    <a href="/" class="font-semibold">{APP_NAME}</a>
                </header>
                <main class=format!("{COLUMN} py-16")>
                    {page}
                </main>
                <footer class=format!("{COLUMN} py-6 text-sm text-ink-muted")>
                    {format!("© {year} {APP_NAME}")}
                </footer>
            </body>
        </html>
    }
}
