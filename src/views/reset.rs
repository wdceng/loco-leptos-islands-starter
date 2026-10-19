//! The password-reset page the forgot-password mail links to
//! (controllers/reset.rs): the form, and what it says after a post. Plain
//! HTML, no island: the form posts and the server answers with a page.

use leptos::prelude::*;

use crate::{
    controllers::reset::PATH,
    models::users::{PASSWORD_MAX, PASSWORD_MIN, RESET_TOKEN_EXPIRATION_MIN},
};

/// The form: the token from the mail's link in a hidden field, and the new
/// password. `problem` is what the server refused last time, if anything;
/// then the field is `autofocus`, so the focus stays in it for the next
/// try: htmx focuses it in the swapped-in page, and `Navigation` leaves an
/// `autofocus` field alone instead of focusing `<main>`.
#[component]
pub fn ResetForm(token: String, problem: Option<String>) -> impl IntoView {
    let refused = problem.is_some();
    view! {
        <h1 class="text-4xl font-bold tracking-tight">"Choose a new password"</h1>
        <p class="mt-4 text-lg text-ink-muted">
            {format!("{PASSWORD_MIN} to {PASSWORD_MAX} characters. Any characters work.")}
        </p>
        {problem.map(|problem| view! { <p role="alert" class="mt-6 font-semibold">{problem}</p> })}
        <form method="post" action=PATH class="mt-8 max-w-sm">
            <input type="hidden" name="token" value=token />
            <label for="password" class="block font-semibold">"New password"</label>
            <input
                id="password"
                name="password"
                type="password"
                autocomplete="new-password"
                autofocus=refused
                required
                minlength=PASSWORD_MIN
                maxlength=PASSWORD_MAX
                class="mt-2 block w-full rounded-md border border-ink-muted px-3 py-2"
            />
            <button
                type="submit"
                class="mt-6 rounded-md bg-primary px-4 py-2 font-semibold text-white"
            >
                "Save the new password"
            </button>
        </form>
    }
}

/// After a good post: the password is changed.
#[component]
pub fn ResetDone() -> impl IntoView {
    view! {
        <h1 class="text-4xl font-bold tracking-tight">"Your password is changed"</h1>
        <p class="mt-4 text-lg text-ink-muted">"Sign in with the new one from now on."</p>
        <p class="mt-8">
            <a href="/" class="font-semibold text-primary">"Back to the home page"</a>
        </p>
    }
}

/// The link's token is unknown, used or expired.
#[component]
pub fn ResetExpired() -> impl IntoView {
    view! {
        <h1 class="text-4xl font-bold tracking-tight">"This link has expired"</h1>
        <p class="mt-4 text-lg text-ink-muted">
            {format!(
                "A reset link works once, for {RESET_TOKEN_EXPIRATION_MIN} minutes. Ask for a new one.",
            )}
        </p>
        <p class="mt-8">
            <a href="/" class="font-semibold text-primary">"Back to the home page"</a>
        </p>
    }
}
