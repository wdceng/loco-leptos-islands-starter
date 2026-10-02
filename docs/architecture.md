# Architecture and Decisions

Why things are built the way they are. To get started, read `README.md`
instead.

Versions: Loco 1.2, Leptos 0.8, Sea-ORM 2.0, Tailwind v4, and stable Rust
1.94 or newer (edition 2024), which Loco 1.2 and Sea-ORM 2.0.4 need.

## One Crate, Two Builds

The crate builds twice: natively with `ssr` as the Loco server, and for
`wasm32-unknown-unknown` with `hydrate` as the small browser bundle that
wakes the islands.

Server-only crates (Loco, tokio, axum, Sea-ORM, the rate limiter) are
`optional` in `Cargo.toml`, pulled in by `ssr` only. Server-only modules in
`src/lib.rs` sit behind `#[cfg(feature = "ssr")]`. If something won't
compile for wasm32, it leaked past that gate, which is why CI builds the
browser half alone.

## How a Page Is Served

1. Loco's router gets the request. Controller routes win. The rest falls
   through to the app's own fallback, `src/controllers/not_found.rs`, which
   serves the files or a real 404.
2. Loco's middleware runs on everything, the fallback too. Routes and misses
   also pass the `rate_limit` layer. Static files don't.
3. `src/controllers/home.rs` builds a `PageMeta` and calls `render_page` in
   `src/render.rs`.
4. `render_page` makes a nonce, streams the shell (`src/views/layout.rs`)
   around the page and sets the page's CSP with that nonce.
5. The browser loads `/pkg/app.js` and the wasm, and `hydrate_islands()` in
   `src/lib.rs` wakes only the `#[island]` components. There's no
   client-side router, so every navigation is a full request.

Leptos meets Loco in `src/app.rs` and `src/render.rs`. In `app.rs`,
`after_context` starts Leptos's task executor. leptos_axum only does that in
its own router helper, which we skip because Loco owns the routes.
`after_context` also loads `LeptosOptions` from `[package.metadata.leptos]`
in `Cargo.toml` or the `LEPTOS_*` variables into Loco's shared store for
the controllers.

## The Stack

**Rust + Loco** - Rails-style framework on Axum and tower, generated from
the Rest API starter (`loco new`: "Rest API (with DB and user auth)",
SQLite, async jobs, no assets). That's Loco's SaaS starter code minus the
Tera pages, which Leptos replaces. Loco brings per-environment YAML config,
the middleware stack, Sea-ORM with migrations, JWT auth, the mailer,
workers, the CLI (`start`, `routes`, `middleware`, `doctor`, `db`, `task`)
and tracing. Loco controllers own every URL: each page controller renders
one Leptos page, and the auth controller answers JSON under `/api/auth`.

**Leptos (islands mode)** - Views are type-checked Rust components rendered
to HTML on the server. Pages are static HTML by default. Only `#[island]`
components hydrate, so the wasm holds just those, not the whole site. None ships, but
the wiring is done (`islands` feature, `hydrate_islands()` in `src/lib.rs`,
`<HydrationScripts options islands=true/>` in the shell), so your first
island in `src/islands.rs` just works. Until then the bundle is only the
loader.

**Tailwind CSS v4** - Utility classes in `view!` macros. cargo-leptos runs
the standalone Tailwind binary, so there's no Node or npm. Only classes you
use reach the stylesheet, which sits next to the islands bundle. The font
is Inter, self-hosted. Colours are Tailwind's default palette, untouched,
plus one brand colour and the role tokens `primary`, `surface`, `ink`,
`ink-muted`, `line` and `card` in `style/tailwind.css`. Views use only the tokens, never a scale
step like `slate-200`, so the palette changes in one place.

**Hashed asset names** - `LEPTOS_HASH_FILES=true cargo leptos build --release`
names the bundle and stylesheet by content hash and writes `hash.txt`, which
`src/assets.rs` reads at boot to link them. Production's one-year cache
depends on it, so every release is built this way. `Cargo.toml` leaves
hashing off on purpose: the watch loop only hashes its first build, then
goes stale.

**SQLite** - Sea-ORM over SQLx, SQLite driver only (`sqlx-postgres` was
dropped on purpose). Each environment has its own `app_<env>.sqlite`:
`?mode=rwc` in the URL creates it, `auto_migrate` applies the schema at
boot. Loco sets the PRAGMAs: WAL, `synchronous = NORMAL`, foreign keys on,
5 s busy timeout. The `-wal` and `-shm` files are part of the database, so
back up with `sqlite3 <file> ".backup <copy>"`, never by copying the file
alone.

