//! The not-found page, rendered by the router's fallback
//! (controllers/not_found.rs) with status 404. The only 404 page there is:
//! no static `404.html` sits in `public/`.

use leptos::prelude::*;

/// Rendered inside the shell's <main>, like every page.
#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <h1 class="text-4xl font-bold tracking-tight">"Page not found"</h1>
        <p class="mt-4 text-lg text-ink-muted">
            "The page you asked for does not exist or has moved."
        </p>
        <p class="mt-8">
            <a href="/" class="font-semibold text-primary">"Back to the home page"</a>
        </p>
    }
}
