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
  - dev-server, staging and production: `CfConnectingIp`, burst 120
  - locally: `ConnectInfo`, the live-reload socket
  - dev-server and staging only: `X-Robots-Tag`
  - dev-server: the same config as staging, apart from the defaults the
    unit replaces (host, database, secrets)
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
  - Every refused login (no account, wrong password, unverified) gives the
    same 401; passwords of 7 and 129 characters are refused, on register
    and reset; a taken email registers like a new one.
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
    disk, because the test environment serves no files, and must carry its
    current `?v=`: a plain or stale string fails with the constant to use.
    Anything else must be a route that answers 200, and a `#id` must exist
    on the page. `/pkg/…` is cargo-leptos's output, which `home.rs` checks.
  - The web manifest (a route) is JSON with `APP_NAME`, `SURFACE_HEX`,
    `standalone`, and icons that are current versioned files in `public/`.
  - The three auth mails are sent and each link is requested: all three
    answer 200.
- `tests/requests/reset.rs`: the reset page as a person uses it. The mail's
  link shows the form (`no-store`, `no-referrer`, `noindex`); a short
  password shows it again with the reason; a good one changes the password,
  the old one stops working, and the link is spent.
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
- `src/paths.rs`: every generated address is a file in `public/` with a
  16-hex version from `build.rs`. Any `public/` file `style/tailwind.css`
  names (none today) is its current versioned URL; if not, the failure
  names the value to write.
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

**This repo runs its checks locally; GitHub Actions is off.**
`.github/workflows/ci.yaml` is kept for a project made from the template
that uses Actions: with it on, it runs on every push to `main` and on pull
requests. Same list as the pre-deploy checks in `staging.md` and `production.md`:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- the wasm32 lint below
- a full `cargo leptos build --split`, with the patched cargo-leptos
  (`tools/cargo-leptos/`)
- two hashed `--split` builds that must give the same file names, the main
  script naming a split loader that exists (`development.md`, "Lazy
  islands")
- `cargo audit`
- the `.sqlx/` cache check: a fresh test database, migrated, then
  `cargo sqlx prepare --check -- --all-targets`
- the browser tests in Chrome: the island collector's, then `e2e/run.sh`
  (below). A failed run keeps `target/e2e/run/` as an artifact

Every job builds with `SQLX_OFFLINE=true`, from the committed cache, like
the server build.

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

### The island collector

`src/collect.rs` frees islands in the browser half only, so its tests run
there, in headless Chrome. Both programs are already on your machine:
cargo-leptos downloaded the test runner with wasm-bindgen, and chromedriver
must match your Chrome.

```bash
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=~/Library/Caches/cargo-leptos/wasm-bindgen-0.2.129/wasm-bindgen-0.2.129/wasm-bindgen-0.2.129-aarch64-apple-darwin/wasm-bindgen-test-runner CHROMEDRIVER=~/Library/Caches/chromedriver/chromedriver-mac-arm64/chromedriver cargo test --lib --target wasm32-unknown-unknown --no-default-features --features hydrate   # the collector's four tests
```

Freed after two sweeps off the page, kept while on it, one strike then
back is kept, and through a debug build's type erasure.

- **Lazy islands are plain in a test build.** The runner loads a plain
  wasm, not cargo-leptos's split one, and a lazy island's import of
  `__wasm_split_placeholder__` would stop the page from loading
  (`IslandsCheck`, `cfg_attr(test, island)`).
- **The server's test crates are native only** (`Cargo.toml`): built for
  wasm32 they don't compile.
- **`every_island_runs_through_collected`** (`src/islands.rs`, native)
  refuses an island whose body skips `collected`.

On a real page: swap `IslandsCheck` away (any link from the home page),
wait two sweeps (2 s each in a debug build), and `data-islands-live` on
`<html>` drops by one, the console saying `islands: 1 live, 1 freed`.

### Browser tests

`e2e/` drives headless Chrome over WebDriver (thirtyfour) against a real
build: the moves in place and the memory tests. A crate of its own, so
`cargo test` in the root runs none of it.

```bash
e2e/run.sh                      # all of them
e2e/run.sh memory               # only the memory tests (cargo test's filter)
E2E_HEADED=1 e2e/run.sh moves   # with Chrome's window
```

It uses the chromedriver in `~/Library/Caches/chromedriver` (the
collector's, above), or `CHROMEDRIVER`, or the one on the `PATH`.

The script builds the site into `target/e2e` with `--split`, starts it on
port 5160 (`LOCO_ENV=e2e`, `config/e2e.yaml`, a fresh SQLite file) and
chromedriver on 9515, runs the tests one at a time, and stops both. It
never touches the watch loop's `target/site`, port 5150 or database, so it
runs beside `cargo leptos watch`.

- **Moves** (`e2e/tests/moves.rs`): the lazy island wakes; a link moves in
  place with its title, the focus on `<main>` and the title read out; no
  field takes the focus on the way (htmx's refocus by `id`, blurred
  first); Back restores the scroll; a 404 shows in the region; a refused
  form swaps in, keeps the address and keeps the focus in its field; a
  second press sends no second post; a sent form looks busy until its
  answer (a slow network); a newer build and a gateway's 502 load plainly.
  DevTools changes those two answers on their way in.
- **Memory** (`e2e/tests/memory.rs`): an island that leaves is freed; twenty
  moves and twenty Back and Forward steps keep the documents, detached
  nodes, listeners, JS heap and WASM memory flat, measured three times.
- **Health** (`e2e/tests/health.rs`), on one page of each kind (`PAGES`;
  a new page goes in): no sideways scroll at 360 to 1440 px, every control
  44px tall at a phone's width, nothing the accessibility audit (axe-core,
  `e2e/vendor/`) finds, the Reduce Motion rule as the browser read it and
  a transition stopped by it, twenty loads in one tab each with its
  islands awake.
- **Every test fails on a page error**: an exception, a console error or
  warning, a CSP refusal. A test that causes a failed load on purpose
  names its status and page (`page.allow_failed_load(404, "/no-such-page")`);
  any other line still fails it.

What a failed test's page showed (screenshot, HTML, the server log's tail,
and every line the page logged) is in `target/e2e/run/failures/`.