## What Is in the App

"From" says where an area comes from: Loco's Rest API starter as generated,
or this template. Nothing the starter generated was removed. Where the
template changed a starter file, the column names the change.

| Area | From | Where | Notes |
|------|------|-------|-------|
| Pages | This template | `src/controllers/home.rs`, `src/controllers/robots.rs`, `src/views/` | `/` renders `views/home.rs` in the shell (`views/layout.rs`) through `render.rs`, which adds the CSP nonce. `/robots.txt` is plain text: `Allow: /` in production, `Disallow: /` elsewhere |
| Health | Loco | Loco, via `AppRoutes::with_default_routes()` in `src/app.rs` | `/_ping`, `/_health` and `/_readiness`, the last two checking the database and queue. Rate limited like any route |
| Users and auth | Loco starter; the `/api/auth` rate limit is this template's | `src/models/users.rs`, `src/controllers/auth.rs` | JSON API under `/api/auth`: `register`, `verify/{token}`, `login`, `forgot`, `reset`, `current`, `magic-link`, `magic-link/{token}`, `resend-verification-mail`. JWT bearer tokens, 7-day expiry |
| Not found | This template | `src/controllers/not_found.rs`, `src/views/not_found.rs` | The router's fallback. Serves the `static` block's folder with its cache header, and anything else as a 404 with `no-store`: the not-found page for an HTML request, one line of text otherwise |
| Mail | Loco starter; the sender setting, link origin and name escaping are this template's | `src/mailers/auth/` | Welcome, forgot-password and magic-link templates, text and HTML, sent over the SMTP in `config/<env>.yaml`. The sender, `settings.mail.from`, is parsed at boot the way the mailer parses it. Links start with `server.host`. The templates escape the registrant's name themselves |
| Background | Loco starter | `src/workers/`, `src/tasks/` | Starter examples: a download worker and a `user_create` CLI task |
| Nightly restart | This template | `src/maintenance.rs` | Staging and production stop once a day (`settings.nightly_restart`), systemd starts them again. Never in development or test |
| Migrations | Loco starter | `migration/` | Sea-ORM migrations, applied at boot (`auto_migrate: true`) |
| Config | Loco starter (development, test, production); `staging.yaml` and the typed settings are this template's | `config/<env>.yaml` | Typed settings (`rate_limit`, `security`, `mail`, `nightly_restart`) in `src/settings.rs` |

## Security

`cargo loco middleware` shows which Loco middlewares are on for the current
`LOCO_ENV`.

### Reverse Proxy and the Visitor IP

Loco speaks plain HTTP. TLS and compression are the reverse proxy's job, so
the `compression` middleware is off.

The reference deployment (`production.md`) is Cloudflare in front of Caddy,
so `config/staging.yaml` and `config/production.yaml` set
`remote_ip.source: CfConnectingIp` and the rate limiter keys on the same
header. Loco 1.2 trusts exactly one source, with no list of trusted proxies.

It's set in config, not code, but it's enforced: the limiter won't boot a
deployed environment keyed on the TCP peer, which behind a proxy is the
proxy. For another proxy, change `remote_ip.source` and extend `KeySource`
in `src/middleware/rate_limit.rs` (see "Without Cloudflare" in
`production.md`). `tests/config.rs` and the limiter's tests pin the current
mapping.

### Layers

| Layer | Implementation | Provided by | Status |
|-------|----------------|-------------|--------|
| Security headers | `secure_headers` with the `github` preset and overrides, plus a per-page nonce CSP (below) | Loco + this project | on |
| Request body limit | `limit_payload`, 64 KB everywhere. API calls are a few hundred bytes, Loco's default is 2 MB. A bigger body gets a 413 before any handler parses it | Loco | on |
| Request timeout | `timeout_request`, 15 s everywhere. A hung handler gets a 408 instead of holding a connection. Well under the reference CDN's 100 s origin limit | Loco | on |
| Panic isolation | `catch_panic` middleware | Loco | on (Loco default) |
| Static files, ETag | The app's own fallback, inside the whole middleware stack, plus Loco's `etag` | Loco + this project | on |
| Authentication | JWT (`auth.jwt` in config), passwords hashed by Loco, e-mail verification, magic links, reset tokens | Loco | on |
| Secrets | `JWT_SECRET` and `MAILER_*` (host, user, password, sender) come from the environment. Production has no defaults and won't boot without them. Staging has public placeholders, so `LOCO_ENV` alone boots it | this project | on |
| Outbound TLS (mailer) | Loco's mailer is `lettre` with RusTLS | Loco | on |
| Rate limiting | `rate_limit` middleware (`src/middleware/rate_limit.rs`), since Loco has none. A `tower_governor` token bucket per visitor IP, or per /64 network for IPv6 so one machine can't rotate addresses. Keyed like Loco's `remote_ip`: `CF-Connecting-IP` behind the reference CDN, the TCP peer locally. Tuned per environment in `settings.rate_limit`. Misses get their own bucket with the same numbers. Static files are exempt. The 429 is an HTML page with `Retry-After`, rounded up so it's never 0, linking the stylesheet resolved at boot. `/api/auth` gets a second bucket (`rate_limit::auth_bucket`, `settings.rate_limit.auth`) because it mails any address (`register`) or takes a password. Deployed, it allows ten calls at once, then one per 30 s, inside the site-wide limit | this project | on |

