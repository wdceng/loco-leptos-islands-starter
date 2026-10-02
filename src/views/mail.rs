//! The HTML part of the auth mails, as Leptos components. Loco's starter
//! shipped them as Tera templates (`html.t`); here a misspelled value is a
//! compile error instead of a failed send, and every value is escaped by
//! Leptos, the name a visitor typed at registration included. The subject
//! and the text part are plain `format!` in `mailers/auth.rs`: no markup.
//!
//! The wording is the starter's. Mail clients ignore stylesheets, so these
//! carry no classes; if a mail ever needs styling, inline `style=` is fine
//! here, since the page CSP doesn't apply to mail.

use leptos::prelude::*;

/// A whole mail document around `body`, ready for `Email::html`.
pub fn document(body: impl IntoView + 'static) -> String {
    let html = view! {
        <html lang="en">
            <head>
                <meta charset="utf-8" />
            </head>
            <body>{body}</body>
        </html>
    }
    .to_html();
    format!("<!DOCTYPE html>{html}")
}

#[component]
pub fn WelcomeMail(name: String, verify_link: String) -> impl IntoView {
    view! {
        <p>{format!("Dear {name},")}</p>
        <p>
            "Welcome to Loco! You can now log in to your account. Before you get started, please verify your account by clicking the link below:"
        </p>
        <p>
            <a href=verify_link>"Verify Your Account"</a>
        </p>
        <p>"Best regards," <br /> "The Loco Team"</p>
    }
}

#[component]
pub fn ForgotPasswordMail(name: String, reset_link: String) -> impl IntoView {
    view! {
        <p>{format!("Hey {name},")}</p>
        <p>"Forgot your password? No worries! You can reset it by clicking the link below:"</p>
        <p>
            <a href=reset_link>"Reset Your Password"</a>
        </p>
        <p>"If you didn't request a password reset, please ignore this email."</p>
        <p>"Best regards," <br /> "The Loco Team"</p>
    }
}

#[component]
pub fn MagicLinkMail(login_link: String) -> impl IntoView {
    view! {
        <p>"Magic link example:"</p>
        <p>
            <a href=login_link>"Verify Your Account"</a>
        </p>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn welcome(name: &str) -> String {
        document(view! {
            <WelcomeMail name=name.to_string() verify_link="https://example.com/api/auth/verify/t0k3n".to_string() />
        })
    }

    #[test]
    fn mail_is_a_whole_document_with_the_link() {
        let html = welcome("Ana");
        assert!(
            html.starts_with("<!DOCTYPE html><html lang=\"en\">"),
            "{html}"
        );
        assert!(
            html.contains("href=\"https://example.com/api/auth/verify/t0k3n\""),
            "{html}"
        );
        assert!(html.contains("Dear Ana"), "{html}");
        assert!(html.trim_end().ends_with("</html>"), "{html}");
        // One text node per paragraph: no hydration markers in a mail.
        assert!(!html.contains("<!>"), "{html}");
    }

    /// The name is typed by whoever registers, so it must arrive as text.
    #[test]
    fn a_visitors_name_is_escaped() {
        let html = welcome("<script>alert(1)</script> & \"x\"");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp;"), "{html}");
    }
}
