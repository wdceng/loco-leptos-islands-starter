//! Config canary. The four `config/<env>.yaml` files are the security policy
//! (headers, timeout, rate limit, caching, what the visitor IP is read from),
//! and nothing else reads them before a deploy. These tests load each file
//! through Loco's own loader and assert the baseline every environment must
//! keep, plus the deliberate differences between them. A typo fails here
//! instead of on the server; a `cache_control` value that is not a valid
//! header fails here instead of silently becoming Loco's one-year default.
//!
//! Staging and production read their secrets from the environment with no
//! defaults. Setting process environment variables from a multi-threaded
//! test binary is unsound, so instead of Loco's `Config::from_folder` the
//! YAML is rendered here with the same Tera setup Loco uses, except that
//! `get_env` answers from a placeholder table before it looks at the real
//! environment. The result is deserialized into Loco's own `Config`, so
//! every field still goes through Loco's types.

use std::path::Path;

use app::settings::Settings;
use axum::http::HeaderValue;
use loco_rs::{
    config::Config, controller::middleware::remote_ip::ClientIpSource, environment::Environment,
};
use rstest::rstest;
use tera::{Context, Kwargs, State, Tera, TeraResult, Value};

/// The variables staging and production require. Placeholders: nothing
/// connects to them.
const PLACEHOLDERS: &[(&str, &str)] = &[
    ("HOST", "https://example.test"),
    ("DATABASE_URL", "sqlite://canary.sqlite?mode=rwc"),
    ("JWT_SECRET", "canary"),
    ("MAILER_HOST", "smtp.example.test"),
    ("MAILER_USER", "canary"),
    ("MAILER_PASSWORD", "canary"),
];

/// `get_env(name="VAR", default="fallback")` with the placeholder table in
/// front of the process environment. Same semantics as Loco's otherwise.
#[allow(clippy::needless_pass_by_value)]
fn get_env(kwargs: Kwargs, _state: &State<'_>) -> TeraResult<Value> {
    let name: String = kwargs.must_get("name")?;
    let placeholder = PLACEHOLDERS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| (*value).to_owned());
    match placeholder.or_else(|| std::env::var(&name).ok()) {
        Some(value) => Ok(Value::from(value)),
        None => kwargs.get::<Value>("default")?.ok_or_else(|| {
            tera::Error::message(format!(
                "environment variable `{name}` not found and no `default` was given"
            ))
        }),
    }
}

fn staging() -> Environment {
    Environment::Any("staging".into())
}

/// Loco's configs use YAML-safe `<%= expr %>` tags, which its loader turns
/// into Tera's `{{ expr }}` before rendering. That translation is private
/// to Loco; ours only needs the interpolation form, so a statement or
/// comment tag (`<% %>`, `<%# %>`) is refused rather than mis-rendered.
fn to_tera_syntax(path: &Path, content: &str) -> String {
    assert!(
        !content.contains("<% ") && !content.contains("<%#"),
        "{}: only `<%= expr %>` tags are supported by this canary",
        path.display()
    );
    content.replace("<%=", "{{").replace("%>", "}}")
}

fn load(env: &Environment) -> Config {
    let path = Path::new("config").join(format!("{env}.yaml"));
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: cannot read: {e}", path.display()));
    let template = to_tera_syntax(&path, &content);
    let mut tera = Tera::default();
    tera.register_function("get_env", get_env);
    tera.add_raw_template("config", &template)
        .unwrap_or_else(|e| panic!("{}: not a Tera template: {e}", path.display()));
    let rendered = tera
        .render("config", &Context::new())
        .unwrap_or_else(|e| panic!("{}: does not render: {e}", path.display()));
    serde_yaml::from_str(&rendered)
        .unwrap_or_else(|e| panic!("{}: not a Loco config: {e}", path.display()))
}

