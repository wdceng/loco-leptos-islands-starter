//! The web manifest (`/manifest.webmanifest`): what a phone needs to put
//! the site on its home screen. The name under the icon, the large icons,
//! the colours of the splash screen and the bars, and `standalone` (opens
//! without the browser's address bar).
//!
//! A route rather than a static file, so nothing in it is written twice:
//! the name is `APP_NAME`, the description the home page's, the colours the
//! shell's constants, and the icons the versioned constants from
//! `paths::assets`, so a changed icon gets a new URL here as everywhere.
//! `no-cache`: browsers recheck it, and it's tiny.

use axum::{
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    response::IntoResponse,
};
use loco_rs::prelude::*;
use serde_json::{Value, json};

use crate::{
    controllers::home::DESCRIPTION,
    paths::{self, assets},
    views::layout::{APP_NAME, BRAND_HEX, SURFACE_HEX},
};

/// The manifest as JSON.
#[must_use]
pub fn body() -> Value {
    json!({
        "id": "/",
        "name": APP_NAME,
        "short_name": APP_NAME,
        "description": DESCRIPTION,
        "lang": "en",
        "start_url": "/",
        "scope": "/",
        "icons": [
            {
                "src": assets::favicon::ANDROID_CHROME_192X192_PNG,
                "sizes": "192x192",
                "type": "image/png"
            },
            {
                "src": assets::favicon::ANDROID_CHROME_512X512_PNG,
                "sizes": "512x512",
                "type": "image/png"
            }
        ],
        "theme_color": BRAND_HEX,
        "background_color": SURFACE_HEX,
        "display": "standalone"
    })
}

#[debug_handler]
async fn manifest() -> Result<Response> {
    Ok((
        [
            (CONTENT_TYPE, "application/manifest+json"),
            (CACHE_CONTROL, "no-cache"),
        ],
        body().to_string(),
    )
        .into_response())
}

pub fn routes() -> Routes {
    Routes::new().add(paths::MANIFEST, get(manifest))
}
