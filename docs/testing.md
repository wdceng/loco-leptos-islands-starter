# Testing

What the tests cover, and how to check things by hand. The why is in
`architecture.md`.

## Automated Tests

```bash
cargo test                                   # all
cargo test home_renders_html                 # one, by name
cargo test -- --nocapture                    # show println! output
cargo nextest run                            # the same suite under nextest
```

Request tests use Loco's harness, `request::<App, _, _>(|request, ctx| ...)`.
It boots the app in-process with `config/test.yaml`. No server, port or
browser. The database is `app_test.sqlite`, rebuilt on every boot
(`dangerously_truncate` and `dangerously_recreate` in `config/test.yaml`),
so the suite runs in about a second.

- **Every request test needs `#[serial]`.** Two app boots at once fight over
  the shared context.
- **`#[serial]` does nothing under nextest.** Each test is its own process,
  so `.config/nextest.toml` runs the binary built from `tests/mod.rs` on one
  thread. Unit tests and the config canary stay parallel. Without it you get
  `UNIQUE constraint failed: seaql_migrations.version` or `no such table`.

**Test files:**

- `tests/mod.rs`: module root, wires the folders below.
- `tests/config.rs`: the config canary. Loads each `config/<env>.yaml` with
  Loco's own loader. Edit a config file, run this first. Every environment
  must have:
  - an empty `ident`
  - `secure_headers` with the seven overrides, plus the strict CSP for
    anything that isn't a page
  - a CSP with `'wasm-unsafe-eval'` and no `unsafe-inline`
  - a 15 s `timeout_request` and a 64 KB `limit_payload`
  - `remote_ip` on, compression and fallback off
  - a `settings` block that parses
  - a mail sender (production's from `MAILER_FROM`)

  Differences on purpose:
  - three `cache_control` values, all valid headers (else Loco silently
    falls back to a year)
  - staging and production: `CfConnectingIp`, burst 120
  - locally: `ConnectInfo`, the live-reload socket
  - staging only: `X-Robots-Tag`
  - test: burst 20
  - test and deployed: the auth API's own bucket, ten at once, then one per
    30 s
  - nightly restart: off locally, on (hour 3) when deployed
  - static-file limit: off locally, on when deployed (burst 300, one token
    per 100 ms)
- `tests/models/users.rs`: the users model against the test database.
  Create with password, find by e-mail and pid, validation, duplicate
  e-mail, and the verification, reset and magic-link token flows. `insta`
  snapshots in `tests/models/snapshots/`.
- `tests/models/sql_pool.rs`: SQLx reads, through `sql::pool`, a user
  Sea-ORM just wrote. It uses a real `query_scalar!`, so it also proves the
  `.sqlx/` cache works.
- `tests/requests/auth.rs`: `/api/auth` end to end. Register, verify, login
  (valid and invalid password, unverified user), current user, forgot and
  reset, magic link, resend verification. Login variants are `rstest`
  cases. `insta` snapshots are in `tests/requests/snapshots/`, shared setup
  in `tests/requests/prepare_data.rs`. Also:
  - Register, forgot and magic link read their mail back. `From:` is
    `settings.mail.from`, and the link starts with `server.host` and carries
    the user's token.
  - The reset link points at `/reset`, which doesn't exist yet (Known Gaps
    in `architecture.md`).
  - Markup in the name arrives escaped in the HTML part.
  - A name over 100 characters registers nobody and sends nothing.
  - A body over 64 KB is a 413.
- `tests/requests/error_page.rs`: a browser's `POST /` is a 405 with the
  Leptos error page, `Allow` kept, the page CSP with its nonce and
  `no-store`. Without an HTML `Accept` it stays Loco's empty 405, and the
  JSON API answers a browser in JSON. A handler that fails gives a browser
  the page with status 500 and anything else Loco's JSON; no route of the
  app fails on purpose, so that one mounts the layer on a small router.
- `tests/requests/links.rs`: every address written as a string leads
  somewhere, since the compiler checks none of them.
  - Every `href` and `src` on the home page, the 404 page, the error page
    and the 429 page. An asset path must be a file in `public/`, checked on
    disk, because the test environment serves no files. Anything else must
    be a route that answers 200, and a `#id` must exist on the page.
    `/pkg/…` is cargo-leptos's output, which `home.rs` checks.
  - The manifest's icons and the stylesheet's font `url()`s exist in
    `public/`.
  - The three auth mails are sent and each link is requested. The verify
    and magic links answer 200. The reset link is pinned as the known gap:
    it's a 404 until a `/reset` page exists, and then the test fails and
    says to remove the gap.
- `tests/requests/home.rs`: a 200 with an HTML content type, a real document
  (doctype, `lang="en"`), the app name (`APP_NAME`), exactly one `<h1>`, the
  skip link and its target, the islands loader script, the wasm file it
  loads (`app.wasm`, guarding the compile-time `LEPTOS_OUTPUT_NAME` gotcha),
  and the footer year computed at render time.
