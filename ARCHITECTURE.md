# Architecture and Decisions

The reference companion to `README.md`: how a request is served, why each part of the stack was chosen, what the security setup does, and what every crate is for. Nothing here is needed to get started; it is here for when you want to know why.

Versions: Rust stable 1.85 or newer (edition 2024), Loco 1.1, Leptos 0.8, Sea-ORM 2.0, Tailwind v4.

## One Crate, Two Builds

The same crate compiles twice. Natively with the `ssr` feature it becomes the Loco server. For `wasm32-unknown-unknown` with the `hydrate` feature it becomes the small browser bundle that wakes up the islands. Every server-only dependency (Loco, tokio, axum, Sea-ORM, the rate limiter) is `optional` in `Cargo.toml` and pulled in by `ssr` only, and every server-only module in `src/lib.rs` is behind `#[cfg(feature = "ssr")]`. If something fails to compile for wasm32, it leaked out of that gate; CI builds the browser half alone to catch exactly that.

## How a Page Is Served

1. The request hits Loco's router. A controller route wins; anything else falls through to the `static` middleware, which serves the cargo-leptos output (`target/site` locally, `site/` on a server).
2. On routes only, Loco's middleware stack runs together with the project's `rate_limit` layer, one token bucket per visitor IP. Static files are never rate limited.
3. The page controller (`src/controllers/home.rs`) builds a `PageMeta` and calls `render_page` in `src/render.rs`.
4. `render_page` generates a nonce, streams the document shell (`src/views/layout.rs`) around the page component, and sets the page's Content-Security-Policy with that nonce.
5. The browser loads `/pkg/app.js` and the wasm bundle; `hydrate_islands()` in `src/lib.rs` wakes only the `#[island]` components. There is no client-side router: every navigation is a full request.

Where Leptos meets Loco is `src/app.rs` and `src/render.rs`. In `app.rs`, `after_context` starts Leptos's task executor (leptos_axum only does that inside its own router helper, which this template bypasses because Loco owns the routes), loads `LeptosOptions` from `[package.metadata.leptos]` in `Cargo.toml` or the `LEPTOS_*` environment variables, and parks them in Loco's shared store for the controllers.

## The Stack

**Rust + Loco** - Rails-style framework on top of Axum and tower, generated from the `SaaS` starter. Loco supplies the app skeleton: per-environment YAML config, the middleware stack, Sea-ORM with migrations, JWT auth, the mailer, background workers, the CLI (`start`, `routes`, `middleware`, `doctor`, `db`, `task`) and tracing setup. Loco controllers own every URL: page controllers render one Leptos page each, the auth controller answers JSON under `/api/auth`.

**Leptos (islands mode)** - Views are type-checked Rust components rendered to HTML on the server. Pages are plain static HTML by default. Only components marked `#[island]` are hydrated in the browser, so the WebAssembly bundle contains just those components, not the whole site. The template ships no island: the wiring is complete (`islands` feature, `hydrate_islands()` in `src/lib.rs`, `<HydrationScripts options islands=true/>` in the shell), so the first `#[island]` you add hydrates without further setup. Until then the bundle is only the loader.

**Tailwind CSS v4** - Utility classes written directly in `view!` macros. cargo-leptos runs the Tailwind standalone binary as part of the build, so there is no Node or npm; only classes actually used end up in the stylesheet, which is emitted next to the islands bundle. Inter font family, self-hosted.

**Hashed asset names** - A release is built with `LEPTOS_HASH_FILES=true cargo leptos build --release`. cargo-leptos then names the bundle and stylesheet after their content hash and writes `hash.txt`, which the app reads at boot (`src/assets.rs`) to link the right names. Production's one-year cache depends on this, so a release must always be built that way. Nothing in `Cargo.toml` turns hashing on, deliberately: the watch loop hashes only its first build and would go stale afterwards.

**SQLite** - Sea-ORM over SQLx, SQLite driver only (`sqlx-postgres` was dropped on purpose). Each environment names its own file (`app_<env>.sqlite`); the `?mode=rwc` in the URL creates it and `auto_migrate` applies the schema on boot. Loco sets the SQLite PRAGMAs at boot (WAL, `synchronous = NORMAL`, foreign keys on, 5 s busy timeout). The `-wal` and `-shm` files next to the database are part of it: back up with `sqlite3 <file> ".backup <copy>"`, never by copying the file alone.

## What Is in the App

