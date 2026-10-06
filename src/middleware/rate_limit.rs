//! `rate_limit`: a token bucket per visitor IP on every route.
//!
//! Applied with `route_layer`, so only matched routes count (pages, the
//! JSON API, `/robots.txt`, the health endpoints). The router's fallback
//! (`controllers::not_found`) serves the static files, which are never
//! limited because a page loads several of them, and answers a miss, which
//! has a bucket of its own with the same numbers ([`site_bucket`]).
//!
//! The visitor address comes from the same place Loco's `remote_ip`
//! middleware is configured to read it, so the limiter and the `RemoteIP`
//! extractor can never disagree: behind Cloudflare that is
//! `CF-Connecting-IP`, locally the TCP peer. The bucket is per IPv4
//! address, but per /64 network for IPv6 ([`bucket_key`]): one machine
//! usually owns a whole /64 and could otherwise take a fresh bucket for
//! every request.
//!
//! Numbers live under `settings.rate_limit` in `config/<env>.yaml`
//! (see `crate::settings`). Every bucket is built here, the same way:
//! [`site_bucket`] for the routes and the misses, and [`auth_bucket`] for
//! the auth API's stricter one (`settings.rate_limit.auth`), built in
//! `after_context` and attached to `/api/auth` alone with Loco's
//! `Routes::layer` in `controllers::auth`.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderValue, Request, Response, StatusCode, header},
    response::{Html, IntoResponse},
};
use governor::middleware::StateInformationMiddleware;
use loco_rs::{
    Error, Result,
    app::AppContext,
    controller::middleware::{
        MiddlewareLayer,
        remote_ip::{ClientIpSource, RemoteIpMiddleware},
    },
    environment::Environment,
};
use serde::Serialize;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder, key_extractor::KeyExtractor,
};

use crate::{
    app::stored,
    assets::Assets,
    settings::{RateLimitSettings, Settings, is_local},
    views::too_many_requests,
};

/// How often idle buckets are dropped from the limiter's map.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Where the visitor address is read from. Derived from `remote_ip`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum KeySource {
    /// The TCP peer (`ConnectInfo`): dev-local and the tests.
    Peer,
    /// `CF-Connecting-IP`, with the peer as fallback: behind Cloudflare.
    CfConnectingIp,
}

/// Maps the `remote_ip` config to a key source, refusing combinations that
/// would silently put every visitor into one bucket.
///
/// # Errors
/// The `remote_ip` source is one this limiter does not support, or the
/// environment is a deployed one (anything but dev-local and the tests,
/// `settings::is_local`) while the key would be the peer address, which
/// behind a reverse proxy is always the proxy itself.
pub fn key_source(
    remote_ip: Option<&RemoteIpMiddleware>,
    env: &Environment,
) -> std::result::Result<KeySource, String> {
    let source = match remote_ip {
        Some(m) if m.enable => match m.source {
            ClientIpSource::CfConnectingIp => KeySource::CfConnectingIp,
            ClientIpSource::ConnectInfo => KeySource::Peer,
            ref other => {
                return Err(format!(
                    "rate_limit: remote_ip.source {other:?} is not supported; \
                     use CfConnectingIp (behind Cloudflare) or ConnectInfo (no proxy)"
                ));
            }
        },
        _ => KeySource::Peer,
    };
    if !is_local(env) && source == KeySource::Peer {
        return Err(
            "rate_limit: outside dev-local and the tests the peer address is the reverse proxy, \
             so every visitor would share one bucket; enable remote_ip with source CfConnectingIp"
                .into(),
        );
    }
    Ok(source)
}

/// The bucket a visitor address belongs to. An IPv4 address is its own
/// bucket. An IPv6 address is keyed by its /64 network, the high 64 bits:
/// that is what one machine or one household is usually given, and every
/// address in it is theirs, so keyed by the full address one server could
/// send each request from a new address and never run out of tokens. An
/// IPv4 address written as IPv6 (`::ffff:a.b.c.d`) is the IPv4 address.
#[must_use]
pub fn bucket_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !u128::from(u64::MAX))),
        },
    }
}

