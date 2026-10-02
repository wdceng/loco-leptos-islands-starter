//! Home page. Loco owns the URL; Leptos renders the document.

use axum::extract::Request;
// Named import on purpose: `leptos::prelude::*` also exports an `Error`
// type that would shadow Loco's.
use leptos::view;
use loco_rs::prelude::*;

use crate::{
    render::render_page,
    views::{
        home::HomePage,
        layout::{APP_NAME, PageMeta},
    },
};

/// The site in one sentence: the home page's meta description, and the
/// summary line of `/llms.txt` (controllers/llms.rs).
pub const DESCRIPTION: &str =
    "A full-stack starter: Loco on the server, Leptos islands in the browser.";

#[debug_handler]
async fn index(State(ctx): State<AppContext>, req: Request) -> Result<Response> {
    let meta = PageMeta {
        lang: "en",
        title: APP_NAME.into(),
        description: DESCRIPTION.into(),
        robots: None,
    };
    render_page(&ctx, req, meta, || view! { <HomePage/> }).await
}

pub fn routes() -> Routes {
    Routes::new().add("/", get(index))
}