| Area | Where | Notes |
|------|-------|-------|
| Pages | `src/controllers/home.rs`, `src/controllers/robots.rs`, `src/views/` | `/` renders `views/home.rs` inside the shell (`views/layout.rs`) through `render.rs`, which adds the per-request CSP nonce. `/robots.txt` is plain text: `Allow: /` in production, `Disallow: /` everywhere else |
| Health | Loco, via `AppRoutes::with_default_routes()` in `src/app.rs` | `/_ping`, `/_health`, `/_readiness` (the last two check the database and queue). Rate-limited like every other route |
| Users and auth | `src/models/users.rs`, `src/controllers/auth.rs` | JSON API under `/api/auth`: `register`, `verify/{token}`, `login`, `forgot`, `reset`, `current`, `magic-link`, `magic-link/{token}`, `resend-verification-mail`. JWT bearer tokens, 7-day expiry |
| Mail | `src/mailers/auth/` | Welcome, forgot-password and magic-link templates (text and HTML). Sent through SMTP from `config/<env>.yaml` |
| Background | `src/workers/`, `src/tasks/` | Starter examples: a download worker and a `user_create` CLI task |
| Nightly restart | `src/maintenance.rs` | Staging and production stop themselves once a day (`settings.nightly_restart`: hour and zone in `config/<env>.yaml`) and systemd starts them again; development and test never do |
| Migrations | `migration/` | Sea-ORM migrations, applied at boot (`auto_migrate: true`) |
| Config | `config/<env>.yaml` | development, test, staging, production. Typed app settings (`rate_limit`, `security`) in `src/settings.rs` |

## Security

`cargo loco middleware` shows which Loco middlewares are on for the current `LOCO_ENV`.

### Reverse Proxy and the Visitor IP

Loco listens on plain HTTP; TLS and compression belong to a reverse proxy, which is why the `compression` middleware is off. `config/staging.yaml` and `config/production.yaml` set `remote_ip.source: CfConnectingIp`, and the rate limiter keys on the same header, because the reference deployment in `DEPLOYMENT.md` is Cloudflare in front of Caddy. That is a property of the config, not the code, but it is enforced: the limiter refuses to boot a deployed environment keyed on the TCP peer, which behind any proxy is the proxy itself. With a different proxy, change `remote_ip.source` and extend `KeySource` in `src/middleware/rate_limit.rs` (see "Without Cloudflare" in `DEPLOYMENT.md`); `tests/config.rs` and the limiter's tests pin the current mapping.

### Layers

| Layer | Implementation | Provided by | Status |
|-------|----------------|-------------|--------|
| Security headers | `secure_headers` middleware, `github` preset plus overrides, and a per-page nonce CSP (see below) | Loco + this project | on |
| Request body limit | `limit_payload` middleware | Loco | on (Loco default) |
| Request timeout | `timeout_request` middleware, 15 s in every environment: a hung handler is cancelled with 408 instead of holding a connection; well under a CDN's origin limit (the reference CDN allows 100 s) | Loco | on |
| Panic isolation | `catch_panic` middleware | Loco | on (Loco default) |
| Static files, ETag | `static` and `etag` middlewares. `compression` stays off on purpose: the reverse proxy compresses | Loco | on |
| Authentication | JWT (`auth.jwt` in config), passwords hashed by Loco, e-mail verification, magic links, reset tokens | Loco | on |
| Secrets | `JWT_SECRET` and `MAILER_*` are read from the environment. Production has no defaults and refuses to boot without them; staging has placeholder defaults so it boots with `LOCO_ENV` alone | this project | on |
| Outbound TLS (mailer) | Loco's mailer is `lettre` with RusTLS | Loco | on |
| Rate limiting | `rate_limit` middleware (`src/middleware/rate_limit.rs`): a `tower_governor` token bucket per visitor IP, keyed by the same source as Loco's `remote_ip` (`CF-Connecting-IP` behind the reference CDN, the TCP peer locally), tuned per environment under `settings.rate_limit` in `config/*.yaml`; only routes count, static assets are exempt; 429 is an HTML page with `Retry-After`. Loco has no built-in limiter | this project | on |

Not used: `cors` (no cross-origin callers), `fallback` (Loco's welcome page; the static `404.html` serves instead) and `powered_by` (Loco's `X-Powered-By` middleware disables itself when `server.ident` is `""`, as it is in all four configs).

### HTTP Security Headers

Two sources, both in `config/*.yaml`:

**Static headers** come from Loco's `secure_headers` middleware under `server.middlewares`. The `github` preset sets Content-Security-Policy, Strict-Transport-Security, X-Content-Type-Options, X-Frame-Options, X-Download-Options, X-Permitted-Cross-Domain-Policies and X-Xss-Protection. Seven overrides apply in every environment, an eighth on staging:

| Header | Why it is an override |
|--------|----------------------|
| X-Frame-Options | Preset says `sameorigin`; `DENY` matches the CSP's `frame-ancestors 'none'` |
| Strict-Transport-Security | Preset sends a bare `max-age`; replaced with the preload form, `max-age=31536000; includeSubDomains; preload`. The `preload` token has no effect until the apex domain is submitted at https://hstspreload.org, a one-time step once every subdomain serves https; until then it is harmless. Behind a CDN the zone's HSTS setting overwrites this header at the edge and must say the same |
| Referrer-Policy | Not in the preset: `strict-origin-when-cross-origin` |
| Permissions-Policy | Not in the preset: camera, microphone, geolocation off |
| Cross-Origin-Opener-Policy | Not in the preset: `same-origin` |
| Cross-Origin-Resource-Policy | Not in the preset: `same-origin`, the app's files load only on its own pages |
| Cross-Origin-Embedder-Policy | Not in the preset: `require-corp`, pages load nothing cross-origin. Together with COOP this makes pages cross-origin isolated (the Spectre-class defence). An embed from another domain (map, video, payment widget) would need that resource to opt in via CORP or CORS, or this header dropped |
| X-Robots-Tag | Staging only: `noindex, nofollow`, so a test copy never appears in search results even through an inbound link (`robots.txt` already disallows crawling outside production). Absent in development, test and production; `tests/config.rs` pins both sides |

**The page CSP** is a template under `settings.security.content_security_policy`, filled per request by `render_page` in `src/render.rs`. Leptos stamps a nonce on every inline script it emits (the islands loader, the dev live-reload hook); the same nonce goes into `script-src 'self' 'nonce-…' 'wasm-unsafe-eval'`, so no `'unsafe-inline'` is needed. The policy starts with `default-src 'none'` and lists every resource kind the page uses (scripts, styles, images, fonts, the manifest, fetches); a new kind of resource is blocked until its directive is added on purpose. Loco's middleware only adds a header the response does not already carry, which is why the page CSP wins on pages while the preset CSP remains the fallback for the JSON API, assets, `robots.txt` and the static 404 and 429 pages. Development and test add the `cargo leptos watch` websocket to `connect-src`; staging and production are identical.

A CDN in front of the app may inject its own HSTS, so a header scan of a proxied host shows the zone setting, not the origin's value.

**The config canary**: `tests/config.rs` loads all four config files and pins the headers, timeout, rate limit, cache policy and CSP shape per environment, so a config change that alters policy must update that test too.

## Environments

| `LOCO_ENV` | Database | Secrets | Static files | Detail |
|------------|----------|---------|--------------|--------|
| `development` | `app_development.sqlite` in the repo | defaults in the file | `target/site`, `no-cache` | `DEVELOPMENT.md` |
| `test` | `app_test.sqlite`, recreated per run | defaults in the file | none | `TESTING.md` |
| `staging` | `app_staging.sqlite` next to the binary | placeholder defaults; the environment may override | `site/`, 60 s; adds `X-Robots-Tag: noindex, nofollow` | `DEPLOYMENT.md` |
| `production` | `app_production.sqlite` next to the binary | required from the environment | `site/`, one year, immutable (requires the `LEPTOS_HASH_FILES=true` build) | `DEPLOYMENT.md` |

The environment is picked by `LOCO_ENV`. Secrets are environment variables, read through the `get_env` helper inside the YAML; Loco loads no `.env` file. How they reach the process is up to the deploy; the reference systemd unit in `DEPLOYMENT.md` sets them.

## Nightly Restart