Not used:

- `cors`: no cross-origin callers.
- `static` and `fallback`: Loco's file serving and welcome page. `src/app.rs`
  takes both out and puts the app's own fallback first.
- `powered_by`: Loco's `X-Powered-By` middleware turns itself off when
  `server.ident` is `""`, as in all four configs.

### HTTP Security Headers

Two sources, both in `config/*.yaml`.

**Static headers** come from Loco's `secure_headers` middleware under
`server.middlewares`. The `github` preset sets Content-Security-Policy,
Strict-Transport-Security, X-Content-Type-Options, X-Frame-Options,
X-Download-Options, X-Permitted-Cross-Domain-Policies and X-Xss-Protection.
Seven overrides apply everywhere, an eighth on staging:

| Header | Why it's an override |
|--------|----------------------|
| X-Frame-Options | Preset says `sameorigin`. `DENY` matches the CSP's `frame-ancestors 'none'` |
| Strict-Transport-Security | Preset sends a bare `max-age`. Ours is the preload form, `max-age=31536000; includeSubDomains; preload`. The `preload` token is harmless until the apex domain is submitted at https://hstspreload.org, a one-time step once every subdomain serves https. Behind a CDN the zone's HSTS setting overwrites it at the edge, so keep them equal. A header scan then shows the zone's value |
| Referrer-Policy | Added: `strict-origin-when-cross-origin` |
| Permissions-Policy | Added: camera, microphone, geolocation off |
| Cross-Origin-Opener-Policy | Added: `same-origin` |
| Cross-Origin-Resource-Policy | Added: `same-origin`, so the app's files load only on its own pages |
| Cross-Origin-Embedder-Policy | Added: `require-corp`, as pages load nothing cross-origin. With COOP this gives cross-origin isolation, the Spectre-class defence. An embed from another domain (map, video, payment widget) would need CORP or CORS opt-in, or this header dropped |
| X-Robots-Tag | Staging only: `noindex, nofollow`, so a test copy stays out of search results even through an inbound link. `robots.txt` already disallows crawling there |

**The page CSP** is a template in `settings.security.content_security_policy`,
filled per request by `render_page`. Leptos stamps a nonce on every inline
script it emits (the islands loader, the dev live-reload hook), and the same
nonce goes into `script-src 'self' 'nonce-…' 'wasm-unsafe-eval'`, so
`'unsafe-inline'` isn't needed. The policy starts at `default-src 'none'` and lists each
resource kind the page uses: scripts, styles, images, fonts, the manifest,
fetches. Anything new stays blocked until its directive is added on purpose.

Loco's middleware only adds a header the response lacks, so the page CSP
wins on pages, the not-found page included. Everything else (the JSON API,
files, `robots.txt`, the not-found line, the 429 page) gets the
`Content-Security-Policy` override under `secure_headers`:
`default-src 'none'`, only the app's own stylesheet, images and fonts, no
scripts, no framing. It replaces the `github` preset's CSP, which allows
scripts from any https origin and inline styles.

Development and test add the `cargo leptos watch` websocket to the page
CSP's `connect-src`. Staging and production are identical.

**The config canary**: `tests/config.rs` loads all four configs and pins,
per environment, the headers, fallback CSP, timeout, body limit, rate limit,
cache policy and page CSP shape.

## Environments

| `LOCO_ENV` | Database | Secrets | Static files | Detail |
|------------|----------|---------|--------------|--------|
| `development` | `app_development.sqlite` in the repo | defaults in the file | `target/site`, `no-cache` | `development.md` |
| `test` | `app_test.sqlite`, recreated per run | defaults in the file | none | `testing.md` |
| `staging` | `/var/lib/app-stg/app_staging.sqlite` (`StateDirectory`) | placeholder defaults, the environment may override | `site/`, 60 s, adds `X-Robots-Tag: noindex, nofollow` | `staging.md` |
| `production` | `/var/lib/app-prod/app_production.sqlite` (`StateDirectory`) | required from the environment | `site/`, one year, immutable (needs the `LEPTOS_HASH_FILES=true` build) | `production.md` |

