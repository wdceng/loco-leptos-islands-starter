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
   also pass a rate limit; static files pass a generous one of their own
   when deployed.
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
use reach the stylesheet, which sits next to the islands bundle. The look
is Tailwind's own default: its font stack, its palette, its reset, so the
template carries no one's brand. Four role tokens in `style/tailwind.css`
(`primary`, `surface`, `ink`, `ink-muted`) point at that palette. Views use
only the tokens, never a scale step like `gray-600`, so a new look changes
those four lines, not the views.

**Hashed asset names** - `LEPTOS_HASH_FILES=true cargo leptos build --release`
names the bundle and stylesheet by content hash and writes `hash.txt`, which
`src/assets.rs` reads at boot to link them. Production's one-year cache
depends on it, so every release is built this way. `Cargo.toml` leaves
hashing off on purpose: the watch loop only hashes its first build, then
goes stale.

cargo-leptos doesn't hash anything in `public/`, so `build.rs` versions
every file there itself, at compile time: a hash of each file's bytes
(FNV-1a, no new crate), written as one Rust constant per file in modules
that follow the folders (`paths::assets::favicon::APPLE_TOUCH_ICON_PNG =
"/favicon/apple-touch-icon.png?v=<hash>"`). Rust code links files only
through those constants, so a renamed or removed file fails the compile,
and the links test fails on a page that links a `public/` file as a plain
string. If the stylesheet names a `public/` file (a self-hosted font, say),
it carries the `?v=` written out; a test in `src/paths.rs` fails with the
value to paste when that file changes.
The web manifest is a route (`src/controllers/manifest.rs`), built from
`APP_NAME`, the home page's description, `SURFACE_HEX` and `BRAND_HEX`, and
the icon constants. Cargo reruns the script when anything in `public/`
changes, is added or is removed, new folders included, and
`watch-additional-files` in `Cargo.toml` makes the watch loop rebuild the
server too. The binary and `site/` of a deploy come from the same checkout,
so the hashes always match the files served; no hash file is needed. The
one address without a version is `/favicon.ico` at the root: browsers ask
for it by that name, and only when a page links no icon, which ours do.

**SQLite** - Sea-ORM over SQLx, SQLite driver only (`sqlx-postgres` was
dropped on purpose). Each environment has its own `app_<env>.sqlite`:
`?mode=rwc` in the URL creates it, `auto_migrate` applies the schema at
boot. Loco sets the PRAGMAs: WAL, `synchronous = NORMAL`, foreign keys on,
5 s busy timeout. The `-wal` and `-shm` files are part of the database, so
back up with `sqlite3 <file> ".backup <copy>"`, never by copying the file
alone.

## Sea-ORM and SQLx

Both, each for its own job, on one connection pool.

| | Sea-ORM | SQLx |
|---|---|---|
| For | Loco's plumbing | the app's own data |
| Connection | opens it (`ctx.db`) | borrows its pool: `sql::pool(&ctx)` (`src/sql.rs`) |
| Schema | the migrations (`migration/`) | none, it reads the migrated schema |
| Queries | Loco's own: the users/auth model, the test harness's reset | everything the app adds: `query!`, `query_as!`, `query_scalar!` |
| Checked | Rust types, but the SQL is built at runtime | the SQL itself, at compile time, against the real schema |
| Tools | `cargo loco db migrate`, `auto_migrate` at boot, `cargo loco generate` | `cargo sqlx prepare -- --all-targets`, the `.sqlx/` cache |

Why both:

- Sea-ORM is how Loco works: `ctx.db`, migrations, generators, auth and the
  test harness all go through it. Dropping it means fighting the framework,
  and the users model is starter code that stays as generated.
- `query!` turns a wrong column, table or type into a `cargo check` error,
  before any test runs. Raw SQL through Sea-ORM isn't checked at all, and
  real queries (search, counts per filter, full-text search) are awkward in
  its query builder.
