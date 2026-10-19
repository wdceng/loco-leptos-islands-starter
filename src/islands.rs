//! Islands: the only components that are compiled into the browser bundle
//! and hydrated there. Everything else in the app is server-rendered HTML.
//!
//! This module is compiled in both builds: it is the one module in `lib.rs`
//! without the `ssr` gate, so the server renders an island's HTML and the
//! browser half brings it to life. A component marked `#[island]` anywhere
//! else, in `views/` for instance, compiles for the server only and never
//! hydrates.
//!
//! Rules that keep the bundle small:
//! - Keep an island a thin wrapper and pass the real content in as
//!   `children`: children are rendered on the server only and never reach
//!   the wasm.
//! - Props are plain data (`String`, numbers, structs deriving `Serialize`
//!   and `Deserialize`); nothing server-only (database, config) inside.
//! - Check both halves: `cargo clippy --all-targets` and
//!   `cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate`.
//!
//! Leptos wraps an island in `<leptos-island>` and its children in
//! `<leptos-children>`; `style/tailwind.css` sets both to `display: contents`
//! so they do not disturb grid or flex layouts.
//!
//! Lazy islands (`#[island(lazy)]`) are split out of the main bundle into a
//! chunk of their own, fetched only by a page that renders one. That needs
//! `--split` on every cargo-leptos command; without it no island on any page
//! runs, and the server refuses to boot (`src/assets.rs`). Make an island
//! lazy when only some pages use it, and render it only where it is needed:
//! one rendered on every page is fetched on every page.
//!
//! Which tool for which job: htmx for whatever needs the server (a page
//! moved to in place, a form sent, a part fetched), Alpine's CSP build for
//! small state in the page (open or closed, focus, keys), an island only
//! for what neither can (Rust shared with the server, a browser API Alpine
//! can't reach, state that must outlive the server's answers).
//!
//! **Every island's body runs through `collected`** (src/collect.rs):
//! Leptos never frees an island htmx swaps out, so without it each one
//! stays in the tab's memory until the next full load. A test below refuses
//! an island that doesn't.

use std::{cell::RefCell, time::Duration};

use leptos::{
    prelude::*,
    wasm_bindgen::{JsCast, JsValue},
};
use web_sys::{
    ScrollBehavior, ScrollRestoration, ScrollToOptions,
    js_sys::{Function, Object, Reflect},
};

use crate::collect::collected;

/// The page's region, the part a move between pages replaces (the shell's
/// `#page`, views/layout.rs), by its id.
pub const PAGE_REGION: &str = "page";

/// The polite live region outside the page's region that reads the new
/// page's title after a move (views/layout.rs).
pub const PAGE_ANNOUNCE: &str = "page-announce";

/// The statuses the app answers with a page of its own, which a move in
/// place shows in the region: a request refused (400, 405, 413, 415), a
/// page not found (404), a slow answer (408), the server's own fault (500).
/// Anything else is loaded plainly ([`Navigation`]): the 429, a page
/// outside the shell, and a gateway's page in front of the app, a
/// restart's 502 above all.
pub const PAGE_STATUSES: [u16; 7] = [400, 404, 405, 408, 413, 415, 500];

/// The page's `<meta>` naming the build it loaded (views/layout.rs).
pub const BUILD_META: &str = "build";

/// The answer header naming the build it came from, which every page sends
/// (src/render.rs) and a move's answer is compared by: one name for both
/// halves.
pub const BUILD_HEADER: &str = "x-build";

/// The key a history entry's state keeps its page's scroll under, beside
/// htmx's own keys.
const SCROLL_KEY: &str = "pageScroll";

/// How long the page rests after a scroll before its entry keeps it: one
/// write when the scrolling stops, not one a frame (Safari refuses more
/// than a hundred history writes in thirty seconds).
const SCROLL_REST: Duration = Duration::from_millis(150);