Secrets are environment variables, read by the `get_env` helper in the
YAML. Loco loads no `.env` file. Getting them to the process is the
deploy's job: each environment has its own
`staging.secrets.env` or `production.secrets.env` in the repo root,
git-ignored, uploaded by every deploy as a root-only `secrets.env` that
systemd loads before it drops to the service user. Mail links use `server.host`
as written, with no bind port added (locally the port is part of it), so the
unit also sets `HOST` to the public origin.

The server has two Cargo profiles in `Cargo.toml`. `release` is for
production, fully optimised with fat LTO. `staging` inherits it but links
with thin LTO in parallel, builds incrementally and keeps line tables, so
`pretty_backtrace` prints function names. The browser half always uses
`wasm-release`.

## Nightly Restart

A fresh process every day is cheap insurance against whatever a
long-running one piles up.

`src/maintenance.rs` is spawned once per server start from
`Hooks::after_routes` in `src/app.rs`, not `after_context`, which Loco's CLI
runs before the logger is up and `create_app` runs again. It reads
`settings.nightly_restart` (`enable`, `hour`, `zone`). An unknown zone or an
hour above 23 fails the boot. It sleeps until the next `hour` o'clock in
`zone`, then sends SIGTERM to its own process.

That's what `systemctl stop` sends, so Loco shuts down gracefully: no new
connections, requests in flight finish, `on_shutdown` runs, exit status 0.
`Restart=always` in the systemd unit (`production.md`) starts it again.
Without that, the stop is just a stop. Without signals, or if raising one
fails, the task exits the process directly.

Development and test never restart, whatever the config says. The stop
would kill `cargo leptos watch` with nothing to restart it, and tests boot
the app inside the test process. The shipped configs keep it off there too,
and `tests/config.rs` pins that.

Daylight saving is handled. A time that doesn't exist on the spring-forward
night makes the loop wait an hour and look again. One that happens twice in
autumn takes the later.

## The Checks CI Runs

```sh
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo leptos build &&
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

The last one lints the browser half alone. It fails if a server-only crate
leaked past the `ssr` gate, and it's the only pass that sees `hydrate`
code. `cargo audit` runs too. The advisories ignored on purpose, each with
its reason, are in `.cargo/audit.toml`.

`cargo loco` here is a cargo alias (`loco = "run --"` in
`.cargo/config.toml`) for the app binary, not the `loco` CLI. You only need
the CLI for `loco new`, which is done.

## Conventions

- Make a component an `#[island]` only if it really needs the browser.
  Everything else stays a server-rendered `#[component]`.
- Islands go in `src/islands.rs`, the only module in `lib.rs` without an
  `ssr` gate, since both halves compile it. Everything under `views/` is
  server only. Check both halves with `cargo clippy --all-targets` and the
  wasm clippy above.
- Keep an island a thin wrapper and pass content in as `children`, which
  render on the server only and stay out of the wasm. `style/tailwind.css`
  sets Leptos's `<leptos-island>` / `<leptos-children>` wrappers to
  `display: contents` so they don't disturb grid or flex layouts.
- The surface colour is repeated as hex in the `theme-color` tag and the
  web manifest, so change it there too.
- No `style=` attributes: the CSP is `style-src 'self'`, and the request
  test fails on one. With `default-src 'none'`, a new resource kind (video,
  iframe, worker) needs its own directive in all four configs before it
  loads.
- The document shell is `shell` in `src/views/layout.rs`. Pages fill only
  `<main>`. Per-page values travel in `PageMeta`. `APP_NAME` there is the
  one place the app's display name lives. The head already has favicons, the
  manifest, `theme-color`, the home-screen tags and the safe-area viewport.
  Open Graph tags wait for a 1200×630 image.
- Everything in `public/` is copied into `site/` and served, in production
  with a one-year cache. Rename a file when its content changes. No
  `.DS_Store` or scratch files.
- The 429 page (`src/middleware/rate_limit.html`) is static HTML outside
  Leptos, reusing the shell's Tailwind classes so the scanner keeps them.
  `public/404.html` is never served, since the not-found page is rendered,
  but it stays because the boot checks for it where `static.must_exist` is
  on.