#[rstest]
#[case::development(Environment::Development)]
// `case::test` would make rstest drop the function without a warning.
#[case::test_env(Environment::Test)]
#[case::staging(staging())]
#[case::production(Environment::Production)]
fn every_environment_keeps_the_security_baseline(#[case] env: Environment) {
    let config = load(&env);
    let server = &config.server;
    let mw = &server.middlewares;

    assert_eq!(
        server.ident.as_deref(),
        Some(""),
        "{env}: server.ident must be empty so X-Powered-By is not sent"
    );

    let headers = mw
        .secure_headers
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: secure_headers block missing"));
    assert!(headers.enable, "{env}: secure_headers disabled");
    assert_eq!(headers.preset, "github", "{env}: secure_headers preset");
    let overrides = headers
        .overrides
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: secure_headers.overrides missing"));
    for (name, value) in [
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "strict-origin-when-cross-origin"),
        (
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=()",
        ),
        ("Cross-Origin-Opener-Policy", "same-origin"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Cross-Origin-Embedder-Policy", "require-corp"),
        (
            "Strict-Transport-Security",
            "max-age=31536000; includeSubDomains; preload",
        ),
    ] {
        assert_eq!(
            overrides.get(name).map(String::as_str),
            Some(value),
            "{env}: override {name}"
        );
    }
    for (name, value) in overrides {
        HeaderValue::from_str(value)
            .unwrap_or_else(|e| panic!("{env}: override {name} is not a valid header: {e}"));
    }

    let timeout = mw
        .timeout_request
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: timeout_request block missing"));
    assert!(timeout.enable, "{env}: timeout_request disabled");
    assert_eq!(timeout.timeout, 15_000, "{env}: timeout_request.timeout ms");

    let remote_ip = mw
        .remote_ip
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: remote_ip block missing"));
    assert!(remote_ip.enable, "{env}: remote_ip disabled");

    // The reverse proxy compresses in front of the app; Loco's welcome page
    // is never wanted.
    assert!(
        mw.compression.as_ref().is_none_or(|c| !c.enable),
        "{env}: compression must stay off"
    );
    assert!(
        mw.fallback.as_ref().is_none_or(|f| !f.enable),
        "{env}: Loco's fallback page must stay off"
    );

    let settings = Settings::from_config(&config)
        .unwrap_or_else(|e| panic!("{env}: settings block rejected: {e}"));
    assert!(settings.rate_limit.enable, "{env}: rate limiting disabled");
    let csp = &settings.security.content_security_policy;
    // Deny by default; every resource kind the page uses is listed on purpose.
    assert!(
        csp.starts_with("default-src 'none';"),
        "{env}: CSP must deny by default"
    );
    for directive in [
        "script-src 'self'",
        "style-src 'self'",
        "img-src 'self'",
        "font-src 'self'",
        "manifest-src 'self'",
        "connect-src 'self'",
        "base-uri 'self'",
        "form-action 'self'",
    ] {
        assert!(csp.contains(directive), "{env}: CSP lacks `{directive}`");
    }
    assert!(
        csp.contains("'wasm-unsafe-eval'"),
        "{env}: CSP must allow the islands bundle"
    );
    assert!(
        csp.contains("frame-ancestors 'none'"),
        "{env}: CSP must forbid framing"
    );
    assert!(!csp.contains("unsafe-inline"), "{env}: CSP allows inline");
}

#[rstest]
#[case::development(Environment::Development, "no-cache", "target/site")]
#[case::staging(staging(), "public, max-age=60", "site")]
#[case::production(Environment::Production, "public, max-age=31536000, immutable", "site")]
fn static_files_are_served_with_the_agreed_cache_policy(
    #[case] env: Environment,
    #[case] cache_control: &str,
    #[case] path: &str,
) {
    let config = load(&env);
    let assets = config
        .server
        .middlewares
        .static_assets
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: static block missing"));
    assert!(assets.enable, "{env}: static disabled");
    assert_eq!(
        assets.folder.uri, "/",
        "{env}: static must be the router fallback"
    );
    assert_eq!(assets.folder.path, Path::new(path), "{env}: static folder");
    let value = assets
        .cache_control
        .as_deref()
        .unwrap_or_else(|| panic!("{env}: static.cache_control missing"));
    assert_eq!(value, cache_control, "{env}: static.cache_control");
    // Loco replaces an invalid value with a one-year default without a word.
    HeaderValue::from_str(value)
        .unwrap_or_else(|e| panic!("{env}: cache_control is not a valid header: {e}"));
}

