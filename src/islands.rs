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

use leptos::prelude::*;

/// Shows whether the browser half runs: the server renders "not running",
/// and the island switches to "running" once its chunk has loaded and
/// hydrated. Lazy, so it also proves the split build works. Home page only.
#[island(lazy)]
pub fn IslandsCheck() -> impl IntoView {
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
}