/// tower_governor key extractor: one bucket per visitor, see [`bucket_key`].
#[derive(Debug, Clone)]
pub struct VisitorIp {
    pub source: KeySource,
}

impl KeyExtractor for VisitorIp {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> std::result::Result<IpAddr, GovernorError> {
        let from_header = match self.source {
            KeySource::CfConnectingIp => req
                .headers()
                .get("cf-connecting-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<IpAddr>().ok()),
            KeySource::Peer => None,
        };
        let from_peer = || {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(addr)| addr.ip())
        };
        // Never a 500: a request with no usable address shares one bucket.
        Ok(bucket_key(
            from_header
                .or_else(from_peer)
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        ))
    }
}

/// The 429 page for one stylesheet path, the URL path `Assets` resolved at
/// boot (a fixed plain name would be a miss in production, where the names
/// are hashed). Rendered by Leptos once per limiter
/// (views/too_many_requests.rs); its `{wait}` stays in it for [`fill_wait`].
#[must_use]
pub fn page_for(stylesheet: &str) -> String {
    too_many_requests::document(stylesheet.to_owned())
}

/// A number of seconds in words: "1 second", "2 seconds".
#[must_use]
pub fn seconds(n: u64) -> String {
    if n == 1 {
        "1 second".into()
    } else {
        format!("{n} seconds")
    }
}

/// `page` (from [`page_for`]) with the wait filled in, in words. The page
/// says at least 1 second.
#[must_use]
pub fn fill_wait(page: &str, wait_secs: u64) -> String {
    page.replace(too_many_requests::WAIT, &seconds(wait_secs.max(1)))
}

/// Turns the limiter's rejection into the HTML 429 built from `page`,
/// keeping the `x-ratelimit-*` headers tower_governor computed. The wait is
/// rounded up: tower_governor rounds it down to whole seconds, so a bucket
/// that refills once a second would always say 0, and `Retry-After: 0`
/// sends a client straight back into another refusal. One second more is
/// never too early.
#[must_use]
pub fn too_many_requests(page: &str, err: GovernorError) -> Response<Body> {
    match err {
        GovernorError::TooManyRequests { wait_time, headers } => {
            let wait_secs = wait_time.saturating_add(1);
            tracing::info!(wait_secs, "rate limit exceeded");
            let mut res = (
                StatusCode::TOO_MANY_REQUESTS,
                Html(fill_wait(page, wait_secs)),
            )
                .into_response();
            if let Some(headers) = headers {
                res.headers_mut().extend(headers);
            }
            // Both arrive with tower_governor's rounded-down value.
            let wait = HeaderValue::from(wait_secs);
            res.headers_mut().insert(header::RETRY_AFTER, wait.clone());
            res.headers_mut().insert("x-ratelimit-after", wait);
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }
        // `VisitorIp` never produces these; fall back to the crate's own text.
        other => other.into(),
    }
}

/// What `cargo loco middleware -c` prints for `rate_limit`.
#[derive(Debug, Clone, Serialize)]
struct Configured {
    #[serde(flatten)]
    settings: RateLimitSettings,
    key_source: KeySource,
}

/// The `rate_limit` entry for `Hooks::middlewares`: a [`site_bucket`] on
/// every route. It reads the settings as the stack is built, so a bad
/// config fails the boot in `apply` instead of silently disabling the
/// limiter.
pub struct RateLimit {
    ctx: AppContext,
}

impl RateLimit {
    #[must_use]
    pub fn from_context(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
}

impl MiddlewareLayer for RateLimit {
    fn name(&self) -> &'static str {
        "rate_limit"
    }

    fn is_enabled(&self) -> bool {
        // Settings that cannot be read must reach `apply` and fail the boot.
        stored::<Settings>(&self.ctx).map_or(true, |settings| settings.rate_limit.enable)
    }

    fn config(&self) -> serde_json::Result<serde_json::Value> {
        let key_source = key_source(
            self.ctx.config.server.middlewares.remote_ip.as_ref(),
            &self.ctx.environment,
        );
        match (stored::<Settings>(&self.ctx), key_source) {
            (Ok(settings), Ok(key_source)) => serde_json::to_value(Configured {
                settings: settings.rate_limit,
                key_source,
            }),
            (Err(e), _) => Ok(serde_json::json!({ "error": e.to_string() })),
            (_, Err(e)) => Ok(serde_json::json!({ "error": e })),
        }
    }

