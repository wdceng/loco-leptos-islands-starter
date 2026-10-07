//! Typed view of the `settings:` block in `config/<env>.yaml`.
//!
//! Loco hands app-specific settings over as raw JSON (`Config::settings`).
//! This module turns them into a struct once, at boot in `after_context`,
//! and parks the result in `AppContext::shared_store`. A missing block, an
//! unknown key or a nonsensical value refuses to start the app, so a typo in
//! production config is a loud failure rather than a silently disabled limit.
//!
//! Only the rate-limit blocks derive `Serialize`: they are what
//! `cargo loco middleware -c` prints for `rate_limit`.

use axum::http::HeaderValue;
use chrono::NaiveTime;
use chrono_tz::Tz;
use lettre::message::Mailbox;
use loco_rs::{Error, Result, config::Config, environment::Environment};
use serde::{Deserialize, Serialize};

/// Placeholder in `settings.security.content_security_policy`, replaced
/// per request by `render_page` (src/render.rs).
pub const NONCE_PLACEHOLDER: &str = "{nonce}";

/// Whether `env` runs on a developer's machine (`development`, Loco's
/// default when `LOCO_ENV` is unset) or in the tests, never deployed: the
/// nightly restart and the SMTP login check skip it, and the rate limiter
/// keys on the connection's own address. Loco reads a name it doesn't know
/// as `Environment::Any`, which is how `staging` and `dev-server` arrive, so
/// everything else counts as deployed.
#[must_use]
pub fn is_local(env: &Environment) -> bool {
    matches!(env, Environment::Development | Environment::Test)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub rate_limit: RateLimitSettings,
    /// Optional: a config without the block doesn't limit the static files.
    #[serde(default)]
    pub file_rate_limit: FileRateLimitSettings,
    pub security: SecuritySettings,
    pub mail: MailSettings,
    /// Optional: a config without the block has no nightly restart.
    #[serde(default)]
    pub nightly_restart: NightlyRestartSettings,
}

impl Settings {
    /// Reads and validates the `settings:` block of the loaded config.
    ///
    /// # Errors
    /// The block is missing, has unknown keys, or fails validation.
    pub fn from_config(config: &Config) -> Result<Self> {
        Self::from_value(config.settings.as_ref())
    }

    /// Split from [`Self::from_config`] so tests can feed a bare value:
    /// `loco_rs::config::Config` is `#[non_exhaustive]` and cannot be built here.
    ///
    /// # Errors
    /// See [`Self::from_config`].
    pub fn from_value(value: Option<&serde_json::Value>) -> Result<Self> {
        let value = value.ok_or_else(|| {
            Error::Message(
                "config: `settings:` block is missing (settings.rate_limit, settings.security and settings.mail are required)"
                    .into(),
            )
        })?;
        let settings: Self = serde_json::from_value(value.clone())
            .map_err(|e| Error::Message(format!("config: invalid `settings:` block: {e}")))?;
        settings.rate_limit.validate()?;
        settings.file_rate_limit.validate()?;
        settings.security.validate()?;
        settings.mail.validate()?;
        settings.nightly_restart.validate()?;
        Ok(settings)
    }
}

/// Headers the app sets itself, as opposed to Loco's `secure_headers`
/// middleware, whose static values live under `server.middlewares`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecuritySettings {
    /// Content-Security-Policy for rendered pages. Must contain `{nonce}`
    /// once; `render_page` replaces it per request.
    pub content_security_policy: String,
}

impl SecuritySettings {
    fn validate(&self) -> Result<()> {
        let template = &self.content_security_policy;
        if template.matches(NONCE_PLACEHOLDER).count() != 1 {
            return Err(Error::Message(format!(
                "config: settings.security.content_security_policy must contain `{NONCE_PLACEHOLDER}` exactly once"
            )));
        }
        // Same check the request path performs, done once at boot so a bad
        // template fails the start instead of every page.
        HeaderValue::from_str(&template.replace(NONCE_PLACEHOLDER, "x")).map_err(|e| {
            Error::Message(format!(
                "config: settings.security.content_security_policy is not a valid header value: {e}"
            ))
        })?;
        Ok(())
    }
}

