//! Config canary. The five `config/<env>.yaml` files are the security policy
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

use app::settings::{DEV_LOCAL, Settings};
use axum::http::HeaderValue;
use loco_rs::{
    config::Config,
    controller::middleware::{limit_payload::DefaultBodyLimitKind, remote_ip::ClientIpSource},
    environment::Environment,
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
    // A sender: `settings.mail.from` is validated as one at boot.
    ("MAILER_FROM", "SaaS Starter <canary@example.test>"),
];

/// The CSP of everything that is not a page (pages send their own): the
/// files, robots.txt, the JSON API, the not-found line and the 429 page.
const FALLBACK_CSP: &str = "default-src 'none'; style-src 'self'; img-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

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

/// Staging's twin on its own subdomain (docs/dev-server.md).
fn dev_server() -> Environment {
    Environment::Any("dev-server".into())
}

/// The local environment, `LOCO_ENV=dev-local` (src/settings.rs).
fn dev_local() -> Environment {
    Environment::Any(DEV_LOCAL.into())
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

fn config_path(env: &Environment) -> std::path::PathBuf {
    Path::new("config").join(format!("{env}.yaml"))
}

/// The config file as YAML text, `get_env` answered from the placeholders.
fn render(env: &Environment) -> String {
    let path = config_path(env);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: cannot read: {e}", path.display()));
    let template = to_tera_syntax(&path, &content);
    let mut tera = Tera::default();
    tera.register_function("get_env", get_env);
    tera.add_raw_template("config", &template)
        .unwrap_or_else(|e| panic!("{}: not a Tera template: {e}", path.display()));
    tera.render("config", &Context::new())
        .unwrap_or_else(|e| panic!("{}: does not render: {e}", path.display()))
}

fn load(env: &Environment) -> Config {
    serde_yaml::from_str(&render(env))
        .unwrap_or_else(|e| panic!("{}: not a Loco config: {e}", config_path(env).display()))
}

/// The dev server is staging's twin. Rendered with the same placeholders,
/// which stand in for everything the unit sets (`HOST`, `DATABASE_URL`,
/// `JWT_SECRET`, `MAILER_*`), the two files are the same config; only the
/// defaults behind those variables differ.
#[test]
fn dev_server_is_staging_on_its_own_host() {
    let yaml = |env: &Environment| -> serde_yaml::Value {
        serde_yaml::from_str(&render(env)).expect("yaml")
    };
    assert_eq!(
        yaml(&dev_server()),
        yaml(&staging()),
        "config/dev-server.yaml must match config/staging.yaml"
    );
    let raw = std::fs::read_to_string(config_path(&dev_server())).expect("dev-server.yaml");
    assert!(
        raw.contains(r#"name="HOST", default="https://dev.example.com""#),
        "dev-server's own host"
    );
}

/// Loco falls back to `development` when `LOCO_ENV` is unset. With no
/// config by that name, a server started without `LOCO_ENV` refuses to boot
/// instead of running on a developer's settings.
#[test]
fn loco_s_default_environment_has_no_config() {
    let path = Path::new("config").join(format!("{}.yaml", Environment::Development));
    assert!(
        !path.exists(),
        "{}: the local config is {DEV_LOCAL}.yaml",
        path.display()
    );
}

#[rstest]
#[case::dev_local(dev_local())]
// `case::test` would make rstest drop the function without a warning.
#[case::test_env(Environment::Test)]
#[case::dev_server(dev_server())]
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
        ("Content-Security-Policy", FALLBACK_CSP),
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

    // Without the block Loco allows 2 MB; the JSON API needs a few hundred
    // bytes.
    let payload = mw
        .limit_payload
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: limit_payload block missing"));
    assert!(
        matches!(payload.body_limit, DefaultBodyLimitKind::Limit(64_000)),
        "{env}: limit_payload.body_limit is {:?}, agreed 64kb",
        payload.body_limit
    );

    let remote_ip = mw
        .remote_ip
        .as_ref()
        .unwrap_or_else(|| panic!("{env}: remote_ip block missing"));
    assert!(remote_ip.enable, "{env}: remote_ip disabled");

    // The reverse proxy compresses in front of the app; Loco's welcome page
    // is never wanted. Its block must be there and off: without it Loco
    // switches the page on everywhere but production.
    assert!(
        mw.compression.as_ref().is_none_or(|c| !c.enable),
        "{env}: compression must stay off"
    );
    assert!(
        mw.fallback.as_ref().is_some_and(|f| !f.enable),
        "{env}: a `fallback:` block with `enable: false` is required"
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
#[case::dev_local(dev_local(), "no-cache", "target/site")]
#[case::dev_server(dev_server(), "public, max-age=60", "site")]
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
#[case::dev_server(dev_server())]
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
    // The auth API's own bucket: ten calls at once, then one every 30 s.
    assert_eq!(
        settings.rate_limit.auth.burst, 10,
        "{env}: agreed auth burst"
    );
    assert_eq!(
        settings.rate_limit.auth.per_second, 30,
        "{env}: agreed auth refill"
    );
    assert!(
        !settings.security.content_security_policy.contains("ws://"),
        "{env}: the live-reload socket is local only"
    );
}

#[rstest]
#[case::dev_local(dev_local())]
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
fn only_the_online_test_copies_hide_from_search_engines() {
    for env in [
        dev_local(),
        Environment::Test,
        dev_server(),
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
        if env == staging() || env == dev_server() {
            assert_eq!(
                robots,
                Some("noindex, nofollow"),
                "{env} must not be indexed"
            );
        } else {
            assert_eq!(
                robots, None,
                "{env}: only staging and dev-server send X-Robots-Tag"
            );
        }
    }
}

/// The nightly restart (src/maintenance.rs) must be off wherever nothing
/// would restart the process: locally, and in the test harness.
#[rstest]
#[case::dev_local(dev_local(), false)]
#[case::test_env(Environment::Test, false)]
#[case::dev_server(dev_server(), true)]
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

/// The static-file limit (src/controllers/not_found.rs): on where the files
/// are cached and seldom asked for again, off locally, where the dev loop
/// rechecks every file on every view, and in the test harness, which serves
/// none. Generous where it is on: a page loads about ten files.
#[rstest]
#[case::dev_local(dev_local(), false)]
#[case::test_env(Environment::Test, false)]
#[case::dev_server(dev_server(), true)]
#[case::staging(staging(), true)]
#[case::production(Environment::Production, true)]
fn static_files_are_limited_only_when_deployed(#[case] env: Environment, #[case] enabled: bool) {
    let files = Settings::from_config(&load(&env))
        .expect("settings")
        .file_rate_limit;
    assert_eq!(files.enable, enabled, "{env}: file_rate_limit.enable");
    if enabled {
        assert_eq!(files.burst, 300, "{env}: agreed burst");
        assert_eq!(files.per_millisecond, 100, "{env}: agreed refill");
    }
}

/// Every mail the app sends names its sender (`settings.mail.from`,
/// src/mailers/auth.rs): Loco's own default is `System <system@example.com>`,
/// which most SMTP services refuse. Deployed environments read it from
/// `MAILER_FROM`, which the placeholder table stands in for here.
#[rstest]
#[case::dev_local(dev_local(), "SaaS Starter <noreply@example.com>")]
#[case::test_env(Environment::Test, "SaaS Starter <noreply@example.com>")]
#[case::dev_server(dev_server(), "SaaS Starter <canary@example.test>")]
#[case::staging(staging(), "SaaS Starter <canary@example.test>")]
#[case::production(Environment::Production, "SaaS Starter <canary@example.test>")]
fn every_environment_names_the_mail_sender(#[case] env: Environment, #[case] from: &str) {
    let settings = Settings::from_config(&load(&env)).expect("settings");
    assert_eq!(settings.mail.from, from, "{env}: settings.mail.from");
}

/// The request tests count on this: twenty requests pass, the next is a 429.
/// Large enough that the auth tests (register, verify, login, then the call
/// under test) never trip it. The auth API's own bucket takes production's
/// numbers: ten calls pass, the eleventh is a 429 while the site-wide bucket
/// still has tokens.
#[test]
fn test_environment_has_the_burst_the_request_tests_expect() {
    let settings = Settings::from_config(&load(&Environment::Test)).expect("settings");
    assert_eq!(settings.rate_limit.burst, 20);
    assert_eq!(settings.rate_limit.per_second, 1);
    assert_eq!(settings.rate_limit.auth.burst, 10);
    assert_eq!(settings.rate_limit.auth.per_second, 30);
}