    fn apply(&self, app: Router<AppContext>) -> Result<Router<AppContext>> {
        Ok(match site_bucket(&self.ctx)? {
            Some(bucket) => app.route_layer(bucket),
            None => app,
        })
    }
}

/// A limiter as a tower layer: one token bucket per visitor
/// ([`VisitorIp`]), the HTML 429 on refusal.
pub type Bucket = GovernorLayer<VisitorIp, StateInformationMiddleware, Body>;

/// A new bucket with the site-wide numbers of `settings.rate_limit`, `None`
/// when that block is off: [`RateLimit`] puts one on every route,
/// `controllers::not_found` one of its own on the misses.
///
/// # Errors
/// The settings or the assets are missing from the shared store, the key
/// source cannot be derived (see [`key_source`]), or no such limiter can
/// be built.
pub fn site_bucket(ctx: &AppContext) -> Result<Option<Bucket>> {
    let site = stored::<Settings>(ctx)?.rate_limit;
    if !site.enable {
        return Ok(None);
    }
    bucket(ctx, Duration::from_secs(site.per_second), site.burst).map(Some)
}

/// The auth API's stricter bucket (`settings.rate_limit.auth`), `None` when
/// the limiter is off: built in `after_context`, attached to `/api/auth`
/// alone in `controllers::auth`. A request there spends a token from both
/// buckets, and the outer, site-wide layer writes `x-ratelimit-limit` and
/// `x-ratelimit-remaining` last, so a response from `/api/auth` reports the
/// site-wide numbers; `retry-after` on a 429 is from the bucket that
/// refused.
///
/// # Errors
/// As [`site_bucket`].
pub fn auth_bucket(ctx: &AppContext) -> Result<Option<Bucket>> {
    let limits = stored::<Settings>(ctx)?.rate_limit;
    if !limits.enable {
        return Ok(None);
    }
    bucket(
        ctx,
        Duration::from_secs(limits.auth.per_second),
        limits.auth.burst,
    )
    .map(Some)
}

/// A bucket for the static files the router's fallback serves
/// (`controllers::not_found`), with the generous numbers of
/// `settings.file_rate_limit`: no person reaches it, only a script pulling
/// the same files over and over. `None` when that block is off.
///
/// # Errors
/// As [`site_bucket`].
pub fn files_bucket(ctx: &AppContext) -> Result<Option<Bucket>> {
    let files = stored::<Settings>(ctx)?.file_rate_limit;
    if !files.enable {
        return Ok(None);
    }
    bucket(
        ctx,
        Duration::from_millis(files.per_millisecond),
        files.burst,
    )
    .map(Some)
}

