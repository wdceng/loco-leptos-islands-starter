//! `/.well-known/security.txt` (src/controllers/security_txt.rs) as a
//! scanner or a researcher fetches it: RFC 9116's fields, plain text,
//! `Expires` in the future.

use app::app::App;
use chrono::{DateTime, Duration, Utc};
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn security_txt_names_the_contact_and_expires_ahead() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/.well-known/security.txt").await;
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.header("content-type"), "text/plain; charset=utf-8");
        let body = res.text();
        assert!(
            body.contains("Contact: mailto:security@example.com\n"),
            "{body}"
        );
        assert!(
            body.contains("Canonical: http://localhost:5150/.well-known/security.txt\n"),
            "{body}"
        );
        let expires = body
            .lines()
            .find_map(|line| line.strip_prefix("Expires: "))
            .unwrap_or_else(|| panic!("no Expires:\n{body}"));
        let expires = DateTime::parse_from_rfc3339(expires)
            .expect("Expires is RFC 3339")
            .with_timezone(&Utc);
        let now = Utc::now();
        assert!(expires > now, "Expires is in the past");
        assert!(
            expires < now + Duration::days(365),
            "Expires is more than a year ahead, which RFC 9116 advises against"
        );
    })
    .await;
}