thread_local! {
    /// Where the page Back or Forward is bringing back was scrolled, until
    /// htmx has swapped it in.
    static RESTORING: RefCell<Option<f64>> = const { RefCell::new(None) };
    /// The page's region as the last look found it: a different element
    /// after a swap is a move between pages.
    static REGION: RefCell<Option<web_sys::Element>> = const { RefCell::new(None) };
    /// The build this page loaded ([`BUILD_META`]): the pages move in place,
    /// so it is the build of the whole visit's code.
    static BUILD: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// What a move between pages in place needs that htmx doesn't do, on every
/// page, outside the region a move replaces (views/layout.rs). Ported from
/// the `Chores` island of the project this template shares its stack with,
/// where each part was found and proven by its browser tests:
///
/// - **The focus and the screen reader.** A swap is no page load to a
///   screen reader. Nothing keeps the focus into the swap, so htmx brings
///   none back; once the new region is in, the focus goes to `<main>` (or
///   stays on a field the new page focuses, `autofocus`), and the new title
///   is read out by a live region ([`PAGE_ANNOUNCE`]).
/// - **The scroll on Back and Forward.** htmx 4 swaps the earlier page back
///   with no scroll of its own, and the browser's restoration ran on the
///   page still showing. So the browser's is off, each history entry keeps
///   its page's scroll, and the page Back brings is scrolled there.
/// - **A deploy while a tab is open.** A move whose answer names a newer
///   build ([`BUILD_HEADER`]) loads plainly instead of swapping, so the next
///   page runs the new build's htmx, Alpine and islands, never an old wasm
///   against a new page's islands.
/// - **An answer the app never sends as a page** (a gateway's 502, the 429)
///   loads plainly, so the browser shows what came instead of a click doing
///   nothing.
///
/// The listeners are removed with the island's owner.
#[island]
pub fn Navigation() -> impl IntoView {
    collected(|| {
        // Effects run in the browser only, after hydration.
        Effect::new(move |_| {
            take_scroll_over();
            REGION.set(document().get_element_by_id(PAGE_REGION));
            BUILD.set(
                document()
                    .query_selector(&format!("meta[name={BUILD_META}]"))
                    .ok()
                    .flatten()
                    .and_then(|meta| meta.get_attribute("content")),
            );
        });
        let fresher = window_event_listener_untyped("htmx:before:response", |ev| {
            if let Some(page) = newer_build(&ev) {
                ev.prevent_default();
                let _ = window().location().assign(&page);
            }
        });
        let swapped = window_event_listener_untyped("htmx:after:swap", |_| {
            if moved() {
                arrive();
            }
        });
        // Back's swap keeps the region's element, so it is no move to
        // `moved`: the scroll is put back once htmx has settled the swap.
        let settled = window_event_listener_untyped("htmx:after:settle", |_| restored());
        // The entry Back or Forward brings back is the current one by now:
        // its scroll is read before htmx asks for its page.
        let restoring = window_event_listener_untyped("htmx:before:history:restore", |_| {
            RESTORING.set(Some(kept_scroll().unwrap_or(0.0)));
        });
        // The entry a move leaves keeps where its page stood, before the
        // move pushes the next one; not while a page is being brought back.
        let leaving = window_event_listener_untyped("htmx:before:request", |_| {
            if RESTORING.with_borrow(Option::is_none) {
                keep_scroll();
            }
        });
        let resting = StoredValue::new(None::<TimeoutHandle>);
        let scrolled = window_event_listener_untyped("scroll", move |_| {
            if let Some(queued) = resting.get_value() {
                queued.clear();
            }
            resting.set_value(set_timeout_with_handle(keep_scroll, SCROLL_REST).ok());
        });
        // htmx gives the focus back to the element with the same `id` in
        // the new page, and `arrive` then moves it to `<main>`: a field
        // styled on its focus (a search that opens) would open and close
        // over the new page. Nothing is focused before a move's swap; a
        // field the new page marks `autofocus` still gets the focus.
        let unfocused = window_event_listener_untyped("htmx:before:swap", |ev| {
            if into_region(&ev) {
                blur_focused();
            }
        });
        let refused = window_event_listener_untyped("htmx:response:error", load_plainly);
        on_cleanup(move || {
            fresher.remove();
            swapped.remove();
            settled.remove();
            restoring.remove();
            leaving.remove();
            scrolled.remove();
            unfocused.remove();
            refused.remove();
            if let Some(queued) = resting.get_value() {
                queued.clear();
            }
        });
    })
}

/// Whether a swap brought a new page's region: the region is another
/// element than the last look found. A part swapped inside the region
/// leaves it the same element.
fn moved() -> bool {
    let region = document().get_element_by_id(PAGE_REGION);
    let same = REGION.with_borrow(|last| match (last, &region) {
        (Some(last), Some(region)) => last.is_same_node(Some(region.as_ref())),
        _ => false,
    });
    REGION.set(region.clone());
    !same && region.is_some()
}

/// The page's scroll the page's own: the browser's restoration off, so
/// Back never scrolls the page still showing; and a page loaded whole on an
/// entry that kept its scroll (a reload) scrolled there, unless the visitor
/// has scrolled already.
fn take_scroll_over() {
    if let Ok(history) = window().history() {
        let _ = history.set_scroll_restoration(ScrollRestoration::Manual);
    }
    if let Some(top) = kept_scroll()
        && window().scroll_y().is_ok_and(|y| y == 0.0)
    {
        scroll_to(top);
    }
}

/// Where the current history entry's page was scrolled, if it kept it.
fn kept_scroll() -> Option<f64> {
    let state = window().history().ok()?.state().ok()?;
    Reflect::get(&state, &JsValue::from_str(SCROLL_KEY))
        .ok()?
        .as_f64()
}

/// The current entry keeps the page's scroll in its state, htmx's own keys
/// kept beside it (Back restores only the entries htmx marked). Best
/// effort: a browser that refuses the write keeps the entry as it was.
fn keep_scroll() {
    let (Ok(history), Ok(y)) = (window().history(), window().scroll_y()) else {
        return;
    };
    let kept = Object::new();
    if let Some(state) = history
        .state()
        .ok()
        .and_then(|state| state.dyn_into::<Object>().ok())
    {
        Object::assign(&kept, &state);
    }
    if Reflect::set(&kept, &JsValue::from_str(SCROLL_KEY), &JsValue::from_f64(y)).is_ok() {
        let _ = history.replace_state(&kept, "");
    }
}

/// After a move, the page Back or Forward brought scrolled where its entry
/// kept it; a move by a link starts at the top, as its swap put it.
fn restored() {
    if let Some(top) = RESTORING.take() {
        scroll_to(top);
    }
}

/// The page scrolled to `top` at once, never a smooth glide from wherever
/// the swap left it.
fn scroll_to(top: f64) {
    let options = ScrollToOptions::new();
    options.set_top(top);
    options.set_behavior(ScrollBehavior::Instant);
    window().scroll_to_with_scroll_to_options(&options);
}

/// The focus and the screen reader's line after a move: on a field the new
/// page focuses (`autofocus`, which htmx honours), else on `<main>`,
/// without scrolling; then the new title into the live region, a tick
/// later: htmx 4 fires `htmx:after:swap` before it sets the new page's
/// title, so read at once it would announce the page just left (found by
/// the browser check, 2026-10-18).
fn arrive() {
    let on_autofocus = document()
        .active_element()
        .is_some_and(|active| active.has_attribute("autofocus"));
    if !on_autofocus
        && let Some(main) = document()
            .get_element_by_id("content")
            .and_then(|main| main.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        let _ = main.focus_with_options(&options);
    }
    set_timeout(
        || {
            if let Some(region) = document().get_element_by_id(PAGE_ANNOUNCE) {
                region.set_text_content(Some(&document().title()));
            }
        },
        Duration::ZERO,
    );
}

/// A link's answer that htmx left unswapped, a status not among
/// [`PAGE_STATUSES`], loaded plainly: the link's own address, so the
/// browser shows whatever answered. A link alone (a GET): a form's post is
/// never sent twice, and an element with its own request keeps its own
/// handling.
fn load_plainly(ev: web_sys::Event) {
    let detail = Reflect::get(&ev, &JsValue::from_str("detail")).unwrap_or(JsValue::UNDEFINED);
    let ctx = Reflect::get(&detail, &JsValue::from_str("ctx")).unwrap_or(JsValue::UNDEFINED);
    let status = Reflect::get(&ctx, &JsValue::from_str("response"))
        .and_then(|response| Reflect::get(&response, &JsValue::from_str("status")))
        .ok()
        .and_then(|status| status.as_f64());
    let link = Reflect::get(&ctx, &JsValue::from_str("sourceElement"))
        .ok()
        .and_then(|source| source.dyn_into::<web_sys::HtmlAnchorElement>().ok())
        .filter(|link| !link.has_attribute("hx-get"));
    let shown =
        status.is_some_and(|status| PAGE_STATUSES.iter().any(|&page| f64::from(page) == status));
    if let Some(link) = link
        && !shown
    {
        let _ = window().location().assign(&link.href());
    }
}

/// The page a move leads to, where its answer comes from a newer build than
/// this page loaded: a deploy since. Asked before the answer is read
/// (`htmx:before:response`), so the caller cancels the swap and loads that
/// page plainly. A move alone: an answer going into the page's region, a
/// link's or Back's (a GET) or a form's whose post led on to a page (a
/// redirect followed, its GET safe to load again); never a post answered in
/// place, which would be sent twice. Nothing where the answer names no
/// build or this page has none.
fn newer_build(ev: &web_sys::Event) -> Option<String> {
    let field = |of: &JsValue, name: &str| Reflect::get(of, &JsValue::from_str(name)).ok();
    let ctx = field(&field(ev.as_ref(), "detail")?, "ctx")?;
    let response = field(&ctx, "response")?;
    let headers = field(&response, "headers")?;
    let get: Function = field(&headers, "get")?.dyn_into().ok()?;
    let theirs = get
        .call1(&headers, &JsValue::from_str(BUILD_HEADER))
        .ok()?
        .as_string()?;
    let ours = BUILD.with_borrow(Clone::clone)?;
    if theirs == ours {
        return None;
    }
    let raw: web_sys::Response = field(&response, "raw")?.dyn_into().ok()?;
    let method = field(&field(&ctx, "request")?, "method")?
        .as_string()
        .unwrap_or_default();
    (into_region(ev) && (method.eq_ignore_ascii_case("get") || raw.redirected())).then(|| raw.url())
}

/// Whether htmx's event `ev` is about a swap into the page's region: its
/// `detail.ctx.target`, the element or its `#id` selector.
fn into_region(ev: &web_sys::Event) -> bool {
    let field = |of: &JsValue, name: &str| Reflect::get(of, &JsValue::from_str(name)).ok();
    let Some(target) = field(ev.as_ref(), "detail")
        .and_then(|detail| field(&detail, "ctx"))
        .and_then(|ctx| field(&ctx, "target"))
    else {
        return false;
    };
    target
        .dyn_ref::<web_sys::Element>()
        .is_some_and(|target| target.id() == PAGE_REGION)
        || target
            .as_string()
            .is_some_and(|target| target == format!("#{PAGE_REGION}"))
}

/// Nothing focused: whatever has the focus loses it.
fn blur_focused() {
    if let Some(focused) = document()
        .active_element()
        .and_then(|focused| focused.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = focused.blur();
    }
}

/// Shows whether the browser half runs: the server renders "not running",
/// and the island switches to "running" once its chunk has loaded and
/// hydrated. Lazy, so it also proves the split build works. Home page only.
// Lazy everywhere but a test build: the browser tests (src/collect.rs) run
// a plain wasm, not cargo-leptos's split one, and a lazy island's import of
// `__wasm_split_placeholder__` would stop the test page from loading.
#[cfg_attr(not(test), island(lazy))]
#[cfg_attr(test, island)]
pub fn IslandsCheck() -> impl IntoView {
    collected(|| {
        let (running, set_running) = signal(false);
        // Effects run in the browser only, after hydration.
        Effect::new(move |_| set_running.set(true));
        view! {
            <p class="mt-8 text-sm text-ink-muted">
                {move || {
                    if running.get() {
                        "Islands: running. This line is a lazy island, loaded on its own."
                    } else {
                        "Islands: not running."
                    }
                }}
            </p>
        }
    })
}

/// Reads the source, so native only: the browser tests build for wasm32.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{fs, path::Path};

    /// Every island's body runs through `collected` (src/collect.rs): its
    /// first statement is the call. One that doesn't would stay in the
    /// tab's memory after htmx swaps it out, which no other test sees.
    #[test]
    fn every_island_runs_through_collected() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/islands.rs");
        let source = fs::read_to_string(&path).expect("src/islands.rs");
        let lines: Vec<&str> = source.lines().collect();
        let mut islands = 0;
        for (at, line) in lines.iter().enumerate() {
            if !line.starts_with("pub fn ") {
                continue;
            }
            // An island: an attribute right above it names `island`, plain or
            // inside a `cfg_attr`.
            let island = lines[..at]
                .iter()
                .rev()
                .take_while(|above| above.starts_with("#["))
                .any(|attribute| attribute.contains("island"));
            if !island {
                continue;
            }
            islands += 1;
            let first = lines[at + 1..]
                .iter()
                .map(|line| line.trim())
                .find(|line| !line.is_empty() && !line.starts_with("//"))
                .expect("the island's first statement");
            assert!(
                first.starts_with("collected("),
                "{line}\n  its body must go through `collected`, starts with: {first}"
            );
        }
        assert_eq!(
            islands, 2,
            "Navigation and IslandsCheck: has the file changed?"
        );
    }
}
