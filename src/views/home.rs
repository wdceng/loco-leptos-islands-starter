//! Home page. Server-rendered only: a plain `#[component]`, not an island,
//! so none of this reaches the browser bundle.

use leptos::prelude::*;

use crate::views::layout::APP_NAME;

/// Landing page content, rendered inside the shell's <main>.
#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <h1 class="text-4xl font-bold tracking-tight">
            {APP_NAME}
        </h1>
        <p class="mt-4 text-lg text-ink-muted">
            "Loco on the server, Leptos islands in the browser."
        </p>
    }
}
