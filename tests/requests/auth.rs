use app::{app::App, models::users};
use insta::{assert_debug_snapshot, with_settings};
use loco_rs::testing::prelude::*;
use rstest::rstest;
use serial_test::serial;

use super::prepare_data;

// TODO: see how to dedup / extract this to app-local test utils
// not to framework, because that would require a runtime dep on insta
macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_suffix("auth_request");
        let _guard = settings.bind_to_scope();
    };
}

/// Every auth mail is sent from `settings.mail.from` (`config/test.yaml`),
/// never Loco's default `System <system@example.com>`.
fn assert_from_the_configured_sender(mail: &str) {
    let from = mail
        .lines()
        .find(|line| line.starts_with("From: "))
        .unwrap_or_else(|| panic!("no From header:\n{mail}"));
    assert!(
        from.contains("SaaS Starter") && from.contains("<noreply@example.com>"),
        "sender is not settings.mail.from: {from}"
    );
}

#[tokio::test]
#[serial]
async fn can_register() {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let email = "test@loco.com";
        let payload = serde_json::json!({
            "name": "loco",
            "email": email,
            "password": "12341234"
        });

        let response = request.post("/api/auth/register").json(&payload).await;
        assert_eq!(
            response.status_code(),
            200,
            "Register request should succeed"
        );
        let saved_user = users::Model::find_by_email(&ctx.db, email)
            .await
            .expect("registration should have persisted a user");

        // Snapshot only the fields this test is about, never the whole
        // `Model` — see the note in `tests/models/users.rs`. Anything the
        // snapshot does not cover, assert directly:
        assert!(
            saved_user.email_verification_token.is_some(),
            "registration should issue an email verification token"
        );
        assert!(
            saved_user.email_verified_at.is_none(),
            "a freshly registered user is not verified yet"
        );
        assert_debug_snapshot!((saved_user.email, saved_user.name));

        let deliveries = ctx.mailer.unwrap().deliveries();
        assert_eq!(deliveries.count, 1, "Exactly one email should be sent");
        // The link starts with `server.host` as configured, port included,
        // never `host:port` (src/mailers/auth.rs).
        let mail = &deliveries.messages[0];
        assert_from_the_configured_sender(mail);
        assert!(
            mail.contains("http://localhost:5150/api/auth/verify/"),
            "verification link does not start with server.host:\n{mail}"
        );

        // with_settings!({
        //     filters => cleanup_email()
        // }, {
        //     assert_debug_snapshot!(ctx.mailer.unwrap().deliveries());
        // });
    })
    .await;
}

#[rstest]
#[case("login_with_valid_password", "12341234")]
#[case("login_with_invalid_password", "invalid-password")]
#[tokio::test]
#[serial]
async fn can_login_with_verify(#[case] test_name: &str, #[case] password: &str) {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let email = "test@loco.com";
        let register_payload = serde_json::json!({
            "name": "loco",
            "email": email,
            "password": "12341234"
        });

        //Creating a new user
        let register_response = request
            .post("/api/auth/register")
            .json(&register_payload)
            .await;

        assert_eq!(
            register_response.status_code(),
            200,
            "Register request should succeed"
        );

        let user = users::Model::find_by_email(&ctx.db, email).await.unwrap();
        let email_verification_token = user
            .email_verification_token
            .expect("Email verification token should be generated");
        request
            .get(&format!("/api/auth/verify/{email_verification_token}"))
            .await;

        //verify user request
        let response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .await;

        // Make sure email_verified_at is set
        let user = users::Model::find_by_email(&ctx.db, email)
            .await
            .expect("Failed to find user by email");

        assert!(
            user.email_verified_at.is_some(),
            "Expected the email to be verified, but it was not. User: {:?}",
            user
        );

        with_settings!({
            filters => cleanup_user_model()
        }, {
            assert_debug_snapshot!(test_name, (response.status_code(), response.text()));
        });
    })
    .await;
}