#[rstest]
#[case::staging(staging())]
#[case::production(Environment::Production)]
fn deployed_environments_key_on_the_proxy_header(#[case] env: Environment) {
    let config = load(&env);
    let mw = &config.server.middlewares;
    let remote_ip = mw.remote_ip.as_ref().expect("remote_ip block");
    assert!(
        matches!(remote_ip.source, ClientIpSource::CfConnectingIp),
        "{env}: the visitor IP comes from the proxy's CF-Connecting-IP header"
    );
    let assets = mw.static_assets.as_ref().expect("static block");
    assert!(
        assets.must_exist,
        "{env}: a deploy without site/ must refuse to boot"
    );
    let settings = Settings::from_config(&config).expect("settings");
    assert_eq!(settings.rate_limit.burst, 120, "{env}: agreed burst");
    assert_eq!(settings.rate_limit.per_second, 1, "{env}: agreed refill");
    assert!(
        !settings.security.content_security_policy.contains("ws://"),
        "{env}: the live-reload socket is development only"
    );
}

#[rstest]
#[case::development(Environment::Development)]
// `case::test` would make rstest drop the function without a warning.
#[case::test_env(Environment::Test)]
fn local_environments_key_on_the_peer_and_allow_live_reload(#[case] env: Environment) {
    let config = load(&env);
    let remote_ip = config
        .server
        .middlewares
        .remote_ip
        .as_ref()
        .expect("remote_ip block");
    assert!(
        matches!(remote_ip.source, ClientIpSource::ConnectInfo),
        "{env}: no proxy locally, the TCP peer is the visitor"
    );
    let settings = Settings::from_config(&config).expect("settings");
    assert!(
        settings
            .security
            .content_security_policy
            .contains("ws://localhost:3001"),
        "{env}: CSP must allow the cargo-leptos reload socket"
    );
}

#[test]
fn only_staging_hides_from_search_engines() {
    for env in [
        Environment::Development,
        Environment::Test,
        staging(),
        Environment::Production,
    ] {
        let config = load(&env);
        let overrides = config
            .server
            .middlewares
            .secure_headers
            .as_ref()
            .and_then(|h| h.overrides.as_ref())
            .expect("overrides");
        let robots = overrides.get("X-Robots-Tag").map(String::as_str);
        if matches!(&env, Environment::Any(name) if name == "staging") {
            assert_eq!(
                robots,
                Some("noindex, nofollow"),
                "staging must not be indexed"
            );
        } else {
            assert_eq!(robots, None, "{env}: only staging sends X-Robots-Tag");
        }
    }
}

/// The nightly restart (src/maintenance.rs) must be off wherever nothing
/// would restart the process: locally, and in the test harness.
#[rstest]
#[case::development(Environment::Development, false)]
#[case::test_env(Environment::Test, false)]
#[case::staging(staging(), true)]
#[case::production(Environment::Production, true)]
fn nightly_restart_runs_only_when_deployed(#[case] env: Environment, #[case] enabled: bool) {
    let settings = Settings::from_config(&load(&env)).expect("settings");
    assert_eq!(
        settings.nightly_restart.enable, enabled,
        "{env}: nightly_restart.enable"
    );
    assert_eq!(settings.nightly_restart.hour, 3, "{env}: agreed hour");
}

/// The request tests count on this: twenty requests pass, the next is a 429.
/// Large enough that the auth tests (register, verify, login, then the call
/// under test) never trip it.
#[test]
fn test_environment_has_the_burst_the_request_tests_expect() {
    let settings = Settings::from_config(&load(&Environment::Test)).expect("settings");
    assert_eq!(settings.rate_limit.burst, 20);
    assert_eq!(settings.rate_limit.per_second, 1);
}