A fresh process every day is cheap insurance against whatever a long-running one accumulates. `src/maintenance.rs` is spawned once per server start from `Hooks::after_routes` in `src/app.rs` (not `after_context`, which Loco's CLI runs before the logger is up and `create_app` runs again). It reads `settings.nightly_restart` (`enable`, `hour`, `zone`, typed in `src/settings.rs`; an unknown zone name or an hour above 23 refuses the boot), sleeps until the next `hour` o'clock in `zone`, and then sends SIGTERM to its own process. That is the signal `systemctl stop` sends, so Loco's graceful shutdown runs: no new connections, requests in flight finish, `on_shutdown` runs, exit status 0. The restart itself is `Restart=always` in the systemd unit (`DEPLOYMENT.md`); without it the stop is just a stop. On a platform without signals, or if raising one fails, the task exits the process directly.

Development and test never restart, whatever the config says: the stop would kill the `cargo leptos watch` server with nothing to restart it, and the test harness boots the app inside the test process. The shipped configs keep the block off locally as well, and `tests/config.rs` pins that. Daylight saving is handled: a time that does not exist on the spring-forward night makes the loop wait an hour and look again, and a time that happens twice in autumn takes the later instance.

## The Checks CI Runs

```sh
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo leptos build &&
cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

The last one compiles the browser half alone and fails if a server-only crate leaked out of the `ssr` feature gate. `cargo audit` runs as well; the advisories deliberately ignored, each with its reason, are in `.cargo/audit.toml`.

`cargo loco` in this project is a cargo alias (`.cargo/config.toml`: `loco = "run --"`) that runs the app binary, not the `loco` CLI; the CLI is only needed for `loco new`, which this template has already done.

## Conventions

- Add an `#[island]` only when a component genuinely needs browser interactivity; everything else stays a server-rendered `#[component]`.
- Styling is Tailwind utility classes inside `view!` macros; the Tailwind input file is `style/tailwind.css`. Never a `style=` attribute: the CSP is `style-src 'self'` and the request test fails on one. `default-src 'none'` means a new resource kind (video, iframe, worker) needs its own directive in all four configs before it loads.
- The document shell is `shell` in `src/views/layout.rs`; pages supply only what goes inside `<main>`. Per-page values travel in `PageMeta`. `APP_NAME` there is the one place the app's display name lives. The head already has favicons, the manifest, `theme-color`, the iOS install tags and the safe-area viewport; Open Graph tags wait for a 1200×630 image.
- Everything under `public/` is copied into `site/` and served, in production with a year-long cache: a file whose content changes must change its name, and no `.DS_Store` or scratch files.
- Static pages outside Leptos (`public/404.html`, `src/middleware/rate_limit.html`) use the same Tailwind classes as the shell so the scanner keeps them in the stylesheet.
- Generated Sea-ORM entities in `src/models/_entities/` are not hand-edited; model logic goes in `src/models/*.rs`.
- A config change that alters policy (headers, timeout, rate limit, cache, CSP) must update `tests/config.rs` with it.
- Secrets: production reads `JWT_SECRET` and `MAILER_*` from the environment with no defaults and refuses to boot without them; staging has public placeholder defaults; development and test have values in the file.

## Known Gaps

- The static `404.html` is served with status 200, and in production is cached for the URL that missed, until a real not-found handler replaces Loco's static fallback.
- A request that reaches the origin directly, bypassing the CDN, could forge `CF-Connecting-IP` until Caddy's `trusted_proxies` or a firewall rule is configured (`DEPLOYMENT.md`).
- No stricter per-route rate limit on login and password reset yet; the hook for one is described in `src/middleware/rate_limit.rs`.
- Magic-link login is limited to two e-mail domains (`EMAIL_DOMAIN_RE` in `src/controllers/auth.rs`).
- The `ts-rs` TypeScript export in `src/dtos/` is commented out until a TypeScript consumer exists.
- No island ships; the first one is yours.

## Crates

Loco already ships the HTTP middleware (tower-http), the mailer (lettre + RusTLS), tracing initialisation and config loading, so none of those are declared here. Every server-only crate is `optional` and pulled in by the `ssr` feature, so the browser build never sees it.

**Declared by the Loco SaaS starter**

| Crate | Purpose |
|-------|---------|
| `loco-rs` | Application framework, default features (`auth`, `cli`, `with-db`, `worker`, `cache_inmem`) |
| `axum` | Handler and router types used in controllers |
| `serde` / `serde_json` | Serialization; the only two crates shared with the browser half |
| `tokio` | Async runtime; `time` for the rate limiter's housekeeping |
| `async-trait` | Required by Loco's `Hooks` trait |
| `tracing` / `tracing-subscriber` | Log macros and subscriber (initialised by Loco) |
| `regex` | E-mail domain check in the auth controller |
| `sea-orm` / `migration` | ORM with the SQLite driver, and the migrations crate |
| `chrono` | Timestamps on the users model; the footer year at render time |
| `validator` | Model validation. Pinned to the version `loco-rs` uses (0.20): the `Validate` derive comes through Loco's prelude and a second version breaks the trait |
| `uuid` | User `pid` and API key |
| `ts-rs` | TypeScript bindings for `src/dtos/`. The export is commented out until a TypeScript consumer exists |
| `include_dir` | Embeds the mail templates |

**Added by this template**

| Crate | Purpose |
|-------|---------|
| `leptos` | Components and SSR; `islands` feature on, `ssr` on the server build, `hydrate` on the browser build |
| `leptos_axum` | Turns a rendered page into an HTTP response in controllers (`ssr` only) |
| `any_spawner` | The task executor Leptos renders on, started once at boot (`ssr` only) |
| `wasm-bindgen` / `console_error_panic_hook` | Browser bindings and panic reporting (`hydrate` only) |
| `tower_governor` | Token-bucket rate limiting behind the `rate_limit` middleware |

**Tests only**

| Crate | Purpose |
|-------|---------|
| `serial_test`, `rstest`, `insta` | Serialised request tests, parameterised cases, snapshots |
| `tera` / `serde_yaml` | The config canary renders the YAML with placeholder secrets instead of touching the process environment |

## Reference

- Loco + Leptos SSR showcase, the integration recipe this template follows: https://github.com/loco-rs/loco/discussions/1748
- Leptos islands: https://book.leptos.dev/islands.html
- cargo-leptos: https://book.leptos.dev/ssr/21_cargo_leptos.html