- **The memory tests turn DevTools' log off.** It keeps every element a
  log entry names, and Chrome notes the reset form on each visit, which
  looked like a leak of one `<main>` per Back.
- **A changed answer keeps DevTools pausing answers for the session.**
  Turned off mid-test, Chrome 114 left a paused wasm fetch hanging, and
  the next page's islands never started.
- **A press is a mouse's, sent through DevTools** (`Page::press`): held
  300 ms so it preloads, as a person's does. Never the driver's element:
  chromedriver keeps each one it hands a test, and that holds the old page
  in memory.

## Manual Testing

### Full build served locally

```bash
cargo leptos build --split && cargo loco start
```

Then, in another terminal:

```bash
for p in / /pkg/app.css /pkg/app.js /pkg/app.wasm /favicon/favicon.ico /robots.txt /nope; do
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

The home page's last line is the `IslandsCheck` lazy island: "Islands:
running" once its chunk has loaded and hydrated, "Islands: not running"
before. The network tab shows the split loader (`__wasm_split.*.js`) and
the island's own `split_islands_check_*.wasm`. If it says "running", the
whole chain works: server markers, bundle load, the split chunk,
`hydrate()`, hydration.

**Missing island function?** The island is in `views/`. Move it to
`src/islands.rs`.

### Moves in place

Start on `/reset?token=x`, console and network tab open, then:

1. Click the app's name: the home page swaps in, no reload (the network
   tab shows one document request marked by htmx, not a navigation), the
   tab's title changes, and "Islands: running" appears.
2. Scroll a little, click a link, then Back: the page comes back where it
   was scrolled.
3. Send the reset form with a short password: the form comes back with
   the reason, and the address stays `/reset?token=x`.

Expected: no console errors, no CSP violations. With a screen reader the
new title is read after each move. The request tests check the shell's
markup (`the_shell_moves_between_pages_in_place`); the browser tests
above check the swap, the focus, the scroll, a deploy and a gateway's 502.

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
LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-release cargo leptos build --release --split &&
LEPTOS_SITE_ROOT=target/site-release PORT=5151 ./target/release/app start
```

Then:

```bash
curl -s http://localhost:5151/ | grep -o 'href="/pkg/[^"]*"'   # app.<hash>.css and app.<hash>.js
cat target/release/hash.txt                                    # the same hashes, plus split: and manifest:
```

Both files must be in `target/site-release/pkg`, and `target/site` keeps
the watch loop's plain names. Then check the three refusals:

1. Edit a hash in `target/release/hash.txt` and start the release binary
   again. It must refuse with "the hash file is stale".
2. Run `LEPTOS_SITE_ROOT=target/site-release cargo loco start`. The debug
   binary has no hash file next to it but `target/site-release` is hashed,
   so it must refuse with "a hashed build must be deployed together with
   its hash file".
3. Run `cargo leptos build` (no `--split`), then `cargo loco start`. It must
   refuse with "built without --split while a lazy island exists". Rebuild
   with `cargo leptos build --split`.

### Security headers

```bash
curl -sI http://localhost:5150/ | grep -iE '^(content-security|x-frame|referrer|permissions|cross-origin|strict-transport|x-content-type)'
```

Expected: all seven. The CSP has `'nonce-…'`, and a second request shows a
different one. `/robots.txt`, `/pkg/app.css` and `/nope` get the strict CSP
from the config instead (`default-src 'none'; style-src 'self'; …`). On a
deployed copy, run it against `http://127.0.0.1:<port>/`.

