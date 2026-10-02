// auth mailer
//
// Loco's starter rendered these mails from Tera templates. Here the HTML part
// is a Leptos component (views/mail.rs), checked at compile time and escaped
// by Leptos; the subject and the text part are plain `format!`. Loco's
// `Mailer::mail` sends the finished parts, the same way `mail_template` does
// after rendering.

use leptos::{prelude::IntoView, view};
use loco_rs::prelude::*;

use crate::{
    app::stored,
    models::users,
    settings::Settings,
    views::mail::{self, ForgotPasswordMail, MagicLinkMail, WelcomeMail},
};

#[allow(clippy::module_name_repetitions)]
pub struct AuthMailer {}
impl Mailer for AuthMailer {}
impl AuthMailer {
    /// The sender, `settings.mail.from`. Without it Loco would send as
    /// `System <system@example.com>`, which most SMTP services refuse.
    fn sender(ctx: &AppContext) -> Result<String> {
        Ok(stored::<Settings>(ctx)?.mail.from)
    }

    /// The public origin the links in a mail start with: `server.host` as
    /// configured, not Loco's `full_url()`, which appends the bind port.
    /// Behind a reverse proxy that port is not the public one; locally the
    /// host in the config carries the port itself.
    fn origin(ctx: &AppContext) -> String {
        ctx.config.server.host.trim_end_matches('/').to_string()
    }

    /// One mail to `user`, from `settings.mail.from`.
    async fn send(
        ctx: &AppContext,
        user: &users::Model,
        subject: String,
        text: String,
        body: impl IntoView + 'static,
    ) -> Result<()> {
        Self::mail(
            ctx,
            &mailer::Email {
                from: Some(Self::sender(ctx)?),
                to: user.email.clone(),
                subject,
                text,
                html: mail::document(body),
                ..Default::default()
            },
        )
        .await
    }

    /// Sending welcome email the the given user
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn send_welcome(ctx: &AppContext, user: &users::Model) -> Result<()> {
        let link = format!(
            "{}/api/auth/verify/{}",
            Self::origin(ctx),
            user.email_verification_token.as_deref().unwrap_or_default()
        );
        Self::send(
            ctx,
            user,
            format!("Welcome {}", user.name),
            format!(
                "Welcome {}, you can now log in.\nVerify your account with the link below:\n\n{link}\n",
                user.name
            ),
            view! { <WelcomeMail name=user.name.clone() verify_link=link.clone() /> },
        )
        .await
    }

    /// Sending forgot password email
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn forgot_password(ctx: &AppContext, user: &users::Model) -> Result<()> {
        let link = format!(
            "{}/reset#{}",
            Self::origin(ctx),
            user.reset_token.as_deref().unwrap_or_default()
        );
        Self::send(
            ctx,
            user,
            "Your reset password link".to_string(),
            format!("Reset your password with this link:\n\n{link}\n"),
            view! { <ForgotPasswordMail name=user.name.clone() reset_link=link.clone() /> },
        )
        .await
    }

    /// Sends a magic link authentication email to the user.
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn send_magic_link(ctx: &AppContext, user: &users::Model) -> Result<()> {
        let token = user
            .magic_link_token
            .as_deref()
            .ok_or_else(|| Error::string("the user model not contains magic link token"))?;
        let link = format!("{}/api/auth/magic-link/{token}", Self::origin(ctx));
        Self::send(
            ctx,
            user,
            "Magic link example".to_string(),
            format!("Magic link with this link:\n{link}\n"),
            view! { <MagicLinkMail login_link=link.clone() /> },
        )
        .await
    }
}