/// Token bucket per visitor IP on every route (static files have their own,
/// [`FileRateLimitSettings`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitSettings {
    pub enable: bool,
    /// Seconds per replenished token: 1 means a sustained 60 requests/minute.
    pub per_second: u64,
    /// Bucket size: how many requests a visitor may make at once before the
    /// sustained rate applies.
    pub burst: u32,
    /// A second, stricter bucket on the `/api/auth` routes alone, inside the
    /// one above: a request there spends a token from both. `register` mails
    /// any address and `login` takes a password, so those routes get a
    /// fraction of the site-wide rate. Off together with `enable`.
    pub auth: RouteRateLimit,
}

impl RateLimitSettings {
    fn validate(&self) -> Result<()> {
        if !self.enable {
            return Ok(());
        }
        if self.per_second == 0 || self.burst == 0 {
            return Err(Error::Message(
                "config: settings.rate_limit.per_second and .burst must be at least 1 when enabled"
                    .into(),
            ));
        }
        if self.auth.per_second == 0 || self.auth.burst == 0 {
            return Err(Error::Message(
                "config: settings.rate_limit.auth.per_second and .burst must be at least 1 when enabled"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// The numbers of a bucket on one group of routes; the same meaning as in
/// [`RateLimitSettings`].
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRateLimit {
    pub per_second: u64,
    pub burst: u32,
}

/// Token bucket per visitor on the static files the router's fallback
/// serves (the bundle, the stylesheet, fonts, images;
/// src/controllers/not_found.rs). Generous, so no person reaches it, only a
/// script pulling the same files over and over. Off when the block is
/// missing, and off locally, where the files are rechecked on every view.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileRateLimitSettings {
    pub enable: bool,
    /// Milliseconds per replenished token: 100 means a sustained 10 files a
    /// second. Milliseconds, not seconds: one page load fetches several
    /// files.
    #[serde(default)]
    pub per_millisecond: u64,
    /// How many files a visitor may fetch at once before the sustained rate
    /// applies.
    #[serde(default)]
    pub burst: u32,
}

impl FileRateLimitSettings {
    fn validate(&self) -> Result<()> {
        if self.enable && (self.per_millisecond == 0 || self.burst == 0) {
            return Err(Error::Message(
                "config: settings.file_rate_limit.per_millisecond and .burst must be at least 1 when enabled"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// Outgoing mail (src/mailers/): the sender on every message the app sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailSettings {
    /// The sender as SMTP sees it: `Name <address>` or a bare address.
    /// Without it Loco sends as `System <system@example.com>`, which most
    /// SMTP services refuse or file as spam. Gmail replaces any sender with
    /// the account's own address; a transactional service wants a sender it
    /// has verified.
    pub from: String,
}

impl MailSettings {
    /// Parsed the way Loco's mailer parses it when it sends (lettre's
    /// `Mailbox`): a sender it would refuse fails the boot here, instead of
    /// every mail later.
    fn validate(&self) -> Result<()> {
        if let Err(e) = self.from.parse::<Mailbox>() {
            return Err(Error::Message(format!(
                "config: settings.mail.from is not a sender like `Name <address>` ({e}): {:?}",
                self.from
            )));
        }
        Ok(())
    }
}

/// Nightly restart (src/maintenance.rs): the server stops itself once a day
/// at `hour` o'clock in `zone` and the service manager starts it again.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightlyRestartSettings {
    pub enable: bool,
    /// Hour of the day, 0 to 23, in `zone`.
    pub hour: u32,
    /// IANA zone name (`UTC`, `Europe/Zagreb`); a name chrono-tz does not
    /// know refuses the boot.
    pub zone: Tz,
}

impl Default for NightlyRestartSettings {
    fn default() -> Self {
        Self {
            enable: false,
            hour: 3,
            zone: Tz::UTC,
        }
    }
}

impl NightlyRestartSettings {
    fn validate(&self) -> Result<()> {
        if self.enable && self.hour > 23 {
            return Err(Error::Message(format!(
                "config: settings.nightly_restart.hour must be 0 to 23, got {}",
                self.hour
            )));
        }
        Ok(())
    }

    /// The restart as a time of day. `None` only for an hour out of range,
    /// which [`Self::validate`] rejects at boot.
    #[must_use]
    pub fn time(&self) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(self.hour, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const CSP: &str = "default-src 'self'; script-src 'self' 'nonce-{nonce}'";
    const FROM: &str = "SaaS Starter <noreply@example.com>";

    /// A valid block with the three required parts. Each test changes the
    /// one part it is about.
    fn base() -> Value {
        json!({
            "rate_limit": {
                "enable": true,
                "per_second": 1,
                "burst": 120,
                "auth": { "per_second": 30, "burst": 10 }
            },
            "security": { "content_security_policy": CSP },
            "mail": { "from": FROM }
        })
    }

    /// [`base`] with one of its parts left out.
    fn without(part: &str) -> Value {
        let mut block = base();
        block.as_object_mut().expect("an object").remove(part);
        block
    }

    fn parse(block: &Value) -> Result<Settings> {
        Settings::from_value(Some(block))
    }

    #[test]
    fn valid_block_parses() {
        let s = parse(&base()).expect("valid settings");
        assert!(s.rate_limit.enable);
        assert_eq!(s.rate_limit.per_second, 1);
        assert_eq!(s.rate_limit.burst, 120);
        assert_eq!(s.rate_limit.auth.per_second, 30);
        assert_eq!(s.rate_limit.auth.burst, 10);
        assert_eq!(s.security.content_security_policy, CSP);
        assert_eq!(s.mail.from, FROM);
    }

    #[test]
    fn missing_block_is_an_error() {
        let err = Settings::from_value(None).expect_err("must fail");
        assert!(err.to_string().contains("missing"), "{err}");
    }

    #[test]
    fn missing_security_is_an_error() {
        let err = parse(&without("security")).expect_err("security block is required");
        assert!(err.to_string().contains("security"), "{err}");
    }

    #[test]
    fn unknown_key_is_an_error() {
        let mut block = base();
        block["rate_limit"]["behind_cloudflare"] = json!(true);
        let err = parse(&block).expect_err("unknown key must fail");
        assert!(err.to_string().contains("behind_cloudflare"), "{err}");
    }

    #[test]
    fn zero_burst_is_an_error_when_enabled() {
        let mut block = base();
        block["rate_limit"]["burst"] = json!(0);
        let err = parse(&block).expect_err("zero burst must fail");
        assert!(err.to_string().contains("at least 1"), "{err}");
    }

    #[test]
    fn file_rate_limit_is_off_when_the_block_is_missing() {
        let s = parse(&base()).expect("valid");
        assert!(!s.file_rate_limit.enable);
    }

    #[test]
    fn file_rate_limit_parses_and_may_be_off_without_numbers() {
        let mut block = base();
        block["file_rate_limit"] = json!({ "enable": true, "per_millisecond": 100, "burst": 300 });
        let s = parse(&block).expect("valid");
        assert!(s.file_rate_limit.enable);
        assert_eq!(s.file_rate_limit.per_millisecond, 100);
        assert_eq!(s.file_rate_limit.burst, 300);

        block["file_rate_limit"] = json!({ "enable": false });
        assert!(!parse(&block).expect("valid").file_rate_limit.enable);
    }

    #[test]
    fn file_rate_limit_needs_positive_numbers_when_enabled() {
        let mut block = base();
        block["file_rate_limit"] = json!({ "enable": true, "per_millisecond": 0, "burst": 300 });
        let err = parse(&block).expect_err("zero rate must fail");
        assert!(err.to_string().contains("file_rate_limit"), "{err}");
    }

    #[test]
    fn auth_bucket_is_required_and_needs_positive_numbers_when_enabled() {
        let mut block = base();
        block["rate_limit"]
            .as_object_mut()
            .expect("an object")
            .remove("auth");
        let err = parse(&block).expect_err("the auth bucket is not optional");
        assert!(err.to_string().contains("auth"), "{err}");

        let mut block = base();
        block["rate_limit"]["auth"]["burst"] = json!(0);
        let err = parse(&block).expect_err("zero auth burst must fail");
        assert!(err.to_string().contains("rate_limit.auth"), "{err}");
    }

    #[test]
    fn zero_values_are_fine_when_disabled() {
        let mut block = base();
        block["rate_limit"] = json!({
            "enable": false,
            "per_second": 0,
            "burst": 0,
            "auth": { "per_second": 0, "burst": 0 }
        });
        parse(&block).expect("disabled limiter needs no numbers");
    }

    #[test]
    fn csp_without_nonce_placeholder_is_an_error() {
        let mut block = base();
        block["security"]["content_security_policy"] = json!("default-src 'self'");
        let err =
            parse(&block).expect_err("a CSP without {nonce} would never allow the inline scripts");
        assert!(err.to_string().contains("exactly once"), "{err}");
    }

    #[test]
    fn csp_with_control_characters_is_an_error() {
        let mut block = base();
        block["security"]["content_security_policy"] =
            json!("default-src 'self';\nscript-src 'nonce-{nonce}'");
        let err = parse(&block).expect_err("newline cannot go into a header");
        assert!(err.to_string().contains("header value"), "{err}");
    }

    #[test]
    fn mail_sender_is_required() {
        let err = parse(&without("mail")).expect_err("every mail needs a sender");
        assert!(err.to_string().contains("mail"), "{err}");
    }

    #[test]
    fn mail_sender_is_a_display_form_or_a_bare_address() {
        for from in [
            FROM,
            "noreply@example.com",
            "\"Doe, John\" <john@example.com>",
        ] {
            let mut block = base();
            block["mail"]["from"] = json!(from);
            parse(&block).unwrap_or_else(|e| panic!("{from:?} is a sender: {e}"));
        }
        for from in [
            "",
            "SaaS Starter",
            "SaaS Starter <not an address>",
            "SaaS Starter <noreply@example.com",
            "<>",
            "noreply@",
            // The address alone is fine, but a display name with a bare
            // comma is not a mailbox: the mailer would refuse every send.
            "Doe, John <john@example.com>",
        ] {
            let mut block = base();
            block["mail"]["from"] = json!(from);
            let Err(err) = parse(&block) else {
                panic!("{from:?} must be refused");
            };
            assert!(err.to_string().contains("mail.from"), "{err}");
        }
    }

    #[test]
    fn nightly_restart_parses_a_zone_name() {
        let mut block = base();
        block["nightly_restart"] = json!({ "enable": true, "hour": 3, "zone": "Europe/Zagreb" });
        let s = parse(&block).expect("valid settings");
        assert!(s.nightly_restart.enable);
        assert_eq!(s.nightly_restart.hour, 3);
        assert_eq!(s.nightly_restart.zone, chrono_tz::Europe::Zagreb);
    }

    #[test]
    fn nightly_restart_is_off_when_the_block_is_missing() {
        let s = parse(&base()).expect("the block is optional");
        assert!(!s.nightly_restart.enable);
    }

    #[test]
    fn nightly_restart_unknown_zone_is_an_error() {
        let mut block = base();
        block["nightly_restart"] = json!({ "enable": true, "hour": 3, "zone": "Mars/Olympus" });
        let err = parse(&block).expect_err("an unknown zone must fail");
        assert!(err.to_string().contains("Mars/Olympus"), "{err}");
    }

    #[test]
    fn nightly_restart_hour_out_of_range_is_an_error_when_enabled() {
        let mut block = base();
        block["nightly_restart"] = json!({ "enable": true, "hour": 24, "zone": "UTC" });
        let err = parse(&block).expect_err("hour 24 must fail");
        assert!(err.to_string().contains("hour"), "{err}");

        block["nightly_restart"]["enable"] = json!(false);
        parse(&block).expect("a disabled restart needs no valid hour");
    }

    #[test]
    fn only_development_and_the_tests_are_local() {
        assert!(is_local(&Environment::Development));
        assert!(is_local(&Environment::Test));
        assert!(!is_local(&Environment::Production));
        assert!(!is_local(&Environment::Any("staging".into())));
        assert!(!is_local(&Environment::Any("dev-server".into())));
    }
}
