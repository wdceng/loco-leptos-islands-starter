//! The `/reset` page (src/controllers/reset.rs), the way a person uses it:
//! the mail's link, the form, a refused password, the new one, and the link
//! spent.

use app::{app::App, models::users};
use loco_rs::testing::prelude::*;
use serial_test::serial;

use super::prepare_data;

fn login(email: &str, password: &str) -> serde_json::Value {
    serde_json::json!({ "email": email, "password": password })
}

#[tokio::test]
#[serial]
async fn the_reset_page_changes_the_password_once() {
    request::<App, _, _>(|request, ctx| async move {
        let email = prepare_data::init_user_login(&request, &ctx)
            .await
            .user
            .email;
        request
            .post("/api/auth/forgot")
            .json(&serde_json::json!({ "email": email }))
            .await;
        let token = users::Model::find_by_email(&ctx.db, &email)
            .await
            .expect("the user")
            .reset_token
            .expect("a reset token");

        // The mail's link: the form, the token in it, the address kept out
        // of referrers and caches, out of search results.
        let page = request.get(&format!("/reset?token={token}")).await;
        assert_eq!(page.status_code(), 200);
        assert_eq!(page.header("cache-control"), "no-store");
        assert_eq!(page.header("referrer-policy"), "no-referrer");
        let body = page.text();
        assert!(body.contains(r#"action="/reset""#), "{body}");
        assert!(body.contains(&format!(r#"value="{token}""#)), "{body}");
        assert!(
            body.contains(r#"name="robots" content="noindex""#),
            "{body}"
        );

        // Too short: the form again, with the reason and the same token.
        let refused = request
            .post("/reset")
            .form(&serde_json::json!({ "token": token, "password": "short" }))
            .await;
        assert_eq!(refused.status_code(), 400);
        // A page answered to a post keeps the address the browser is at:
        // htmx would otherwise push `/reset` as a POST's (src/render.rs).
        assert_eq!(refused.header("hx-push-url"), "false");
        assert!(
            page.maybe_header("hx-push-url").is_none(),
            "a GET's page pushes as usual"
        );
        let body = refused.text();
        assert!(body.contains("must be 8 to 128 characters"), "{body}");
        assert!(body.contains(&format!(r#"value="{token}""#)), "{body}");

        // The new password: done, and it's the one that signs in now.
        let done = request
            .post("/reset")
            .form(&serde_json::json!({ "token": token, "password": "a-new-password" }))
            .await;
        assert_eq!(done.status_code(), 200);
        assert!(done.text().contains("Your password is changed"));
        let new = request
            .post("/api/auth/login")
            .json(&login(&email, "a-new-password"))
            .await;
        assert_eq!(new.status_code(), 200, "the new password");
        let old = request
            .post("/api/auth/login")
            .json(&login(&email, "12341234"))
            .await;
        assert_eq!(old.status_code(), 401, "the old password");

        // The link is spent.
        let again = request
            .post("/reset")
            .form(&serde_json::json!({ "token": token, "password": "another-password" }))
            .await;
        assert_eq!(again.status_code(), 400);
        assert!(again.text().contains("This link has expired"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unknown_token_is_an_expired_link() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request
            .post("/reset")
            .form(&serde_json::json!({ "token": "no-such-token", "password": "a-new-password" }))
            .await;
        assert_eq!(res.status_code(), 400);
        assert!(res.text().contains("This link has expired"));
        assert_eq!(res.header("cache-control"), "no-store");
    })
    .await;
}