- `tests/requests/not_found.rs`: unknown paths. As HTML, a 404 with the
  not-found page inside the shell and `cache-control: no-store`. Without an
  HTML `Accept`, a 404 with one line of text and no page. Both carry the
  security headers and a request id. Twenty misses in a row are answered,
  the twenty-first is a 429, and routes still answer: misses have their own
  bucket, with the site-wide numbers.
- `tests/requests/robots.rs`: `/robots.txt` in the test environment (200,
  `text/plain`, `Disallow: /`).
- `tests/requests/llms.rs`: `/llms.txt` is Markdown starting with
  `# APP_NAME`, its links start with `server.host`, and each one answers
  200.
- Robots tags and the manifest: the home page has no
  `<meta name="robots">` and its manifest link asks for credentials
  (`home.rs`); the 404 and error pages say `noindex` (`not_found.rs`,
  `error_page.rs`).
- `tests/requests/rate_limit.rs`:
  - With `burst: 20` from `config/test.yaml`, twenty requests pass with
    `x-ratelimit-remaining` counting down. The next is a 429 HTML page with
    `retry-after: 1` (under a second, rounded up, never 0),
    `cache-control: no-store`, the security headers, the strict CSP for
    non-page responses and a request id, so the limiter sits inside
    `secure_headers` and `request_id`. An unmatched path is still a 404:
    the limiter only covers routes.
  - The auth bucket (`auth.burst: 10`): ten logins are 401s, the eleventh is
    the 429 page with about 30 s in `retry-after`. `/robots.txt` still
    answers, since the site-wide bucket has tokens left.
- `tests/requests/security_headers.rs`:
  - On the home page, the CSP nonce matches every inline script. The CSP
    has `'wasm-unsafe-eval'` and `frame-ancestors 'none'`, no
    `unsafe-inline`.
  - The preset headers and overrides are there (`x-frame-options: DENY`,
    referrer, permissions, COOP, CORP, COEP, nosniff, HSTS). No
    `x-powered-by`.
  - Any `style=` attribute fails it, since `style-src` is `'self'`.
  - `/robots.txt` and a miss get the strict CSP from the config
    (`default-src 'none'`, no scripts).
- `tests/tasks/user_create.rs`: the `user_create` CLI task.
- `tests/workers/`: an empty Loco starter module.

**Unit tests in `src/`:**

- `src/assets.rs`: parsing the `css:` line of cargo-leptos's hash file. The
  boot-time checks (stale hash file, hashed site without one) are manual,
  see "Hashed asset names". They're skipped in the test environment, so
  `cargo test` passes whatever the last build left in `target/site`.
  Request tests run without a hash file, so they expect the plain
  `/pkg/app.css`.
- `src/controllers/llms.rs`: the body starts with the app name and the
  description, and a host with a trailing `/` gives no `//` in the links.
- `src/controllers/robots.rs`: `/robots.txt` on production (`Allow: /`) and
  on staging. The harness only boots the `test` environment.
- `src/views/mail.rs`: a mail is a whole HTML document with its link and no
  `<!>` hydration markers, and markup in a visitor's name arrives escaped.
- `src/deploy_checks.rs`: real secret values pass, and empty, blank and
  `replace-me` values are named. The boot itself (production refusing,
  the SMTP login warning) is a manual check below.
- `src/views/error.rs`: server errors, the 405, the 408 and other client
  errors each have their own wording.
- `src/views/too_many_requests.rs`: the 429 page links the stylesheet,
  keeps the `{wait}` placeholder for each refusal, and carries no script.
- `src/middleware/error_page.rs`: only a browser asking for a page outside
  `/api/` gets one, only error answers without HTML are replaced, and the
  page keeps the status and the original headers.
- `src/maintenance.rs`: the nightly restart's arithmetic. The next restart
  is today while the hour is ahead, tomorrow from the hour on. In
  Europe/Zagreb, 2026-03-29 02:30 (the spring-forward gap) is `None` but
  03:00 exists, and 2026-10-25 02:30 (repeated in autumn) resolves to the
  later, CET one. Development and test never restart, production and
  staging may. The stop itself is Loco's ordinary SIGTERM shutdown, checked
  by hand below.
- `src/middleware/rate_limit.rs`: the key extractor (IPv6 keyed by its /64
  network, IPv4 written as IPv6 keyed as IPv4), the 429 page (the wait
  rounded up, in words: "1 second", "6 seconds"; the stylesheet resolved at
  boot), and the config-to-key-source mapping.
