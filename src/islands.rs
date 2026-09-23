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
//! The template ships no island; the first one goes here.