Under `cargo leptos watch --split -- start`, with the console open: no CSP
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

## The Low Bar

The oldest browsers the template supports, about 30 months back. Chrome
114 has a lane here; the rest are findings from an islands app on this
stack, kept for when an app adds them.

```bash
e2e/run.sh low          # the same tests in Chrome for Testing 114
e2e/run.sh low moves    # cargo test's filter, as on the top lane
```

It uses Chrome for Testing 114 and chromedriver 114 from
`~/Library/Caches/e2e-floor/` (below), or `E2E_BINARY` and `CHROMEDRIVER`.
Local only, never in `ci.yaml`. The two leak tests skip there,
saying why (no `DOM.getDetachedDomNodes`); the rest run.

- **htmx's `fetch()` is resource type `XHR` in Chrome 114,** `Fetch` in
  155: the harness pauses both, or a changed answer never reaches the page.

| Browser | Low bar | Why that version |
|---|---|---|
| desktop Chrome | 114 (May 2023) | first with popovers |
| Firefox | 128 (Jul 2024) | Tailwind v4's floor |
| iOS | 17.5 (May 2024) | last iOS 17 |
| Android | 15, Chrome 124 (Apr 2024) | Android 14's Chrome is 113, before popovers |

Getting the browsers (macOS):

```bash
# Chrome for Testing 114 and its driver (CfT's own drivers start at 115)
curl -O https://storage.googleapis.com/chrome-for-testing-public/114.0.5735.90/mac-arm64/chrome-mac-arm64.zip
curl -O https://chromedriver.storage.googleapis.com/114.0.5735.90/chromedriver_mac_arm64.zip
ditto -x -k chrome-mac-arm64.zip ~/Library/Caches/e2e-floor/chrome-114             # not unzip: the app holds links
ditto -x -k chromedriver_mac_arm64.zip ~/Library/Caches/e2e-floor/chromedriver-114  # where run.sh looks
```

- Firefox 128.0.3 from `ftp.mozilla.org`, geckodriver 0.34 or newer.
- iOS 17.5: the `.dmg` from developer.apple.com/download/all, then
  `xcrun simctl runtime add <dmg>`. A disk image, not a cryptex, or
  safaridriver opens no session.
- Android 15: `system-images;android-35;google_apis_playstore;arm64-v8a`,
  chromedriver 124.

Traps that looked like site bugs and weren't:

- **macOS freezes each new Chrome for Testing version** on a Bluetooth or
  local-network prompt until someone clicks Allow on that Mac. Every test
  just times out.
- **Chrome 117 to 121 don't start on macOS 27.** 114 to 116 and 122 and up do.
- **Boot emulators cold (`-no-snapshot`).** A snapshot a broken run saved
  lost Chrome's connection and every `adb` forward.
- **Not all low lanes at once,** even on 64 GB: run them in two batches.
- **iOS 17.5 opens new tabs empty:** no cookies, no `localStorage`. A
  second-tab test checks first and skips with the reason.
- **`checkVisibility` before Chrome 121** knows only the old option names:
  pass `{ visibilityProperty: true, checkVisibilityCSS: true,
  opacityProperty: true, checkOpacity: true }`.
- **`scrollIntoView({ behavior: 'instant' })` still glides in Chrome 114**
  under `scroll-behavior: smooth`: set `scrollBehavior = 'auto'` on
  `<html>` around it.
- **Chrome 114's DevTools has no `DOM.getDetachedDomNodes`:** the memory
  tests skip there, asked of the browser, not by version.
- **A retried test reuses names** unless each try makes its own.

Site bugs it found, for when an app uses these features:

- **Firefox 128's click has no `pointerType`.** A hover-opened panel took
  it for not-a-mouse and closed: keep a hovered flag from `pointerenter`
  and `pointerleave` instead.
- **No popovers (below the bar):** a notice shown by CSS alone,
  `hidden [@supports_not_selector(:popover-open)]:block`, and every
  `[popover]` and `[popovertarget]` hidden, or the buttons do nothing.
- **No anchor positioning (Chrome before 125, Firefox 128, Safari before
  26):** a fallback placing dropdowns under the header.
- **Ask for the property you use: `@supports not (position-area: bottom)`.**
  Chrome 125 to 128 have `anchor-name` but only the old `inset-area`.
- **No subgrid in Chrome 114 to 116:** `grid-cols-subgrid` lists stack
  every value on its own line.
- **Chrome 114 to 123 stop starting islands** after 9 to 14 whole loads
  in one tab (idle time starved). Light pages never get there. The
  template's Leptos fork gives the loader a 2 s timeout.

Probe before fixing: a small test that prints what the page saw, on the
failing lane and on its current twin. Rerun a failure alone before
believing it.

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
