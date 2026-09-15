# Loco + Leptos SSR Starter

A GitHub template for full-stack Rust web apps. One crate, two builds: Loco on the server, Leptos in islands mode for the pages. Users, JWT authentication and transactional mail come from Loco's SaaS starter; the database is SQLite, one file per environment, next to the binary. Security headers, a per-page CSP with nonces, per-visitor rate limiting, hashed asset names and a config canary test are wired in and documented.

Use it with GitHub's **Use this template** button, then follow "After creating a project" below.

## Tech Stack

Built with a minimal-dependency philosophy: compile-time safety over runtime errors, one language end to end, and no framework JavaScript shipped for static content. No JavaScript is written by hand.

**Rust + Loco** - Rails-style framework on top of Axum and tower, generated from the `SaaS` starter with a REST API. Loco supplies the app skeleton: per-environment YAML config, the middleware stack, Sea-ORM with migrations, JWT auth, the mailer, background workers, the CLI (`start`, `routes`, `middleware`, `doctor`, `db`, `task`) and tracing setup. Loco controllers own every URL: page controllers render one Leptos page each, the auth controller answers JSON under `/api/auth`.

**Leptos (islands mode)** - Views are type-checked Rust components rendered to HTML on the server. Pages are plain static HTML by default. Only components marked `#[island]` are hydrated in the browser, so the WebAssembly bundle contains just those components, not the whole site. There is no client-side router: every navigation is a normal full-page request handled by Loco. No island exists yet; the first candidates are the login and registration forms, which would call the JSON API from the browser.

Wiring: `leptos` is built with the `islands` feature; the client entry calls `leptos::mount::hydrate_islands()`; the page shell renders `<HydrationScripts options islands=true/>`; the `target/site` output of `cargo leptos` is served by Loco's `static` middleware.

**Tailwind CSS v4** - Utility classes written directly in `view!` macros. cargo-leptos runs the Tailwind standalone binary as part of the build, so there is still no Node or npm; only classes actually used end up in the stylesheet, which is emitted next to the islands bundle. Release builds hash the bundle and stylesheet names, so production caches them for a year and every deploy is a fresh URL. Inter font family, self-hosted.

**SQLite** - Sea-ORM over SQLx, SQLite driver only (`sqlx-postgres` was dropped on purpose). Each environment names its own file (`app_<env>.sqlite`), created on first boot by `auto_migrate`. Loco sets the SQLite PRAGMAs at boot (WAL, `synchronous = NORMAL`, foreign keys on, 5 s busy timeout). The `-wal` and `-shm` files next to the database are part of it: back up with `sqlite3 <file> ".backup <copy>"`, never by copying the file alone.

## After Creating a Project

The crate, binary and asset bundle are all named `app`. To rename them, change the name in `Cargo.toml` (`[package]`, `[[bin]]` and `output-name`), `LEPTOS_OUTPUT_NAME` in `.cargo/config.toml`, the `use app::` imports in `src/bin/main.rs`, `examples/playground.rs` and `tests/`, the `/pkg/app.css` links in `public/404.html` and `src/middleware/rate_limit.html`, the asserted names in `tests/requests/home.rs`, and the `app_<env>.sqlite` defaults in `config/*.yaml`. Then delete any `app_*.sqlite*` files, since the next boot creates the new ones.

Also yours to set: `APP_NAME` in `src/views/layout.rs` (page title, header, footer), `name` and `description` in `public/favicon/site.webmanifest`, the `HOST` defaults in `config/staging.yaml` and `config/production.yaml`, and the icons in `public/favicon/` (the template ships a placeholder set; `theme_color` in the manifest matches it). On the server, the secrets in the systemd unit (`README.DEPLOY.md`). `Cross.toml` pins the Linux build image; change the target if the server is not x86_64.

## What Is in the App