/// One token bucket per visitor, as a tower layer: `burst` requests at
/// once, then one every `period`. The visitor address is read where the
/// config says ([`key_source`]), and the HTML 429 on refusal links the
/// stylesheet `Assets` resolved at boot.
///
/// # Errors
/// The assets are missing from the shared store, the key source cannot be
/// derived, or `period` or `burst` is 0.
fn bucket(ctx: &AppContext, period: Duration, burst: u32) -> Result<Bucket> {
    let source = key_source(
        ctx.config.server.middlewares.remote_ip.as_ref(),
        &ctx.environment,
    )
    .map_err(Error::Message)?;
    let stylesheet = stored::<Assets>(ctx)?.stylesheet;
    let config = GovernorConfigBuilder::default()
        .period(period)
        .burst_size(burst)
        .key_extractor(VisitorIp { source })
        .use_headers()
        .finish()
        .ok_or_else(|| {
            Error::Message("rate_limit: the interval and the burst must be above 0".into())
        })?;
    let config = Arc::new(config);

    // Housekeeping: drop buckets nobody has touched for a while, so the
    // map does not grow with every IP ever seen. Holds only a Weak: the
    // task ends when the limiter is dropped (tests boot many apps).
    let limiter = Arc::downgrade(config.limiter());
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(CLEANUP_INTERVAL);
        tick.tick().await; // the first tick completes immediately
        loop {
            tick.tick().await;
            let Some(limiter) = limiter.upgrade() else {
                break;
            };
            limiter.retain_recent();
            tracing::debug!(buckets = limiter.len(), "rate_limit: dropped idle buckets");
        }
    });

    // The page with its stylesheet, built once; each refusal fills in only
    // the wait.
    let page: Arc<str> = page_for(&stylesheet).into();
    Ok(GovernorLayer::new(config).error_handler(move |err| too_many_requests(&page, err)))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::*;
    use crate::settings::DEV_LOCAL;

    const HEADER_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
    const PEER_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

    fn request(header: Option<&str>, peer: bool) -> Request<()> {
        let mut builder = Request::builder().uri("/");
        if let Some(value) = header {
            builder = builder.header("cf-connecting-ip", value);
        }
        let mut req = builder.body(()).expect("valid request");
        if peer {
            req.extensions_mut()
                .insert(ConnectInfo(SocketAddr::new(PEER_IP, 4321)));
        }
        req
    }

    fn extract(source: KeySource, header: Option<&str>, peer: bool) -> IpAddr {
        VisitorIp { source }
            .extract(&request(header, peer))
            .expect("extractor is infallible")
    }

    #[test]
    fn cloudflare_source_prefers_the_header() {
        assert_eq!(
            extract(KeySource::CfConnectingIp, Some("203.0.113.9"), true),
            HEADER_IP
        );
    }

    #[test]
    fn cloudflare_source_falls_back_to_the_peer() {
        assert_eq!(extract(KeySource::CfConnectingIp, None, true), PEER_IP);
        assert_eq!(
            extract(KeySource::CfConnectingIp, Some("not an ip"), true),
            PEER_IP
        );
    }

    #[test]
    fn peer_source_ignores_the_header() {
        assert_eq!(extract(KeySource::Peer, Some("203.0.113.9"), true), PEER_IP);
    }

    fn v6(address: &str) -> IpAddr {
        IpAddr::V6(address.parse().expect("valid IPv6"))
    }

    #[test]
    fn ipv6_addresses_share_the_bucket_of_their_64_network() {
        let a = bucket_key(v6("2001:db8:1:2:aaaa:bbbb:cccc:dddd"));
        let b = bucket_key(v6("2001:db8:1:2::1"));
        assert_eq!(a, b, "one /64, one bucket");
        assert_eq!(a, v6("2001:db8:1:2::"), "keyed by the network");
        assert_ne!(
            a,
            bucket_key(v6("2001:db8:1:3::1")),
            "the next /64 is another visitor"
        );
    }

    #[test]
    fn ipv4_keeps_its_own_bucket_also_when_written_as_ipv6() {
        assert_eq!(bucket_key(HEADER_IP), HEADER_IP);
        assert_eq!(bucket_key(v6("::ffff:203.0.113.9")), HEADER_IP);
    }

    #[test]
    fn the_extractor_keys_an_ipv6_visitor_by_the_network() {
        assert_eq!(
            extract(
                KeySource::CfConnectingIp,
                Some("2001:db8:1:2:aaaa:bbbb:cccc:dddd"),
                true
            ),
            v6("2001:db8:1:2::")
        );
    }

    #[test]
    fn no_address_at_all_shares_one_bucket() {
        assert_eq!(
            extract(KeySource::Peer, None, false),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
        assert_eq!(
            extract(KeySource::CfConnectingIp, None, false),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
    }

    /// The 429 page as the test environment builds it: plain names.
    fn page() -> String {
        page_for("/pkg/app.css")
    }

    #[test]
    fn page_fills_in_the_wait() {
        let filled = fill_wait(&page(), 7);
        assert!(filled.contains("in 7 seconds."), "{filled}");
        assert!(!filled.contains("{wait}"));
        assert!(
            fill_wait(&page(), 0).contains("in 1 second."),
            "0 must round up to 1, in the singular"
        );
    }

    #[test]
    fn page_links_the_resolved_stylesheet() {
        let page = page_for("/pkg/app.ab12cd.css");
        assert!(
            page.contains(r#"href="/pkg/app.ab12cd.css""#),
            "the hashed name, as resolved at boot:\n{page}"
        );
        assert!(!page.contains("{stylesheet}"));
    }

    #[test]
    fn seconds_take_the_right_number() {
        assert_eq!(seconds(1), "1 second");
        assert_eq!(seconds(2), "2 seconds");
        assert_eq!(seconds(21), "21 seconds");
    }

    #[tokio::test]
    async fn rejection_is_an_html_429_with_retry_headers() {
        // tower_governor's 5 is a wait of 5.x seconds rounded down.
        let mut given = HeaderMap::new();
        given.insert("x-ratelimit-limit", HeaderValue::from_static("3"));
        given.insert("x-ratelimit-after", HeaderValue::from_static("5"));
        given.insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        let res = too_many_requests(
            &page(),
            GovernorError::TooManyRequests {
                wait_time: 5,
                headers: Some(given),
            },
        );

        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(res.headers()[header::RETRY_AFTER], "6", "rounded up");
        assert_eq!(res.headers()["x-ratelimit-after"], "6", "rounded up");
        assert_eq!(res.headers()["x-ratelimit-limit"], "3");
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            res.headers()[header::CONTENT_TYPE]
                .to_str()
                .expect("ascii")
                .starts_with("text/html")
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .expect("body");
        let body = String::from_utf8(body.to_vec()).expect("utf-8");
        assert!(body.contains("Too many requests"));
        assert!(
            body.contains("in 6 seconds"),
            "the page says what the header says"
        );
    }

    #[test]
    fn retry_after_is_added_when_governor_gave_no_headers() {
        let res = too_many_requests(
            &page(),
            GovernorError::TooManyRequests {
                wait_time: 2,
                headers: None,
            },
        );
        assert_eq!(res.headers()[header::RETRY_AFTER], "3");
    }

    /// A bucket that refills once a second leaves less than a second to
    /// wait, which tower_governor reports as 0: "retry now".
    #[test]
    fn a_wait_under_a_second_is_never_zero() {
        let mut given = HeaderMap::new();
        given.insert("x-ratelimit-after", HeaderValue::from_static("0"));
        given.insert(header::RETRY_AFTER, HeaderValue::from_static("0"));
        let res = too_many_requests(
            &page(),
            GovernorError::TooManyRequests {
                wait_time: 0,
                headers: Some(given),
            },
        );
        assert_eq!(res.headers()[header::RETRY_AFTER], "1");
        assert_eq!(res.headers()["x-ratelimit-after"], "1");
    }

    fn remote_ip(enable: bool, source: ClientIpSource) -> RemoteIpMiddleware {
        RemoteIpMiddleware { enable, source }
    }

    #[test]
    fn key_source_follows_remote_ip() {
        let staging = Environment::Any("staging".into());
        assert_eq!(
            key_source(None, &Environment::Any(DEV_LOCAL.into())),
            Ok(KeySource::Peer)
        );
        assert_eq!(
            key_source(
                Some(&remote_ip(true, ClientIpSource::ConnectInfo)),
                &Environment::Test
            ),
            Ok(KeySource::Peer)
        );
        assert_eq!(
            key_source(
                Some(&remote_ip(true, ClientIpSource::CfConnectingIp)),
                &staging
            ),
            Ok(KeySource::CfConnectingIp)
        );
    }

    #[test]
    fn key_source_refuses_a_shared_bucket_when_deployed() {
        let err = key_source(None, &Environment::Production).expect_err("must refuse");
        assert!(err.contains("reverse proxy"), "{err}");
        let err = key_source(
            Some(&remote_ip(false, ClientIpSource::CfConnectingIp)),
            &Environment::Any("staging".into()),
        )
        .expect_err("disabled remote_ip means peer");
        assert!(err.contains("CfConnectingIp"), "{err}");
    }

    #[test]
    fn key_source_refuses_unsupported_sources() {
        let err = key_source(
            Some(&remote_ip(true, ClientIpSource::RightmostXForwardedFor)),
            &Environment::Any(DEV_LOCAL.into()),
        )
        .expect_err("XFF is not supported");
        assert!(err.contains("not supported"), "{err}");
    }
}