- One pool, so both get Loco's PRAGMAs and SQLite's single writer isn't
  fought over by two pools.

Rules:

- **`sqlx` stays at the version Sea-ORM uses** (0.9.0 with Sea-ORM 2.0.4).
  A second `sqlx` would be a different `SqlitePool` type, so `src/sql.rs`
  stops compiling. After a Sea-ORM upgrade, `cargo tree -i sqlx` shows one
  version.
- **Only the macros:** `query!`, `query_as!`, `query_scalar!`. Never
  runtime `sqlx::query("…")` or SQL built with `format!`. Values are bound,
  never interpolated.
- **One transaction, one library.** A Sea-ORM transaction and a SQLx
  transaction are never the same transaction.
- **A new table or column:** a migration, `cargo loco db migrate`, then
  `cargo sqlx prepare -- --all-targets` (`development.md`). Only then does
  `query!` see it. `.sqlx/` is committed, so builds without a database
  (CI, `cross`) check the queries offline. Always `-- --all-targets`: the
  plain form drops the tests' queries from the cache.
- **SQLite reports few types for expressions.** `COUNT(*)`, `MAX(...)`,
  `CASE` and `json_each` columns come back nullable or loosely typed, so
  name the type: `AS "total!: i64"` (not null), `AS "name?"` (nullable).
- **Never export `DATABASE_URL`.** Every config reads it, `test.yaml` too,
  so an exported one sends `cargo test` to that database, and the test
  harness wipes it. Set it on the `prepare` command only.
- `tests/models/sql_pool.rs` pins that SQLx reads what Sea-ORM wrote, with a
  real `query_scalar!`, so the cache is in use from the start.
- **Once the app has dozens of `query!` calls, move them into a crate of
  their own** (a `db` crate next to this one). Every recompile re-expands
  every `query!` in the crate, so with the queries next to the pages, each
  page edit in `cargo leptos watch` pays for all of them. In their own crate
  they only recompile when the SQL changes.

## What Is in the App

"From" says where an area comes from: Loco's Rest API starter as generated,
or this template. Nothing the starter generated was removed except the
nine Tera mail templates (`src/mailers/auth/*/*.t`), replaced by Leptos and
`format!` with the same content. Where the
template changed a starter file, the column names the change.

