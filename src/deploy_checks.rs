//! Two boot checks for what the config's `get_env` can't judge. A
//! `get_env` without a default refuses the boot when a variable is missing,
//! but a variable that is set and still wrong goes through.
//!
//! - [`refuse_placeholders`], production only: a JWT secret or SMTP host,
//!   user or password that is empty or still reads `replace-me` (the
//!   placeholder `docs/production.md` writes into `secrets.production.env`)
//!   refuses the boot. Staging keeps its placeholder defaults on purpose.
//! - [`spawn_smtp_login`], staging and production: logs in to the SMTP
//!   server once, in the background, and sends nothing. A failure is a
//!   warning in the journal, not a refusal: an outage at the mail provider
//!   must not keep the site down. Loco keeps its own transport's type
//!   private, so [`transport`] builds one from the same `mailer.smtp`
//!   block, the same way Loco's `EmailSender::smtp` does (loco-rs 1.2).

use lettre::{
    AsyncSmtpTransport, Tokio1Executor,
    transport::smtp::{authentication::Credentials, extension::ClientId},
};
use loco_rs::{
    Error, Result,
    app::AppContext,
    config::{MailerTls, SmtpMailer},
    environment::Environment,
};
use tracing::{info, warn};

use crate::maintenance::is_local;

/// What `docs/production.md` writes for a value still to be filled in.
pub const PLACEHOLDER: &str = "replace-me";

/// The secrets a production boot reads, by the variable that sets them.
fn secrets(ctx: &AppContext) -> Vec<(&'static str, String)> {
    let mut values = Vec::new();
    if let Some(jwt) = ctx.config.auth.as_ref().and_then(|auth| auth.jwt.as_ref()) {
        values.push(("JWT_SECRET", jwt.secret.clone()));
    }
    let smtp = ctx
        .config
        .mailer
        .as_ref()
        .and_then(|mailer| mailer.smtp.as_ref())
        .filter(|smtp| smtp.enable);
    if let Some(smtp) = smtp {
        values.push(("MAILER_HOST", smtp.host.clone()));
        if let Some(auth) = &smtp.auth {
            values.push(("MAILER_USER", auth.user.clone()));
            values.push(("MAILER_PASSWORD", auth.password.clone()));
        }
    }
    values
}

/// The names whose value is empty or still the placeholder.
fn not_filled_in(values: &[(&'static str, String)]) -> Vec<&'static str> {
    values
        .iter()
        .filter(|(_, value)| value.trim().is_empty() || value.contains(PLACEHOLDER))
        .map(|(name, _)| *name)
        .collect()
}

/// Refuses a production boot with a secret that is empty or still
/// `replace-me`. Any other environment passes.
///
/// # Errors
/// Production, and at least one of `JWT_SECRET`, `MAILER_HOST`,
/// `MAILER_USER`, `MAILER_PASSWORD` not filled in.
pub fn refuse_placeholders(ctx: &AppContext) -> Result<()> {
    if !matches!(ctx.environment, Environment::Production) {
        return Ok(());
    }
    let missing = not_filled_in(&secrets(ctx));
    if missing.is_empty() {
        return Ok(());
    }
    Err(Error::Message(format!(
        "production won't start: {} empty or still `{PLACEHOLDER}`. Fill them in secrets.production.env and deploy it (docs/production.md)",
        missing.join(", ")
    )))
}

/// An SMTP transport for `smtp`, built the way Loco's
/// `EmailSender::smtp` builds the one it sends with: the same TLS modes,
/// port, credentials and hello name.
fn transport(smtp: &SmtpMailer) -> std::result::Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let host = &smtp.host;
    let mut builder = match smtp.tls_mode() {
        MailerTls::Starttls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host).map_err(|e| e.to_string())?
        }
        MailerTls::Implicit => {
            AsyncSmtpTransport::<Tokio1Executor>::relay(host).map_err(|e| e.to_string())?
        }
        MailerTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
    }
    .port(smtp.port);
    if let Some(auth) = &smtp.auth {
        builder = builder.credentials(Credentials::new(auth.user.clone(), auth.password.clone()));
    }
    if let Some(hello_name) = &smtp.hello_name {
        builder = builder.hello_name(ClientId::Domain(hello_name.clone()));
    }
    Ok(builder.build())
}

/// Starts the one-time SMTP login in the background, when deployed and
/// mail goes over SMTP. Development (Mailpit may not run) and test (the
/// stub mailer) are skipped.
pub fn spawn_smtp_login(ctx: &AppContext) {
    if is_local(&ctx.environment) {
        return;
    }
    let smtp = ctx
        .config
        .mailer
        .as_ref()
        .filter(|mailer| !mailer.stub)
        .and_then(|mailer| mailer.smtp.as_ref())
        .filter(|smtp| smtp.enable);
    let Some(smtp) = smtp else {
        info!("smtp login check skipped: mail isn't sent over SMTP");
        return;
    };
    let transport = match transport(smtp) {
        Ok(transport) => transport,
        Err(err) => {
            warn!(error = %err, "smtp login check: the SMTP settings don't form a transport");
            return;
        }
    };
    tokio::spawn(async move {
        // Connects, starts TLS and logs in with MAILER_USER and
        // MAILER_PASSWORD, then sends NOOP. No mail goes out.
        match transport.test_connection().await {
            Ok(true) => info!("smtp login check: the mail server accepted the login"),
            Ok(false) => warn!("smtp login check: logged in, but the server didn't answer NOOP"),
            Err(err) => warn!(
                error = %err,
                "smtp login check failed: no mail will go out until MAILER_* in secrets.env is fixed"
            ),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        pairs.iter().map(|(n, v)| (*n, (*v).to_string())).collect()
    }

    #[test]
    fn real_values_pass() {
        let filled = values(&[
            ("JWT_SECRET", "k3l9…a long random string"),
            ("MAILER_HOST", "smtp.example.net"),
            ("MAILER_USER", "noreply@example.net"),
            ("MAILER_PASSWORD", "an app password"),
        ]);
        assert!(not_filled_in(&filled).is_empty());
    }

    #[test]
    fn empty_and_placeholder_values_are_named() {
        let mixed = values(&[
            ("JWT_SECRET", "k3l9…a long random string"),
            ("MAILER_HOST", "replace-me"),
            ("MAILER_USER", "   "),
            ("MAILER_PASSWORD", ""),
        ]);
        assert_eq!(
            not_filled_in(&mixed),
            vec!["MAILER_HOST", "MAILER_USER", "MAILER_PASSWORD"]
        );
    }
}
