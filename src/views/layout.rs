//! The HTML document every page is rendered into.
//!
//! Loco controllers own the routes; each one renders a page component inside
//! this shell. The shell is server-rendered HTML with htmx and Alpine in its
//! `<head>` ([`Scripts`]), and the two Leptos helpers: the hydration scripts
//! that load the islands bundle, and the dev-only auto-reload hook.
//!
//! Moves between pages happen in place: every link and form in the page's
//! region (`#page`) is boosted, so htmx fetches the next page and swaps its
//! region in, and the `Navigation` island (src/islands.rs) does what a
//! swap needs beyond that. Without htmx (blocked, not loaded yet) every
//! link and form still works as a plain one.

use chrono::Datelike;
use leptos::{
    attr::{
        any_attribute::{AnyAttribute, IntoAnyAttribute},
        custom::custom_attribute,
    },
    prelude::*,
};

use crate::{
    assets::Assets,
    islands::{BUILD_META, Navigation, PAGE_ANNOUNCE, PAGE_REGION, PAGE_STATUSES},
    paths::{
        self,
        assets::{js, vendor},
    },
};

/// htmx's settings, its `htmx-config` meta: no indicator styles of its own
/// (they'd be inline, which the CSP refuses); no swap of a 4xx or 5xx
/// unless the element names the status (the region names every status the
/// app answers with a page, [`PAGE_STATUSES`]); the Alpine and preload
/// extensions, the preload's own boost mode off (it would fetch a page for
/// every scroll a phone starts on a link); Back and Forward in place
/// (`history`), htmx fetching the earlier page again into the region
/// (`hx-history-elt`); a morph leaves islands alone; swaps go in at once, no
/// view transition, and no settle: htmx's default gives each new element
/// whose `id` matches an old one the old one's attributes for a frame, so
/// a menu with the same `id` on every page would arrive open and fade shut
/// over the new page (`defaultSettleDelay`; found in an islands app).
pub const HTMX_CONFIG: &str = r#"{"includeIndicatorCSS":false,"noSwap":[204,304,"4xx","5xx"],"extensions":"alpine-compat,preload","preload":{"autoBoost":false},"history":true,"morphSkip":"[hx-morph-skip], leptos-island","transitions":false,"defaultSettleDelay":0}"#;

/// When a page a link leads to starts loading: at a mouse's press, the
/// click a moment later taking the answer already on its way (100 to 150 ms
/// saved per move). Never at a hover, which would fetch every link the
/// pointer crosses, nor at a touch, which starts most scrolls on a phone.
/// A POST is never fetched ahead.
pub const PRELOAD: &str = "mousedown";

/// How a move goes in: the answer's region in place of the page's own, the
/// page at its top. Carried by `hx-boost` it applies to boosted links and
/// forms alone, so an element with its own `hx-*` keeps its own target;
/// carried by a status, an error page from any request in the region shows
/// in the region.
const TO_REGION: &str =
    r##"{"target":"#page","select":"#page","swap":"outerHTML show:top showTarget:body"}"##;

/// The page's region's attributes, the part a move replaces: every link and
/// form in it boosted ([`TO_REGION`]), a GET started at a press
/// ([`PRELOAD`]), and every status the app answers with a page of its own
/// shown in it ([`PAGE_STATUSES`]), so a link to a missing page shows the
/// not-found page instead of doing nothing. htmx 4 inherits only what says
/// `:inherited`.
fn region_attributes() -> Vec<AnyAttribute> {
    [
        custom_attribute("hx-boost:inherited", TO_REGION).into_any_attr(),
        custom_attribute("hx-preload:inherited", PRELOAD).into_any_attr(),
    ]
    .into_iter()
    .chain(PAGE_STATUSES.iter().map(|status| {
        custom_attribute(format!("hx-status:{status}:inherited"), TO_REGION).into_any_attr()
    }))
    .collect()
}

/// `text` escaped for a `<title>`, which the shell writes as raw content
/// (`inner_html`): the parser ends a title at `</title>` and decodes
/// character references in it, so `&`, `<` and `>` are written as
/// references and show exactly as typed.
fn title_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// htmx's settings ([`HTMX_CONFIG`]), htmx, its Alpine and preload
/// extensions, the glue that wakes islands htmx swaps in
/// (`public/js/htmx-islands.js`) and Alpine's CSP build: all `defer`, so
/// they run in the order they stand, the extensions and the glue after
/// htmx, Alpine last. Files from `script-src 'self'`, so no nonce. Alpine's
/// CSP build reads its attributes without `eval`, which the CSP refuses.
#[component]
fn Scripts() -> impl IntoView {
    view! {
        <meta name="htmx-config" content=HTMX_CONFIG/>
        <script src=vendor::HTMX_4_0_0_MIN_JS defer></script>
        <script src=vendor::HX_ALPINE_COMPAT_4_0_0_MIN_JS defer></script>
        <script src=vendor::HX_PRELOAD_4_0_0_MIN_JS defer></script>
        <script src=js::HTMX_ISLANDS_JS defer></script>
        <script src=vendor::ALPINEJS_CSP_3_17_4_MIN_JS defer></script>
    }
}

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
/// tells the hydration scripts where the bundle lives. `assets` was resolved
/// at boot (`src/assets.rs`): the stylesheet's address, hashed in a release
/// build, and the build's name.
pub fn shell(
    options: LeptosOptions,
    assets: Assets,
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
                // The title as the element's own escaped text, never a child
                // view: a title holds characters alone, so the marker a dev
                // build's erased view leaves after its content (`<!>`)
                // showed in every tab, and htmx copies it on every swap.
                <title inner_html=title_text(&meta.title)></title>
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
                <link rel="stylesheet" href=assets.stylesheet/>
                // The build this page loaded, which a move's answer is
                // compared with (`X-Build`, the `Navigation` island):
                // another after a deploy, and the move is a full load.
                <meta name=BUILD_META content=assets.build/>
                // Live reload while `cargo leptos watch` runs; renders
                // nothing in production.
                <AutoReload options=options.clone()/>
                <Scripts/>
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
                //
                // The page's region: what a move replaces, and where Back
                // and Forward put the earlier page (`hx-history-elt`). Out of
                // the layout (`contents`), so the body still lays out the
                // header, `<main>` and the footer. `<main>` takes the focus
                // after a move (`Navigation`), out of the Tab order.
                <div id=PAGE_REGION class="contents" hx-history-elt="" {..region_attributes()}>
                    <header class=format!("{COLUMN} py-6")>
                        <a href="/" class="font-semibold">{APP_NAME}</a>
                    </header>
                    <main id="content" tabindex="-1" class=format!("{COLUMN} py-16 outline-hidden")>
                        {page}
                    </main>
                    <footer class=format!("{COLUMN} py-6 text-sm text-ink-muted")>
                        {format!("© {year} {APP_NAME}")}
                    </footer>
                </div>
                // The new page's title, read out after a move in place, which
                // a screen reader doesn't announce as a page load.
                <p id=PAGE_ANNOUNCE role="status" class="sr-only"></p>
                // What a move needs beyond htmx, for the whole visit: outside
                // the region, so a move never replaces it (src/islands.rs).
                <Navigation/>
            </body>
        </html>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_shows_exactly_as_written() {
        assert_eq!(title_text("Reset · App"), "Reset · App");
        assert_eq!(
            title_text("A & B </title><script>x</script>"),
            "A &amp; B &lt;/title&gt;&lt;script&gt;x&lt;/script&gt;"
        );
    }
}