- Escape visitor input in mail templates (`{{ name | escape }}` in
  `html.t`). Loco's Tera only escapes templates named `.html`, `.htm` or
  `.xml`.
- Don't hand-edit the generated Sea-ORM entities in
  `src/models/_entities/`. Model logic goes in `src/models/*.rs`.
- A config change to policy (headers, timeout, rate limit, cache, CSP)
  must update `tests/config.rs` too.

## Known Gaps

- A request that reaches the origin directly, around the CDN, can forge
  `CF-Connecting-IP` until the firewall from `production.md` is set up.
- The 429 on `/api/auth` is the same HTML page as elsewhere, not JSON. API
  clients should read `Retry-After`.
- Magic-link login is limited to two e-mail domains (`EMAIL_DOMAIN_RE` in
  `src/controllers/auth.rs`).
- The password-reset mail links to `/reset#<token>`, a page the template
  doesn't have. Loco's starter leaves it to the front end and only
  `POST /api/auth/reset` exists, so the link gets the not-found page.
  Browsers never send what's after the `#`, so that page must be an island
  that reads the token and posts the new password to the API. The verify
  and magic-link mails link straight to JSON API routes, so clicking them
  shows JSON.
- The `ts-rs` TypeScript export in `src/dtos/` is commented out until a
  TypeScript consumer exists.
- No island ships. The first one is yours.

## Crates

Loco already brings the HTTP middleware (tower-http), the mailer (lettre +
RusTLS), tracing setup and config loading, so none of that is declared
here.

**Declared by Loco's Rest API starter**

| Crate | Purpose |
|-------|---------|
| `loco-rs` | The framework, default features (`auth`, `cli`, `with-db`, `db-sqlite`, `worker`, `cache_inmem`) |
| `axum` | Handler and router types in controllers |
| `serde` / `serde_json` | Serialization. The only two crates shared with the browser half |
| `tokio` | Async runtime. `time` for the rate limiter's housekeeping |
| `async-trait` | Loco's `Hooks` trait needs it |
| `tracing` / `tracing-subscriber` | Log macros and subscriber, initialised by Loco |
| `regex` | E-mail domain check in the auth controller |
| `sea-orm` / `migration` | ORM with the SQLite driver, and the migrations crate |
| `chrono` | Timestamps on the users model, the footer year at render time |
| `validator` | Model validation. Pinned to 0.20 like `loco-rs`: its prelude supplies the `Validate` derive, and a second version breaks the trait |
| `uuid` | User `pid` and API key |
| `ts-rs` | TypeScript bindings for `src/dtos/`, export commented out for now |
| `include_dir` | Embeds the mail templates |

**Added by this template**

| Crate | Purpose |
|-------|---------|
| `leptos` | Components and SSR. `islands` feature on, `ssr` on the server build, `hydrate` on the browser build |
| `leptos_axum` | Turns a rendered page into an HTTP response in controllers (`ssr` only) |
| `any_spawner` | The task executor Leptos renders on, started once at boot (`ssr` only) |
| `wasm-bindgen` / `console_error_panic_hook` | Browser bindings and panic reporting (`hydrate` only) |
| `tower_governor` | Token-bucket rate limiting behind the `rate_limit` middleware |
| `governor` | Names the limiter's config types where `rate_limit` builds each bucket. Already in via `tower_governor` (`ssr` only) |
| `tower-http` | File serving and the cache header in the fallback, the same pieces Loco's `static` middleware uses. Already in via Loco (`ssr` only) |
| `lettre` | Only its address parser, so `settings.mail.from` is parsed at boot the way the mailer parses it when sending. Already in via Loco (`ssr` only) |
| `chrono-tz` | Reads the restart hour in a fixed IANA zone. The `serde` feature turns the config's zone name into a `Tz` at boot (`ssr` only) |
| `nix` | Sends SIGTERM to the app at the restart hour, so Loco's graceful shutdown runs (unix only, `ssr` only) |

**Tests only**

| Crate | Purpose |
|-------|---------|
| `serial_test`, `rstest`, `insta` | Serialised request tests, parameterised cases, snapshots |
| `tera` / `serde_yaml` | The config canary renders the YAML with placeholder secrets, leaving the process environment alone |

## Reference

- Loco docs: https://loco.rs/docs/
- Loco's guide for coding agents: https://loco.rs/AGENTS.md
- Loco + Leptos SSR showcase, the integration recipe this template follows:
  https://github.com/loco-rs/loco/discussions/1748
- Leptos islands: https://book.leptos.dev/islands.html
- cargo-leptos: https://book.leptos.dev/ssr/21_cargo_leptos.html