- `src/paths.rs`: every file in `public/` has a 16-hex version from
  `build.rs`. Every font URL in `style/tailwind.css` and every icon in
  `site.webmanifest` is the current versioned URL; if not, the failure
  names the value to write. The preloaded font is one of the stylesheet's
  URLs, `?v=` included. The links test then requests each address.
- `src/render.rs`: the nonce substitution.
- `src/settings.rs`: the `settings:` block parses. Unknown keys, a CSP
  template without `{nonce}` and a zero rate-limit burst are refused. The
  auth bucket is required, and refuses zeros while the limiter is on. The
  static-file limit is off without its block, parses when on, and refuses
  zeros when enabled.
  `mail.from` is parsed the way the mailer parses it: `Name <address>`, a
  bare address and a quoted name pass, a bare comma in the name
  (`Doe, John <…>`) fails like any malformed sender. `nightly_restart`
  parses a zone name, refuses an unknown one and an hour above 23, and is
  off when the block is missing.

`tests/config.rs` and the auth tests use `rstest`: one body, many `#[case]`
inputs.

### CI

`.github/workflows/ci.yaml` runs on every push to `main` and on pull
requests. Same list as the pre-deploy checks in `staging.md` and `production.md`:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- the wasm32 lint below
- a full `cargo leptos build`
- `cargo audit`
- the `.sqlx/` cache check: a fresh test database, migrated, then
  `cargo sqlx prepare --check -- --all-targets`

Every job builds with `SQLX_OFFLINE=true`, from the committed cache, like
`cross`.

`cargo audit` fails on vulnerabilities. Advisories ignored on purpose are
in `.cargo/audit.toml`, each with its reason.

### Browser half lint

The normal build and tests only compile the server. This lints the library
for wasm32 with just the `hydrate` feature:

```bash
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

It fails if a server-only dependency leaks out of the `ssr` gate, and it's
the only pass that sees the islands.

## Manual Testing

### Full build served locally

```bash
cargo leptos build && cargo loco start
```

Then, in another terminal:

```bash
for p in / /pkg/app.css /pkg/app.js /pkg/app.wasm /fonts/Inter-Regular.woff2 /robots.txt /nope; do
  printf '%-28s ' "$p"; curl -s -o /dev/null -w '%{http_code} %{size_download}B\n' "http://localhost:5150$p"
done
```

Expected: 200 for all but `/nope`, a 404 with one line of text, since curl
doesn't ask for HTML. With `-H 'Accept: text/html'` it's the not-found
page, still a 404, with `cache-control: no-store`. No `x-powered-by`
anywhere.

### Islands in the browser

Open http://localhost:5150 with the developer console open. Expected: no
errors, no warning about a missing island function, and the wasm and JS
requests in the network tab.

Once you have an island (a login form, say), use it. If it works, the whole
chain works: server markers, bundle load, `hydrate()`, hydration.

**Missing island function?** The island is in `views/`. Move it to
`src/islands.rs`.

### Rate limiting

Development allows a burst of 300 route requests per IP, refilling one per
second (`settings.rate_limit` in `config/development.yaml`). Against
`cargo loco start`, in one go:

```bash
seq 1 305 | xargs -I{} curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5150/robots.txt | sort | uniq -c
```

Expected: `300 200` and `5 429`. Then:

```bash
curl -i http://localhost:5150/robots.txt          # the 429 page: retry-after: 1, x-ratelimit-*, cache-control: no-store
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5150/pkg/app.css   # 200: assets are never limited
cargo loco middleware -c                           # shows rate_limit with its numbers and key source
```

Locally the key is the TCP peer, so a `CF-Connecting-IP` header is ignored.
On staging and production the key is that header, set by the CDN and passed
on by the proxy. On a deployed copy, a burst with
`-H 'CF-Connecting-IP: 203.0.113.9'` against `http://127.0.0.1:<port>/`
empties only that fake visitor's bucket. Hits without the header all share
the proxy's bucket.

An IPv6 visitor is keyed by its /64 network, not its address. On a deployed
copy, a burst that changes the address inside one /64 still empties one
bucket:

```bash
for i in $(seq 1 125); do curl -s -o /dev/null -w '%{http_code}\n' -H "CF-Connecting-IP: 2001:db8:1:2::$i" http://127.0.0.1:<port>/robots.txt; done | sort | uniq -c
```

Expected: `120 200` and `5 429`. With `2001:db8:1:$i::1` instead, every
request is a different /64, so all 125 pass.

The auth API has its own, smaller bucket: `settings.rate_limit.auth`, burst
30 in development. `cargo loco middleware -c` shows it under `auth` in the
`rate_limit` entry, not as a middleware of its own. To see it:

```bash
seq 1 35 | xargs -I{} curl -s -o /dev/null -w '%{http_code}\n' -X POST -H 'content-type: application/json' -d '{"email":"nobody@example.com","password":"x"}' http://localhost:5150/api/auth/login | sort | uniq -c
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5150/robots.txt
```

