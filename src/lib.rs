//! SaaS starter. This crate is compiled twice: with `ssr` into the Loco
//! server, and with `hydrate` into the browser bundle that wakes up the
//! Leptos islands. Everything below the first gate is server only.

#[cfg(feature = "ssr")]
pub mod app;
#[cfg(feature = "ssr")]
pub mod assets;
#[cfg(feature = "ssr")]
pub mod controllers;
#[cfg(feature = "ssr")]
pub mod data;
#[cfg(feature = "ssr")]
pub mod dtos;
#[cfg(feature = "ssr")]
pub mod initializers;
#[cfg(feature = "ssr")]
pub mod mailers;
#[cfg(feature = "ssr")]
pub mod middleware;
#[cfg(feature = "ssr")]
pub mod models;
#[cfg(feature = "ssr")]
pub mod render;
#[cfg(feature = "ssr")]
pub mod settings;
#[cfg(feature = "ssr")]
pub mod tasks;
#[cfg(feature = "ssr")]
pub mod views;
#[cfg(feature = "ssr")]
pub mod workers;

/// Browser entry point, called by the script tag cargo-leptos injects into
/// every page. Islands mode: only components marked `#[island]` are
/// hydrated; the rest of the page stays the static HTML the server sent.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