#[tokio::test]
#[serial]
async fn login_with_un_existing_email() {
    configure_insta!();

    request::<App, _, _>(|request, _ctx| async move {

        let login_response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "un_existing@loco.rs",
                "password":  "1234"
            }))
            .await;

        assert_eq!(login_response.status_code(), 401, "Login request should return 401");
        login_response.assert_json(&serde_json::json!({"error": "unauthorized", "description": "You do not have permission to access this resource"}));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_login_without_verify() {
    configure_insta!();

    request::<App, _, _>(|request, _ctx| async move {
        let email = "test@loco.com";
        let password = "12341234";
        let register_payload = serde_json::json!({
            "name": "loco",
            "email": email,
            "password": password
        });

        //Creating a new user
        let register_response = request
            .post("/api/auth/register")
            .json(&register_payload)
            .await;

        assert_eq!(
            register_response.status_code(),
            200,
            "Register request should succeed"
        );

        //verify user request
        let login_response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .await;

        assert_eq!(
            login_response.status_code(),
            200,
            "Login request should succeed"
        );

        with_settings!({
            filters => cleanup_user_model()
        }, {
            assert_debug_snapshot!(login_response.text());
        });
    })
    .await;
}

#[tokio::test]
#[serial]
async fn invalid_verification_token() {
    configure_insta!();

    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/verify/invalid-token").await;

        assert_eq!(response.status_code(), 401, "Verify request should reject");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_reset_password() {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let login_data = prepare_data::init_user_login(&request, &ctx).await;

        let forgot_payload = serde_json::json!({
            "email": login_data.user.email,
        });
        let forget_response = request.post("/api/auth/forgot").json(&forgot_payload).await;
        assert_eq!(
            forget_response.status_code(),
            200,
            "Forget request should succeed"
        );

        let user = users::Model::find_by_email(&ctx.db, &login_data.user.email)
            .await
            .expect("Failed to find user by email");

        assert!(
            user.reset_token.is_some(),
            "Expected reset_token to be set, but it was None. User: {user:?}"
        );
        // The reset clears it; the mail is checked against it at the end.
        let reset_token = user.reset_token.clone().expect("a reset token");
        assert!(
            user.reset_sent_at.is_some(),
            "Expected reset_sent_at to be set, but it was None. User: {user:?}"
        );

        let new_password = "new-password";
        let reset_payload = serde_json::json!({
            "token": user.reset_token,
            "password": new_password,
        });

        let reset_response = request.post("/api/auth/reset").json(&reset_payload).await;
        assert_eq!(
            reset_response.status_code(),
            200,
            "Reset password request should succeed"
        );

        let user = users::Model::find_by_email(&ctx.db, &user.email)
            .await
            .unwrap();

        assert!(user.reset_token.is_none());
        assert!(user.reset_sent_at.is_none());

        assert_debug_snapshot!(reset_response.text());

        let login_response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": user.email,
                "password": new_password
            }))
            .await;

        assert_eq!(
            login_response.status_code(),
            200,
            "Login request should succeed"
        );

        let deliveries = ctx.mailer.unwrap().deliveries();
        assert_eq!(deliveries.count, 2, "Exactly one email should be sent");
        // The second mail is the reset link (the first is the welcome mail
        // from registering). `/reset` is a page the project's front end
        // provides; the template has none yet (Known Gaps in docs/ARCHITECTURE.md).
        let mail = &deliveries.messages[1];
        assert_from_the_configured_sender(mail);
        assert!(
            mail.contains(&format!("http://localhost:5150/reset#{reset_token}")),
            "reset link does not start with server.host:\n{mail}"
        );
        // with_settings!({
        //     filters => cleanup_email()
        // }, {
        //     assert_debug_snapshot!(deliveries.messages);
        // });
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_get_current_user() {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let user = prepare_data::init_user_login(&request, &ctx).await;

        let (auth_key, auth_value) = prepare_data::auth_header(&user.token);
        let response = request
            .get("/api/auth/current")
            .add_header(auth_key, auth_value)
            .await;

        assert_eq!(
            response.status_code(),
            200,
            "Current request should succeed"
        );

        with_settings!({
            filters => cleanup_user_model()
        }, {
            assert_debug_snapshot!((response.status_code(), response.text()));
        });
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_auth_with_magic_link() {
    configure_insta!();
    request::<App, _, _>(|request, ctx| async move {
        seed::<App>(&ctx).await.unwrap();

        let payload = serde_json::json!({
            "email": "user1@example.com",
        });
        let response = request.post("/api/auth/magic-link").json(&payload).await;
        assert_eq!(
            response.status_code(),
            200,
            "Magic link request should succeed"
        );

        let deliveries = ctx.mailer.unwrap().deliveries();
        assert_eq!(deliveries.count, 1, "Exactly one email should be sent");

        // let redact_token = format!("[a-zA-Z0-9]{{{}}}", users::MAGIC_LINK_LENGTH);
        // with_settings!({
        //      filters => {
        //          let mut combined_filters = cleanup_email().clone();
        //         combined_filters.extend(vec![(r"(\\r\\n|=\\r\\n)", ""), (redact_token.as_str(), "[REDACT_TOKEN]") ]);
        //         combined_filters
        //     }
        // }, {
        //     assert_debug_snapshot!(deliveries.messages);
        // });

        let user = users::Model::find_by_email(&ctx.db, "user1@example.com")
            .await
            .expect("User should be found");

        let magic_link_token = user
            .magic_link_token
            .expect("Magic link token should be generated");
        let mail = &deliveries.messages[0];
        assert_from_the_configured_sender(mail);
        assert!(
            mail.contains(&format!(
                "http://localhost:5150/api/auth/magic-link/{magic_link_token}"
            )),
            "magic link does not start with server.host:\n{mail}"
        );
        let magic_link_response = request
            .get(&format!("/api/auth/magic-link/{magic_link_token}"))
            .await;
        assert_eq!(
            magic_link_response.status_code(),
            200,
            "Magic link authentication should succeed"
        );

        with_settings!({
            filters => cleanup_user_model()
        }, {
            assert_debug_snapshot!(magic_link_response.text());
        });
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_reject_invalid_email() {
    configure_insta!();
    request::<App, _, _>(|request, _ctx| async move {
        let invalid_email = "user1@temp-mail.com";
        let payload = serde_json::json!({
            "email": invalid_email,
        });
        let response = request.post("/api/auth/magic-link").json(&payload).await;
        assert_eq!(
            response.status_code(),
            400,
            "Expected request with invalid email '{invalid_email}' to be blocked, but it was allowed."
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_reject_invalid_magic_link_token() {
    configure_insta!();
    request::<App, _, _>(|request, ctx| async move {
        seed::<App>(&ctx).await.unwrap();

        let magic_link_response = request.get("/api/auth/magic-link/invalid-token").await;
        assert_eq!(
            magic_link_response.status_code(),
            401,
            "Magic link authentication should be rejected"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_resend_verification_email() {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let email = "test@loco.com";
        let payload = serde_json::json!({
            "name": "loco",
            "email": email,
            "password": "12341234"
        });

        let response = request.post("/api/auth/register").json(&payload).await;
        assert_eq!(
            response.status_code(),
            200,
            "Register request should succeed"
        );

        let resend_payload = serde_json::json!({ "email": email });

        let resend_response = request
            .post("/api/auth/resend-verification-mail")
            .json(&resend_payload)
            .await;

        assert_eq!(
            resend_response.status_code(),
            200,
            "Resend verification email should succeed"
        );

        let deliveries = ctx.mailer.unwrap().deliveries();

        assert_eq!(
            deliveries.count, 2,
            "Two emails should have been sent: welcome and re-verification"
        );

        let user = users::Model::find_by_email(&ctx.db, email)
            .await
            .expect("User should exist");

        // Narrowed on purpose — see the note in `tests/models/users.rs`.
        assert!(
            user.email_verification_token.is_some(),
            "resending should leave a verification token on the user"
        );
        assert_debug_snapshot!("resend_verification_user", (user.email, user.name));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn cannot_resend_email_if_already_verified() {
    configure_insta!();

    request::<App, _, _>(|request, ctx| async move {
        let email = "verified@loco.com";
        let payload = serde_json::json!({
            "name": "verified",
            "email": email,
            "password": "12341234"
        });

        request.post("/api/auth/register").json(&payload).await;

        // Verify user
        let user = users::Model::find_by_email(&ctx.db, email).await.unwrap();
        if let Some(token) = user.email_verification_token.clone() {
            request.get(&format!("/api/auth/verify/{token}")).await;
        }

        // Try resending verification email
        let resend_payload = serde_json::json!({ "email": email });

        let resend_response = request
            .post("/api/auth/resend-verification-mail")
            .json(&resend_payload)
            .await;

        assert_eq!(
            resend_response.status_code(),
            200,
            "Should return 200 even if already verified"
        );

        let deliveries = ctx.mailer.unwrap().deliveries();
        assert_eq!(
            deliveries.count, 1,
            "Only the original welcome email should be sent"
        );
    })
    .await;
}

/// The `text/html` part of a raw multipart message from the stub mailer,
/// with its transfer encoding undone, so the test reads what a mail client
/// renders.
fn html_part(mail: &str) -> String {
    let start = mail
        .find("Content-Type: text/html")
        .unwrap_or_else(|| panic!("no HTML part:\n{mail}"));
    let (head, rest) = mail[start..]
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an HTML part without a body:\n{mail}"));
    let body = rest.split("\r\n--").next().unwrap_or(rest);
    if head.contains("quoted-printable") {
        decode_quoted_printable(body)
    } else {
        assert!(
            !head.contains("base64"),
            "the HTML part is base64 now, decode it here:\n{head}"
        );
        body.to_owned()
    }
}

/// Quoted-printable (RFC 2045): soft line breaks removed, `=XX` turned back
/// into its byte.
fn decode_quoted_printable(body: &str) -> String {
    let unfolded = body.replace("=\r\n", "");
    let bytes = unfolded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'='
            && let Some(byte) = bytes
                .get(i + 1..i + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).expect("the HTML part is UTF-8")
}

/// The name is typed by whoever registers, and the welcome mail goes to any
/// address they give. Loco's Tera does not escape `html.t`, so the template
/// does (`{{ name | escape }}`): markup in the name arrives as text.
#[tokio::test]
#[serial]
async fn markup_in_the_name_arrives_escaped_in_the_html_mail() {
    request::<App, _, _>(|request, ctx| async move {
        let payload = serde_json::json!({
            "name": "<b>Ana</b> <a href=\"https://evil.example\">click</a>",
            "email": "ana@example.com",
            "password": "12341234"
        });
        let response = request.post("/api/auth/register").json(&payload).await;
        assert_eq!(response.status_code(), 200);

        let mails = ctx.mailer.expect("stub mailer").deliveries().messages;
        assert_eq!(mails.len(), 1, "the welcome mail");
        let html = html_part(&mails[0]);
        assert!(
            html.contains("&lt;b&gt;Ana&lt;&#x2F;b&gt;") || html.contains("&lt;b&gt;Ana&lt;/b&gt;"),
            "the markup is shown as text:\n{html}"
        );
        for raw in ["<b>Ana", "<a href=\"https://evil.example\""] {
            assert!(!html.contains(raw), "raw `{raw}` in the HTML part:\n{html}");
        }
    })
    .await;
}

/// The name is capped at 100 characters (`users::Validator`): a longer one
/// registers nobody and sends nothing.
#[tokio::test]
#[serial]
async fn a_name_longer_than_100_characters_is_refused() {
    request::<App, _, _>(|request, ctx| async move {
        let email = "long@example.com";
        let payload = serde_json::json!({
            "name": "a".repeat(101),
            "email": email,
            "password": "12341234"
        });
        let response = request.post("/api/auth/register").json(&payload).await;
        assert_eq!(
            response.status_code(),
            200,
            "register answers the same either way, so it does not reveal who exists"
        );
        assert!(
            users::Model::find_by_email(&ctx.db, email).await.is_err(),
            "no user was created"
        );
        assert_eq!(
            ctx.mailer.expect("stub mailer").deliveries().count,
            0,
            "no mail was sent"
        );
    })
    .await;
}

/// The body limit (`limit_payload`, 64 KB in every config): a larger body is
/// refused before any handler parses it.
#[tokio::test]
#[serial]
async fn a_body_past_the_limit_is_refused() {
    request::<App, _, _>(|request, _ctx| async move {
        let payload = serde_json::json!({
            "email": "nobody@example.com",
            "password": "a".repeat(70_000)
        });
        let response = request.post("/api/auth/login").json(&payload).await;
        assert_eq!(response.status_code(), 413, "a body past 64 KB");
    })
    .await;
}
