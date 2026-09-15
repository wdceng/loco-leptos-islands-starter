//! Where a Loco controller hands a request to Leptos for a page.
//!
//! Every page controller calls [`render_page`]: it fetches the Leptos
//! options and the app settings from the shared store, renders the shell
//! around the page component, and sets the page's Content-Security-Policy.
//!
//! The CSP carries a per-request nonce, the same one Leptos stamps on the
//! inline scripts it emits (`HydrationScripts`, `AutoReload`). leptos_axum
//! generates its own nonce inside the render call, too late to end up in a
//! response header, so the nonce is created here and handed in through the
//! additional-context hook, which leptos_axum runs after its own
//! `provide_nonce()`: ours replaces it. Loco's `secure_headers` middleware
//! adds a header only when the response does not already carry it, so this
//! CSP wins on pages and the preset one remains the fallback elsewhere.

use axum::{
    extract::Request,
    http::{HeaderValue, header::CONTENT_SECURITY_POLICY},
};
use leptos::{
    config::LeptosOptions,
    nonce::Nonce,
    prelude::{IntoView, provide_context},
};
use loco_rs::prelude::*;

use crate::{
    assets::Assets,
    settings::Settings,
    views::layout::{PageMeta, shell},
};

/// Placeholder in `settings.security.content_security_policy`.
pub const NONCE_PLACEHOLDER: &str = "{nonce}";

/// Renders `page` inside the document shell and returns the HTTP response,
/// streaming, with the nonce CSP attached.
///
/// # Errors
/// The Leptos options or the settings are missing from the shared store
/// (the app did not boot through `after_context`), or the CSP template does
/// not form a valid header value.
pub async fn render_page<V, F>(
    ctx: &AppContext,
    req: Request,
    meta: PageMeta,
    page: F,
) -> Result<Response>
where
    F: Fn() -> V + Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    let options: LeptosOptions = ctx
        .shared_store
        .get()
        .ok_or_else(|| Error::Message("Leptos options missing from shared store".into()))?;
    let settings: Settings = ctx
        .shared_store
        .get()
        .ok_or_else(|| Error::Message("settings missing from shared store".into()))?;
    let assets: Assets = ctx
        .shared_store
        .get()
        .ok_or_else(|| Error::Message("assets missing from shared store".into()))?;

    let nonce = Nonce::new();
    let csp = csp_header(&settings.security.content_security_policy, &nonce)?;

    let render = leptos_axum::render_app_to_stream_with_context(
        move || provide_context(nonce.clone()),
        move || {
            shell(
                options.clone(),
                assets.stylesheet.clone(),
                meta.clone(),
                page(),
            )
        },
    );
    let mut res = render(req).await;
    res.headers_mut().insert(CONTENT_SECURITY_POLICY, csp);
    Ok(res)
}

/// Fills the nonce into the CSP template from the config.
///
/// # Errors
/// The result is not a valid header value (the template is validated at
/// boot, so this only fires if the nonce itself were malformed).
fn csp_header(template: &str, nonce: &Nonce) -> Result<HeaderValue> {
    let value = template.replace(NONCE_PLACEHOLDER, nonce.as_inner());
    HeaderValue::from_str(&value)
        .map_err(|e| Error::Message(format!("content-security-policy header: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_is_substituted() {
        let nonce = Nonce::from_value("abc123");
        let header = csp_header(
            "default-src 'self'; script-src 'self' 'nonce-{nonce}' 'wasm-unsafe-eval'",
            &nonce,
        )
        .expect("valid header");
        let value = header.to_str().expect("ascii");
        assert_eq!(
            value,
            "default-src 'self'; script-src 'self' 'nonce-abc123' 'wasm-unsafe-eval'"
        );
        assert!(!value.contains(NONCE_PLACEHOLDER));
    }

    #[test]
    fn invalid_header_value_is_an_error() {
        let nonce = Nonce::from_value("abc123");
        let err = csp_header("default-src 'self'\nscript-src {nonce}", &nonce)
            .expect_err("newline is not allowed in a header value");
        assert!(err.to_string().contains("content-security-policy"), "{err}");
    }
}