| Area | Where | Notes |
|------|-------|-------|
| Pages | `src/controllers/home.rs`, `src/views/` | `/` (home) and `/robots.txt`. `views/layout.rs` is the document shell; `render.rs` renders a page with the per-request CSP nonce |
| Users and auth | `src/models/users.rs`, `src/controllers/auth.rs` | JSON API under `/api/auth`: `register`, `verify/{token}`, `login`, `forgot`, `reset`, `current`, `magic-link`, `magic-link/{token}`, `resend-verification-mail`. JWT bearer tokens, 7-day expiry |
| Mail | `src/mailers/auth/` | Welcome, forgot-password and magic-link templates (text and HTML). Sent through SMTP from `config/<env>.yaml` |
| Background | `src/workers/`, `src/tasks/` | Starter examples: a download worker and a `user_create` CLI task |
| Migrations | `migration/` | Sea-ORM migrations, applied at boot (`auto_migrate: true`) |
| Config | `config/<env>.yaml` | development, test, staging, production. Typed app settings (`rate_limit`, `security`) in `src/settings.rs` |

## Security

`cargo loco middleware` shows which Loco middlewares are on for the current `LOCO_ENV`.

| Layer | Implementation | Provided by | Status |
|-------|----------------|-------------|--------|
| Security headers | `secure_headers` middleware, `github` preset plus overrides, and a per-page nonce CSP (see below) | Loco + this project | on |
| Request body limit | `limit_payload` middleware | Loco | on (Loco default) |
| Request timeout | `timeout_request` middleware, 15 s in every environment: a hung handler is cancelled with 408 instead of holding a connection; well under a CDN's origin limit (Cloudflare: 100 s) | Loco | on |
| Panic isolation | `catch_panic` middleware | Loco | on (Loco default) |
| Static files, ETag | `static` and `etag` middlewares. `compression` stays off on purpose: the reverse proxy compresses in front of the app | Loco | on |
| Authentication | JWT (`auth.jwt` in config), passwords hashed by Loco, e-mail verification, magic links, reset tokens | Loco | on |
| Secrets | `JWT_SECRET` and `MAILER_*` are read from the environment. Production has no defaults and refuses to boot without them; staging has placeholder defaults so it boots with `LOCO_ENV` alone | this project | on |
| Outbound TLS (mailer) | Loco's mailer is `lettre` with RusTLS | Loco | on |
| Rate limiting | `rate_limit` middleware (`src/middleware/rate_limit.rs`): a `tower_governor` token bucket per visitor IP, keyed by the same source as Loco's `remote_ip` (`CF-Connecting-IP` behind Cloudflare, the TCP peer locally), tuned per environment under `settings.rate_limit` in `config/*.yaml`; only routes count, static assets are exempt; 429 is an HTML page with `Retry-After`. Loco has no built-in limiter. | this project | on |

