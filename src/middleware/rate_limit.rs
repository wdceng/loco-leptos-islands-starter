//! `rate_limit`: a token bucket per visitor IP on every route.
//!
//! Applied with `route_layer`, so only matched routes count (pages, the
//! JSON API, `/robots.txt`, the health endpoints); the static assets served
//! by the fallback (bundle, stylesheet, fonts, `404.html`) are never limited.
//!
//! The visitor address comes from the same place Loco's `remote_ip`
//! middleware is configured to read it, so the limiter and the `RemoteIP`
//! extractor can never disagree: behind Cloudflare that is
//! `CF-Connecting-IP`, locally the TCP peer.
//!
//! Numbers live under `settings.rate_limit` in `config/<env>.yaml`
//! (see `crate::settings`). A stricter, separate bucket for a sensitive
//! route (login, password reset) can later be attached to that route alone
//! with Loco's `Routes::layer`, reusing `VisitorIp` and `too_many_requests`
//! from here.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
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

use crate::settings::{RateLimitSettings, Settings};

/// The 429 page. Kept next to this file, not under `public/`, because
/// cargo-leptos would otherwise ship it as a static `/429.html` with a
/// literal `{wait}` in it.
const PAGE: &str = include_str!("rate_limit.html");

/// How often idle buckets are dropped from the limiter's map.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Where the visitor address is read from. Derived from `remote_ip`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum KeySource {
    /// The TCP peer (`ConnectInfo`): development and tests.
    Peer,
    /// `CF-Connecting-IP`, with the peer as fallback: behind Cloudflare.
    CfConnectingIp,
}

/// Maps the `remote_ip` config to a key source, refusing combinations that
/// would silently put every visitor into one bucket.
///
/// # Errors
/// The `remote_ip` source is one this limiter does not support, or the
/// environment is a deployed one (anything but development/test) while the
/// key would be the peer address, which behind a reverse proxy is always the
/// proxy itself.
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
    let local = matches!(env, Environment::Development | Environment::Test);
    if !local && source == KeySource::Peer {
        return Err(
            "rate_limit: outside development/test the peer address is the reverse proxy, \
             so every visitor would share one bucket; enable remote_ip with source CfConnectingIp"
                .into(),
        );
    }
    Ok(source)
}

/// tower_governor key extractor: one bucket per visitor IP.
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
        Ok(from_header
            .or_else(from_peer)
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)))
    }
}

/// The 429 page with the wait filled in. `wait_time` is whole seconds and
/// can be 0 right before a token replenishes; the page says at least 1.
#[must_use]
pub fn render_page(wait_secs: u64) -> String {
    PAGE.replace("{wait}", &wait_secs.max(1).to_string())
}

/// Turns the limiter's rejection into the HTML 429, keeping the headers
/// tower_governor already computed (`retry-after`, `x-ratelimit-*`).
#[must_use]
pub fn too_many_requests(err: GovernorError) -> Response<Body> {
    match err {
        GovernorError::TooManyRequests { wait_time, headers } => {
            tracing::info!(wait_secs = wait_time, "rate limit exceeded");
            let mut res =
                (StatusCode::TOO_MANY_REQUESTS, Html(render_page(wait_time))).into_response();
            if let Some(headers) = headers {
                res.headers_mut().extend(headers);
            }
            if !res.headers().contains_key(header::RETRY_AFTER) {
                res.headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from(wait_time));
            }
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

/// The `rate_limit` entry for `Hooks::middlewares`. Carries the outcome of
/// the boot-time validation so a bad config fails loudly in `apply` instead
/// of silently disabling the limiter.
#[derive(Debug, Clone)]
pub struct RateLimit {
    inner: std::result::Result<Configured, String>,
}

impl RateLimit {
    #[must_use]
    pub fn from_context(ctx: &AppContext) -> Self {
        Self {
            inner: Self::configure(ctx),
        }
    }

    fn configure(ctx: &AppContext) -> std::result::Result<Configured, String> {
        let settings = ctx
            .shared_store
            .get::<Settings>()
            .ok_or(
                "rate_limit: settings missing from the shared store (after_context did not run)",
            )?
            .rate_limit;
        let key_source = key_source(
            ctx.config.server.middlewares.remote_ip.as_ref(),
            &ctx.environment,
        )?;
        Ok(Configured {
            settings,
            key_source,
        })
    }
}

impl MiddlewareLayer for RateLimit {
    fn name(&self) -> &'static str {
        "rate_limit"
    }

    fn is_enabled(&self) -> bool {
        // A broken config must reach `apply` and fail the boot.
        self.inner.as_ref().map_or(true, |c| c.settings.enable)
    }

    fn config(&self) -> serde_json::Result<serde_json::Value> {
        match &self.inner {
            Ok(c) => serde_json::to_value(c),
            Err(e) => Ok(serde_json::json!({ "error": e })),
        }
    }

    fn apply(&self, app: Router<AppContext>) -> Result<Router<AppContext>> {
        let c = self.inner.as_ref().map_err(|e| Error::Message(e.clone()))?;
        let config = GovernorConfigBuilder::default()
            .per_second(c.settings.per_second)
            .burst_size(c.settings.burst)
            .key_extractor(VisitorIp {
                source: c.key_source.clone(),
            })
            .use_headers()
            .finish()
            .ok_or_else(|| Error::Message("rate_limit: per_second and burst must be > 0".into()))?;
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

        let layer = GovernorLayer::new(config).error_handler(too_many_requests);
        Ok(app.route_layer(layer))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::*;

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

    #[test]
    fn page_fills_in_the_wait() {
        let page = render_page(7);
        assert!(page.contains("in 7 seconds"), "{page}");
        assert!(!page.contains("{wait}"));
        assert!(
            render_page(0).contains("in 1 seconds"),
            "0 must round up to 1"
        );
    }

    #[tokio::test]
    async fn rejection_is_an_html_429_with_retry_headers() {
        let mut given = HeaderMap::new();
        given.insert("x-ratelimit-limit", HeaderValue::from_static("3"));
        given.insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        let res = too_many_requests(GovernorError::TooManyRequests {
            wait_time: 5,
            headers: Some(given),
        });

        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(res.headers()[header::RETRY_AFTER], "5");
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
        assert!(body.contains("in 5 seconds"));
    }

    #[test]
    fn retry_after_is_added_when_governor_gave_no_headers() {
        let res = too_many_requests(GovernorError::TooManyRequests {
            wait_time: 2,
            headers: None,
        });
        assert_eq!(res.headers()[header::RETRY_AFTER], "2");
    }

    fn remote_ip(enable: bool, source: ClientIpSource) -> RemoteIpMiddleware {
        RemoteIpMiddleware { enable, source }
    }

    #[test]
    fn key_source_follows_remote_ip() {
        let staging = Environment::Any("staging".into());
        assert_eq!(
            key_source(None, &Environment::Development),
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
            &Environment::Development,
        )
        .expect_err("XFF is not supported");
        assert!(err.contains("not supported"), "{err}");
    }
}
