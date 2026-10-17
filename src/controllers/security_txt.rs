//! `/.well-known/security.txt` (RFC 9116): where someone who finds a
//! security hole reports it. A route rather than a file under `public/`,
//! like `/robots.txt`: the contact comes from the config and `Expires`
//! moves with every answer, so it never goes stale.

use axum::{http::header::CONTENT_TYPE, response::IntoResponse};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use loco_rs::prelude::*;

use crate::{app::stored, settings::Settings};

/// The file's address, fixed by RFC 9116.
pub const PATH: &str = "/.well-known/security.txt";

/// How far ahead `Expires` is: under the RFC's one-year advice.
const VALID_FOR_DAYS: i64 = 180;

/// The file for `contact`, served from `host`, answered at `now`.
#[must_use]
pub fn body_for(contact: &str, host: &str, now: DateTime<Utc>) -> String {
    let host = host.trim_end_matches('/');
    let expires = (now + Duration::days(VALID_FOR_DAYS)).to_rfc3339_opts(SecondsFormat::Secs, true);
    format!(
        "Contact: {contact}\n\
         Expires: {expires}\n\
         Preferred-Languages: en\n\
         Canonical: {host}{PATH}\n"
    )
}

#[debug_handler]
async fn security_txt(State(ctx): State<AppContext>) -> Result<Response> {
    let settings: Settings = stored(&ctx)?;
    let body = body_for(
        &settings.security.contact,
        &ctx.config.server.host,
        Utc::now(),
    );
    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response())
}

pub fn routes() -> Routes {
    Routes::new().add(PATH, get(security_txt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_rfc_9116_asks_for() {
        let now = DateTime::parse_from_rfc3339("2026-10-18T12:00:00Z")
            .expect("a date")
            .with_timezone(&Utc);
        let body = body_for("mailto:security@example.com", "https://example.com/", now);
        assert_eq!(
            body,
            "Contact: mailto:security@example.com\n\
             Expires: 2027-04-16T12:00:00Z\n\
             Preferred-Languages: en\n\
             Canonical: https://example.com/.well-known/security.txt\n"
        );
    }
}