Expected: `30 401` and `5 429`, then a `200`: the site-wide bucket of 300
still has tokens.

### Outgoing mail

With a catcher on port 1025 (`prerequisites.md`) and the dev server
running:

```bash
curl -s -X POST -H 'content-type: application/json' -d '{"name":"Ana","email":"ana@example.com","password":"12341234"}' http://localhost:5150/api/auth/register
```

Expected in the catcher's inbox (http://localhost:8025): one mail from
`SaaS Starter <noreply@example.com>` (`settings.mail.from` in
`config/development.yaml`, never Loco's `System <system@example.com>`),
with a verification link starting `http://localhost:5150/api/auth/verify/`
(`server.host` as written, not `host:port`).

### Secrets and the SMTP login at boot

Production refuses a secret that is set but not filled in. Run it with a
throwaway database so nothing lands in the repo:

```bash
LOCO_ENV=production JWT_SECRET=x MAILER_HOST=replace-me MAILER_USER=u MAILER_PASSWORD= MAILER_FROM="App <a@example.com>" DATABASE_URL="sqlite:///tmp/prod-check.sqlite?mode=rwc" ./target/debug/app start
```

Expected: exit status 1 and
`production won't start: MAILER_HOST, MAILER_PASSWORD empty or still replace-me`.

The SMTP login only runs when deployed, so check it on staging after a
deploy: `journalctl -u app-stg -b | grep 'smtp login check'`. A wrong host
or password is a `WARN` with the reason, and the site still answers.

### Hashed asset names

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

Then:

```bash
curl -s http://localhost:5150/ | grep -o 'href="/pkg/[^"]*"'   # app.<hash>.css and app.<hash>.js
cat target/release/hash.txt                                    # the same hashes
```

Both files must be in `target/site/pkg`. Then check the two refusals:

1. Edit a hash in `target/release/hash.txt` and start the release binary
   again. It must refuse with "the hash file is stale".
2. Run `cargo loco start`. The debug binary has no hash file next to it but
   `target/site` is hashed, so it must refuse with "a hashed build must be
   deployed together with its hash file".

Finish with `cargo leptos build` to get plain names back for development.

### Security headers

```bash
curl -sI http://localhost:5150/ | grep -iE '^(content-security|x-frame|referrer|permissions|cross-origin|strict-transport|x-content-type)'
```

Expected: all seven. The CSP has `'nonce-…'`, and a second request shows a
different one. `/robots.txt`, `/pkg/app.css` and `/nope` get the strict CSP
from the config instead (`default-src 'none'; style-src 'self'; …`). On a
deployed copy, run it against `http://127.0.0.1:<port>/`.

Under `cargo leptos watch -- start`, with the console open: no CSP
violation, and live reload still works (the development `connect-src`
allows its websocket).

After a deploy, scan the public host on https://securityheaders.com and
https://observatory.mozilla.org. CSP and X-Frame-Options must not fail.

**HSTS and `nosniff` may come from the CDN.** A CDN such as Cloudflare adds
its own in front of the app, so they show up even when the origin is down.

### Nightly restart

The timing is unit-tested. At the hour, the task sends SIGTERM to its own
process. You can try that any time:

```bash
cargo build
./target/debug/app start & sleep 5; kill -TERM $!; wait $!; echo "exit status $?"
```

Expected: `shutting down...` in the log and exit status 0, which the
unit's `Restart=always` turns into a restart. On a deployed copy,
`journalctl -u <unit>` shows `nightly restart scheduled for 03:00 UTC` at
boot, then next morning `nightly restart: stopping`, `shutting down...` and
a fresh boot.

### Request timeout

Nothing in the app hangs, so there's no automated test. To check the 15 s
`timeout_request`, add this to `src/controllers/home.rs` for a moment:

```rust
async fn slow() -> Result<Response> {
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    format::text("too late")
}
// in routes(): .add("/slow", get(slow))
```

Run the server and time a request:

```bash
curl -s -o /dev/null -w 'status=%{http_code} seconds=%{time_total}\n' http://localhost:5150/slow
```

Expected: `status=408 seconds=15.0…`, `latency=15002 ms status=408` in the
server log, and `/` still answering in milliseconds. Remove the handler
afterwards.

## Tests to Add With the Features They Cover

- **Each new page:** a request test for status, `lang` and one distinctive
  string.
- **The first island** (a login or registration form): validation rejects
  bad input, a valid submission reaches the JSON API (its rate-limit bucket
  is already covered in `tests/requests/rate_limit.rs`). A natural home for
  `rstest`. Its request test checks the server render:
  `<leptos-island data-component="Name_` (prefix only, the name carries a
  hash) and `<leptos-children>`, which proves the content went in as
  children and stayed out of the wasm.
