//! `/reset`: the page the forgot-password mail links to
//! (`/reset?token=…`, src/mailers/auth.rs), and its form's post. A plain
//! HTML form, so it works without the islands bundle.
//!
//! The token arrives in the address, so both answers go out with
//! `Referrer-Policy: no-referrer` (no request from the page carries the
//! address on) and `Cache-Control: no-store` (the browser keeps no copy).
//! The token works once, for `RESET_TOKEN_EXPIRATION_MIN` minutes.

use axum::{
    Form,
    body::Body,
    extract::{Query, Request},
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, REFERRER_POLICY},
        request::Parts,
    },
};
// Named imports on purpose: `leptos::prelude::*` also exports an `Error`
// type that would shadow Loco's.
use leptos::{IntoView, view};
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::{
    models::users::{self, password_problem},
    render::render_page,
    views::{
        layout::PageMeta,
        reset::{ResetDone, ResetExpired, ResetForm},
    },
};

/// The page's address, the form's target and the mail link's start.
pub const PATH: &str = "/reset";

/// The mail link's query.
#[derive(Debug, Deserialize)]
pub struct Link {
    #[serde(default)]
    pub token: String,
}

/// What the form posts; empty when a field is missing.
#[derive(Debug, Deserialize)]
pub struct Submitted {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub password: String,
}

fn meta() -> PageMeta {
    PageMeta {
        lang: "en",
        title: "Reset your password".into(),
        description: "Choose a new password for your account.".into(),
        // A page reached from a mail, never from a search.
        robots: Some("noindex"),
    }
}

/// Renders `page` with `status`, the address kept out of every referrer and
/// no copy kept by the browser.
async fn answer<V, F>(
    ctx: &AppContext,
    req: Request,
    status: StatusCode,
    page: F,
) -> Result<Response>
where
    F: Fn() -> V + Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    let mut res = render_page(ctx, req, meta(), page).await?;
    *res.status_mut() = status;
    let headers = res.headers_mut();
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

/// `GET /reset?token=…`: the form. The token isn't checked here: an
/// expired one is said after the post, together with everything else.
#[debug_handler]
async fn page(
    State(ctx): State<AppContext>,
    Query(link): Query<Link>,
    req: Request,
) -> Result<Response> {
    let token = link.token;
    answer(&ctx, req, StatusCode::OK, move || {
        view! { <ResetForm token=token.clone() problem=None /> }
    })
    .await
}

/// `POST /reset`: the password rule first, then the token. A refused
/// password shows the form again with the reason and the same token.
async fn submit(
    State(ctx): State<AppContext>,
    parts: Parts,
    Form(form): Form<Submitted>,
) -> Result<Response> {
    let req = Request::from_parts(parts, Body::empty());
    if let Some(problem) = password_problem(&form.password) {
        let token = form.token;
        return answer(&ctx, req, StatusCode::BAD_REQUEST, move || {
            view! { <ResetForm token=token.clone() problem=Some(problem.clone()) /> }
        })
        .await;
    }
    let Ok(user) = users::Model::find_by_reset_token(&ctx.db, &form.token).await else {
        return answer(
            &ctx,
            req,
            StatusCode::BAD_REQUEST,
            || view! { <ResetExpired /> },
        )
        .await;
    };
    let user = user
        .into_active_model()
        .reset_password(&ctx.db, &form.password)
        .await?;
    tracing::info!(
        pid = user.pid.to_string(),
        "password reset through the page"
    );
    answer(&ctx, req, StatusCode::OK, || view! { <ResetDone /> }).await
}

pub fn routes() -> Routes {
    Routes::new().add(PATH, get(page).post(submit))
}