| Area | From | Where | Notes |
|------|------|-------|-------|
| Pages | This template | `src/controllers/home.rs`, `src/controllers/robots.rs`, `src/controllers/llms.rs`, `src/controllers/manifest.rs`, `src/views/` | `/` renders `views/home.rs` in the shell (`views/layout.rs`) through `render.rs`, which adds the CSP nonce. `PageMeta.robots` adds `<meta name="robots">` per page: `noindex` on the 404 and error pages, none on the home page. `/robots.txt` is plain text: `Allow: /` in production, `Disallow: /` elsewhere. `/llms.txt` describes the site for AI assistants in Markdown (llmstxt.org): `APP_NAME`, the home page's description and links built from `server.host`. Add a line there for every public page. `/manifest.webmanifest` is the web manifest, a route built from `APP_NAME`, the colours and the versioned icon constants, sent `no-cache` |
| Health | Loco | Loco, via `AppRoutes::with_default_routes()` in `src/app.rs` | `/_ping`, `/_health` and `/_readiness`, the last two checking the database and queue. Rate limited like any route |
| Users and auth | Loco starter; the `/api/auth` rate limit is this template's | `src/models/users.rs`, `src/controllers/auth.rs` | JSON API under `/api/auth`: `register`, `verify/{token}`, `login`, `forgot`, `reset`, `current`, `magic-link`, `magic-link/{token}`, `resend-verification-mail`. JWT bearer tokens, 7-day expiry |
| Not found | This template | `src/controllers/not_found.rs`, `src/views/not_found.rs` | The router's fallback. Serves the `static` block's folder with its cache header, and anything else as a 404 with `no-store`: the not-found page for an HTML request, one line of text otherwise |
| Error pages | This template | `src/middleware/error_page.rs`, `src/views/error.rs` | A layer right after `timeout_request`. When a request accepts HTML and isn't under `/api/`, an error answer without an HTML body (Loco's JSON 500, the empty 405 and 408, the JSON 400, 413 and 415) becomes the Leptos error page: same status, original headers kept (`Allow`), page CSP, `no-store`. Everything else keeps Loco's answer |
| Mail | Loco starter's mailer and wording; the rendering, sender setting and link origin are this template's | `src/mailers/auth.rs`, `src/views/mail.rs` | Welcome, forgot-password and magic-link mails, sent over the SMTP in `config/<env>.yaml` with Loco's `Mailer::mail`. The HTML part is a Leptos component: checked at compile time, every value escaped. Subject and text are `format!`. The sender, `settings.mail.from`, is parsed at boot the way the mailer parses it. Links start with `server.host` |
| Background | Loco starter | `src/workers/`, `src/tasks/` | Starter examples: a download worker and a `user_create` CLI task |
| Deploy checks | This template | `src/deploy_checks.rs` | At boot. Production refuses a secret that is set but empty or `replace-me`. Staging and production log in to SMTP once in the background and log the result, sending nothing |
| Nightly restart | This template | `src/maintenance.rs` | Staging and production stop once a day (`settings.nightly_restart`), systemd starts them again. Never in development or test |
| Migrations | Loco starter | `migration/` | Sea-ORM migrations, applied at boot (`auto_migrate: true`) |
| Own queries | This template | `src/sql.rs`, `.sqlx/` | SQLx on Sea-ORM's pool, checked at compile time ("Sea-ORM and SQLx") |
| Config | Loco starter (development, test, production); `staging.yaml`, `dev-server.yaml` (staging's copy) and the typed settings are this template's | `config/<env>.yaml` | Typed settings (`rate_limit`, `security`, `mail`, `nightly_restart`) in `src/settings.rs` |

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
| Secrets | `JWT_SECRET` and `MAILER_*` (host, user, password, sender) come from the environment. Production has no defaults and won't boot without them, nor with one that is empty or still `replace-me` (`src/deploy_checks.rs`). Staging has public placeholders, so `LOCO_ENV` alone boots it. Deployed, the app logs in to SMTP once at start, in the background: a wrong account is a warning in the journal, never a refused boot. The production deploy chain stops before building when `secrets.production.env` is missing or has `replace-me` left | this project | on |
| Outbound TLS (mailer) | Loco's mailer is `lettre` with RusTLS | Loco | on |
| Rate limiting | `rate_limit` middleware (`src/middleware/rate_limit.rs`), since Loco has none. A `tower_governor` token bucket per visitor IP, or per /64 network for IPv6 so one machine can't rotate addresses. Keyed like Loco's `remote_ip`: `CF-Connecting-IP` behind the reference CDN, the TCP peer locally. Tuned per environment in `settings.rate_limit`. Misses get their own bucket with the same numbers. Static files get a generous bucket of their own on staging and production (`settings.file_rate_limit`, `rate_limit::files_bucket`): 300 at once, then 10 a second, which no person reaches, only a script pulling the same files over and over. Off in development and test. The 429 is an HTML page with `Retry-After`, rounded up so it's never 0, linking the stylesheet resolved at boot. `/api/auth` gets a second bucket (`rate_limit::auth_bucket`, `settings.rate_limit.auth`) because it mails any address (`register`) or takes a password. Deployed, it allows ten calls at once, then one per 30 s, inside the site-wide limit | this project | on |

Not used:

- `cors`: no cross-origin callers.
- `static` and `fallback`: Loco's file serving and welcome page. `src/app.rs`
  takes both out and puts the app's own fallback first.
- `powered_by`: Loco's `X-Powered-By` middleware turns itself off when
  `server.ident` is `""`, as in all five configs.

### HTTP Security Headers

Two sources, both in `config/*.yaml`.

**Static headers** come from Loco's `secure_headers` middleware under
`server.middlewares`. The `github` preset sets Content-Security-Policy,
Strict-Transport-Security, X-Content-Type-Options, X-Frame-Options,
X-Download-Options, X-Permitted-Cross-Domain-Policies and X-Xss-Protection.
Seven overrides apply everywhere, an eighth on staging and the dev server:

| Header | Why it's an override |
|--------|----------------------|
| X-Frame-Options | Preset says `sameorigin`. `DENY` matches the CSP's `frame-ancestors 'none'` |
| Strict-Transport-Security | Preset sends a bare `max-age`. Ours is the preload form, `max-age=31536000; includeSubDomains; preload`. The `preload` token is harmless until the apex domain is submitted at https://hstspreload.org, a one-time step once every subdomain serves https. Behind a CDN the zone's HSTS setting overwrites it at the edge, so keep them equal. A header scan then shows the zone's value |
| Referrer-Policy | Added: `strict-origin-when-cross-origin` |
| Permissions-Policy | Added: camera, microphone, geolocation off |
| Cross-Origin-Opener-Policy | Added: `same-origin` |
| Cross-Origin-Resource-Policy | Added: `same-origin`, so the app's files load only on its own pages |
| Cross-Origin-Embedder-Policy | Added: `require-corp`, as pages load nothing cross-origin. With COOP this gives cross-origin isolation, the Spectre-class defence. An embed from another domain (map, video, payment widget) would need CORP or CORS opt-in, or this header dropped |
| X-Robots-Tag | Staging and dev-server only: `noindex, nofollow`, so a test copy stays out of search results even through an inbound link. `robots.txt` already disallows crawling there |

**The page CSP** is a template in `settings.security.content_security_policy`,
filled per request by `render_page`. Leptos stamps a nonce on every inline
script it emits (the islands loader, the dev live-reload hook), and the same
nonce goes into `script-src 'self' 'nonce-…' 'wasm-unsafe-eval'`, so
`'unsafe-inline'` isn't needed. The policy starts at `default-src 'none'` and lists each
resource kind the page uses: scripts, styles, images, fonts, the manifest,
fetches. Anything new stays blocked until its directive is added on purpose.

Loco's middleware only adds a header the response lacks, so the page CSP
wins on pages, the not-found and error pages included. Everything else
(the JSON API, files, `robots.txt`, `llms.txt`, the not-found line, the
429 page) gets the
`Content-Security-Policy` override under `secure_headers`:
`default-src 'none'`, only the app's own stylesheet, images and fonts, no
scripts, no framing. It replaces the `github` preset's CSP, which allows
scripts from any https origin and inline styles.

Development and test add the `cargo leptos watch` websocket to the page
CSP's `connect-src`. Staging and production are identical.

**The config canary**: `tests/config.rs` loads all five configs and pins,
per environment, the headers, fallback CSP, timeout, body limit, rate limit,
cache policy and page CSP shape.

## Environments

| `LOCO_ENV` | Database | Secrets | Static files | Detail |
|------------|----------|---------|--------------|--------|
| `development` | `app_development.sqlite` in the repo | defaults in the file | `target/site`, `no-cache` | `development.md` |
| `test` | `app_test.sqlite`, recreated per run | defaults in the file | none | `testing.md` |
| `dev-server` | `/var/lib/app-dev/app_dev-server.sqlite` (`StateDirectory`) | as staging | as staging | `dev-server.md` |
| `staging` | `/var/lib/app-stg/app_staging.sqlite` (`StateDirectory`) | placeholder defaults, the environment may override | `site/`, 60 s, adds `X-Robots-Tag: noindex, nofollow` | `staging.md` |
| `production` | `/var/lib/app-prod/app_production.sqlite` (`StateDirectory`) | required from the environment | `site/`, one year, immutable (needs the `LEPTOS_HASH_FILES=true` build) | `production.md` |

Secrets are environment variables, read by the `get_env` helper in the
YAML. Loco loads no `.env` file. Getting them to the process is the
deploy's job: each environment has its own
`secrets.dev-server.env`, `secrets.staging.env` or
`secrets.production.env` in the repo root,
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

Every job builds with `SQLX_OFFLINE=true`, from the committed `.sqlx/`
cache, as `cross` does. A separate job migrates a fresh test database and
runs `cargo sqlx prepare --check -- --all-targets`, so a stale cache fails
in CI, not on the next deploy.

`cargo loco` here is a cargo alias (`loco = "run --"` in
`.cargo/config.toml`) for the app binary, not the `loco` CLI. You only need
the CLI for `loco new`, which is done.

## Conventions

- Database queries the app adds use SQLx's `query!` on `sql::pool(&ctx)`.
  Loco's own models stay on Sea-ORM. The rules are in "Sea-ORM and SQLx".
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
- The surface and primary colours are repeated as hex in `SURFACE_HEX` and
  `BRAND_HEX` (`src/views/layout.rs`), which the `theme-color` tag and the
  web manifest use. Change them with `style/tailwind.css`.
- No `style=` attributes: the CSP is `style-src 'self'`, and the request
  test fails on one. With `default-src 'none'`, a new resource kind (video,
  iframe, worker) needs its own directive in all five configs before it
  loads.
- The document shell is `shell` in `src/views/layout.rs`. Pages fill only
  `<main>`. Per-page values travel in `PageMeta`. `APP_NAME` there is the
  one place the app's display name lives. The head already has favicons, the
  manifest, `theme-color` and the home-screen tags. The viewport has no
  `viewport-fit=cover`: iOS keeps the page clear of the notch itself. With
  `cover`, the page needs `env(safe-area-inset-*)` padding, and WebKit
  sometimes updates those values only on the next scroll after a rotation,
  so the page sits off-centre until then. Open Graph tags wait for a
  1200×630 image.
- Everything in `public/` is copied into `site/` and served, in production
  with a one-year cache. Every file is versioned for you (`?v=<hash>`,
  "Hashed asset names"): link it from Rust through `src/paths.rs`, never as
  a string. No `.DS_Store` or scratch files.
- Every page a browser can get is Leptos: the pages, the 404, the error
  page and the 429. No HTML is written by hand or built in Rust strings.
  The error page and the 429 share `ErrorPage` (`src/views/error.rs`); the
  429 is its own document (`src/views/too_many_requests.rs`), rendered once
  per limiter at boot, without the shell's scripts, because it goes out
  under the strict fallback CSP. It uses the shell's `BODY` and `COLUMN`
  constants, so its classes can't drift.
- No static `404.html`: a miss always gets the rendered not-found page.
  Loco's own `static` check also wants such a file, but this app replaces
  that middleware, and its own boot check only requires the site folder.
- Mails: the HTML part is a Leptos component in `src/views/mail.rs`,
  rendered with `mail::document`, and sent with Loco's `Mailer::mail`.
  Subject and text are `format!`. No Tera mail templates: they fail only at
  send time, and Loco's Tera escapes only files named `.html`, `.htm` or
  `.xml`. Keep one text node per paragraph (`format!` inside `{}`), so no
  `<!>` hydration markers end up in the mail.
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
  shows JSON. `tests/requests/links.rs` pins all three: once a `/reset`
  page exists, it fails and says to remove this gap.
- Addresses are strings: links, asset paths and the mail links compile
  whatever they say. `tests/requests/links.rs` checks every one the pages,
  the manifest, the stylesheet and the mails use, so a renamed file or a
  dead link fails a test instead of reaching a visitor.
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
| `sqlx` | The app's own queries, `query!` checked at compile time, on Sea-ORM's pool (`src/sql.rs`). Pinned to the version Sea-ORM uses (`ssr` only) |
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
