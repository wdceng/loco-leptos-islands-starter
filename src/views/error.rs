//! The error page a browser gets instead of Loco's raw answers: a 500 is
//! JSON there, a 405 or a 408 has an empty body, and a 413 or a bad form
//! is a line of JSON. `middleware/error_page.rs` renders a `Notice` with
//! these words inside the shell, keeping the original status. The 404 and
//! the 429 have pages of their own.

use axum::http::StatusCode;

/// The heading and the one-line explanation for a status.
#[must_use]
pub fn wording(status: StatusCode) -> (&'static str, &'static str) {
    match status {
        StatusCode::METHOD_NOT_ALLOWED => (
            "This page can't do that",
            "The link or form you used doesn't fit this page. Go back and try again from the page itself.",
        ),
        StatusCode::REQUEST_TIMEOUT => (
            "That took too long",
            "The server didn't finish in time. Please try again.",
        ),
        StatusCode::PAYLOAD_TOO_LARGE => (
            "That was too much to send",
            "The form sent more than this site accepts.",
        ),
        s if s.is_server_error() => (
            "Something went wrong on our side",
            "It's not your fault. Please try again in a moment.",
        ),
        _ => (
            "That request didn't work",
            "Something in it was wrong or missing. Go back and try again.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_class_has_its_own_words() {
        let five_hundred = wording(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(five_hundred, wording(StatusCode::BAD_GATEWAY));
        assert_ne!(five_hundred, wording(StatusCode::BAD_REQUEST));
        assert_ne!(
            wording(StatusCode::METHOD_NOT_ALLOWED),
            wording(StatusCode::BAD_REQUEST)
        );
        assert_ne!(
            wording(StatusCode::REQUEST_TIMEOUT),
            wording(StatusCode::BAD_REQUEST)
        );
    }
}