Not used: `cors` (no cross-origin callers), `fallback` (Loco's welcome page; the static `404.html` serves instead), `powered_by` (`server.ident: ""` drops the header).

### HTTP Security Headers

Two sources, both in `config/*.yaml`:

**Static headers** come from Loco's `secure_headers` middleware under `server.middlewares`. The `github` preset sets Content-Security-Policy, Strict-Transport-Security, X-Content-Type-Options, X-Frame-Options, X-Download-Options, X-Permitted-Cross-Domain-Policies and X-Xss-Protection. Five headers are added and two replaced through `overrides`:

| Header | Why it is an override |
|--------|----------------------|
| X-Frame-Options | Preset says `sameorigin`; `DENY` matches the CSP's `frame-ancestors 'none'` |
| Strict-Transport-Security | Preset sends a bare `max-age`; replaced with the preload form, `max-age=31536000; includeSubDomains; preload`. Behind a CDN the zone's HSTS setting overwrites it at the edge and must say the same. The preload list itself is joined once, for the apex domain after go-live, at https://hstspreload.org; from then on every subdomain must be https. |
| Referrer-Policy | Not in the preset: `strict-origin-when-cross-origin` |
| Permissions-Policy | Not in the preset: camera, microphone, geolocation off |
| Cross-Origin-Opener-Policy | Not in the preset: `same-origin` |
| Cross-Origin-Resource-Policy | Not in the preset: `same-origin`, our files load only on our own pages |
| Cross-Origin-Embedder-Policy | Not in the preset: `require-corp`, our pages load nothing cross-origin. Together with COOP this makes pages cross-origin isolated (the Spectre-class defence). An embed from another domain (map, video, payment widget) would need that resource to opt in via CORP or CORS, or this header dropped. |

**The page CSP** is a template under `settings.security.content_security_policy`, filled per request by `render_page` in `src/render.rs`. Leptos stamps a nonce on every inline script it emits (the islands loader, the dev live-reload hook); the same nonce goes into `script-src 'self' 'nonce-…' 'wasm-unsafe-eval'`, so no `'unsafe-inline'` is needed. The policy starts with `default-src 'none'` and lists every resource kind the page uses (scripts, styles, images, fonts, the manifest, fetches); a new kind of resource is blocked until its directive is added on purpose. Loco's middleware only adds a header the response does not already carry, which is why the page CSP wins on pages while the preset CSP remains the fallback for the JSON API, assets, `robots.txt` and the static 404 and 429 pages. Development and test add the `cargo leptos watch` websocket to `connect-src`; staging and production are identical.

A CDN in front of the app may inject its own HSTS, so a header scan of a proxied host shows the zone setting, not the origin's value.

`tests/config.rs` is the canary: it loads all four config files and pins the headers, timeout, rate limit, cache policy and CSP shape per environment, so a config change that alters policy must update that test too.

## Environments

| `LOCO_ENV` | Database | Secrets | Static files | Detail |
|------------|----------|---------|--------------|--------|
| `development` | `app_development.sqlite` in the repo | defaults in the file | `target/site`, `no-cache` | `README.DEV.md` |
| `test` | `app_test.sqlite`, recreated per run | defaults in the file | none | `README.TESTING.md` |
| `staging` | `app_staging.sqlite` next to the binary | placeholder defaults; the unit may override | `site/`, 60 s | `README.DEPLOY.md` |
| `production` | `app_production.sqlite` next to the binary | required from the environment | `site/`, one year, hashed names | `README.DEPLOY.md` |

The environment is picked by `LOCO_ENV`. Secrets are read from environment variables through the `get_env` helper inside the YAML; Loco does not load a `.env` file. On the server the variables live in the systemd unit.

## Development

```sh
cargo install --locked loco cargo-leptos     # once; see PREREQUISITES.md
brew services start mailpit                  # local SMTP on 1025, inbox on http://localhost:8025
cargo leptos watch -- start                  # both halves, Tailwind, live reload, server on http://localhost:5150
```

`cargo loco start` runs the server alone against whatever `target/site` holds. The first boot creates `app_development.sqlite` and applies the migrations. `cargo clippy --all-targets` and `cargo test` before handing over; `cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate` proves nothing leaked out of the `ssr` gate.

Tailwind is enabled by `tailwind-input-file = "style/tailwind.css"` in the `[package.metadata.leptos]` section of `Cargo.toml`. The Tailwind version is pinned with the `LEPTOS_TAILWIND_VERSION` environment variable when needed.

## Docs

| File | Covers |
|---|---|
| `PREREQUISITES.md` | One-time tool setup |
| `README.DEV.md` | Day-to-day commands, environments, release builds, gotchas |
| `README.TESTING.md` | What each test covers, manual checks |
| `README.DEPLOY.md` | Bare-binary deploy with placeholders: systemd unit, Caddy, Cloudflare notes |
| `README.DPY-falkenstein-1.md`, `README.DPY.STG.md` | The same deploy on the falkenstein-1 server: one-time setup, the staging pipeline |
| `README.Docker.md` | Container path: image, compose with Caddy (`Caddyfile`) |
| `CLAUDE.md` | Architecture and conventions for AI-assisted work; `AGENTS.md` is Loco's generic guide |

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
