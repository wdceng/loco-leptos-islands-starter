// auth mailer
#![allow(non_upper_case_globals)]

use loco_rs::prelude::*;
use serde_json::json;

use crate::{models::users, settings::Settings};

static welcome: Dir<'_> = include_dir!("src/mailers/auth/welcome");
static forgot: Dir<'_> = include_dir!("src/mailers/auth/forgot");
static magic_link: Dir<'_> = include_dir!("src/mailers/auth/magic_link");

#[allow(clippy::module_name_repetitions)]
pub struct AuthMailer {}
impl Mailer for AuthMailer {}
impl AuthMailer {
    /// The sender, `settings.mail.from`. Without it Loco would send as
    /// `System <system@example.com>`, which most SMTP services refuse.
    fn sender(ctx: &AppContext) -> Result<String> {
        ctx.shared_store
            .get::<Settings>()
            .map(|settings| settings.mail.from)
            .ok_or_else(|| Error::Message("settings missing from shared store".into()))
    }

    /// The public origin the links in a mail start with: `server.host` as
    /// configured, not Loco's `full_url()`, which appends the bind port.
    /// Behind a reverse proxy that port is not the public one; locally the
    /// host in the config carries the port itself.
    fn origin(ctx: &AppContext) -> String {
        ctx.config.server.host.trim_end_matches('/').to_string()
    }

    /// Sending welcome email the the given user
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn send_welcome(ctx: &AppContext, user: &users::Model) -> Result<()> {
        Self::mail_template(
            ctx,
            &welcome,
            mailer::Args {
                from: Some(Self::sender(ctx)?),
                to: user.email.clone(),
                locals: json!({
                  "name": user.name,
                  "verifyToken": user.email_verification_token,
                  "host": Self::origin(ctx)
                }),
                ..Default::default()
            },
        )
        .await?;

        Ok(())
    }

    /// Sending forgot password email
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn forgot_password(ctx: &AppContext, user: &users::Model) -> Result<()> {
        Self::mail_template(
            ctx,
            &forgot,
            mailer::Args {
                from: Some(Self::sender(ctx)?),
                to: user.email.clone(),
                locals: json!({
                  "name": user.name,
                  "resetToken": user.reset_token,
                  "host": Self::origin(ctx)
                }),
                ..Default::default()
            },
        )
        .await?;

        Ok(())
    }

    /// Sends a magic link authentication email to the user.
    ///
    /// # Errors
    ///
    /// When email sending is failed
    pub async fn send_magic_link(ctx: &AppContext, user: &users::Model) -> Result<()> {
        Self::mail_template(
            ctx,
            &magic_link,
            mailer::Args {
                from: Some(Self::sender(ctx)?),
                to: user.email.clone(),
                locals: json!({
                  "name": user.name,
                  "token": user.magic_link_token.clone().ok_or_else(|| Error::string(
                            "the user model not contains magic link token",
                    ))?,
                  "host": Self::origin(ctx)
                }),
                ..Default::default()
            },
        )
        .await?;

        Ok(())
    }
}
