# Testing

## Automated Tests

```bash
cargo test                                   # all
cargo test home_renders_html                 # one, by name
cargo test -- --nocapture                    # show println! output
```

Request tests use Loco's harness: `request::<App, _, _>(|request, ctx| ...)`
boots the whole app in-process with `config/test.yaml` and sends HTTP
requests to it. No server, port or browser is involved, and there is no
database, so the suite runs in well under a second.

`#[serial]` on every request test is required: each one boots the app, and
two boots at once would fight over the shared context.

**Test files:**
- `tests/config.rs`: the config canary. Loads each `config/<env>.yaml`
  through Loco's own loader and asserts the security baseline every
  environment must keep (empty `ident`, `secure_headers` with the four
  overrides, 15 s `timeout_request`, `remote_ip` on, compression and
  fallback off, a `settings` block that parses, a CSP with
  `'wasm-unsafe-eval'` and no `unsafe-inline`) plus the deliberate
  differences: the three `cache_control` values and that they are valid
  headers (Loco would silently fall back to a year), `CfConnectingIp` and
  burst 120 for staging and production, `ConnectInfo` and the live-reload
  socket locally, `X-Robots-Tag` on staging only, burst 3 in test. Edit a
  config file, run this first.
- `tests/mod.rs`: module root, wires the folders below.
- `tests/requests/home.rs`: the home page. Asserts a 200 with an HTML content
  type, a real document (doctype, `lang="en"`), the app name (`APP_NAME`),
  the islands loader script, the wasm file name it loads (`app.wasm`,
  guarding the compile-time `LEPTOS_OUTPUT_NAME` gotcha), and the footer
  year computed at render time.
- `tests/requests/robots.rs`: `/robots.txt` over HTTP in the test environment
  (200, `text/plain`, `Disallow: /`). The production branch (`Allow: /`) and
  the staging one are unit-tested inside `src/controllers/robots.rs`, since
  the harness only boots the `test` environment.
- `tests/requests/rate_limit.rs`: with `burst: 3` from `config/test.yaml`, three
  requests pass with `x-ratelimit-remaining` counting down, the fourth is a
  429 HTML page with `retry-after` and `cache-control: no-store`, and an
  unmatched path is still a 404 (the limiter only covers routes). The key
  extractor, the 429 page and the config-to-key-source mapping have unit tests
  in `src/middleware/rate_limit.rs`; the `settings:` parsing in `src/settings.rs`.
- `tests/requests/security_headers.rs`: the home page carries a CSP whose
  nonce matches the one on every inline script, with `'wasm-unsafe-eval'`,
  `frame-ancestors 'none'` and no `unsafe-inline`, plus the preset headers and
  overrides (`x-frame-options: DENY`, referrer, permissions, COOP, CORP,
  COEP, nosniff, HSTS) and no `x-powered-by`; it also fails if any `style=` attribute
  appears, since `style-src` is `'self'`. `/robots.txt` gets the preset CSP,
  proving the fallback. The CSP template validation is unit-tested in
  `src/settings.rs`, the nonce substitution in `src/render.rs`.
- `src/assets.rs` (unit tests): parsing the `css:` line of cargo-leptos's
  hash file. The boot-time consistency checks (stale hash file, hashed site
  without one) are exercised by hand, see "Hashed asset names" below, and
  are skipped in the test environment so `cargo test` passes whatever the
  last build left in `target/site`. The request tests run without a hash
  file, so they assert the plain `/pkg/app.css` name.
- `tests/tasks/`, `tests/workers/`: empty Loco starter modules.

Available but unused so far: `rstest` for table-driven tests (one body, many
`#[case]` inputs) and `insta` for snapshot tests.

### CI

`.github/workflows/ci.yaml` runs on every push to `main` and on pull
requests: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test`, the wasm32 build below, a full `cargo leptos build`, and
`cargo audit`. The same list as the pre-deploy checks in `README.DEPLOY.md`.
`cargo audit` fails only on vulnerabilities; "unmaintained" advisories (one
today, `proc-macro-error2`, pulled in by a dependency) are warnings.

### Browser half compile check

The normal build and test only compile the server. This compiles the library
for wasm32 with only the `hydrate` feature and fails if any server-only
dependency leaked out of the `ssr` gate:

```bash
cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

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

Expected: 200 for everything, with `/nope` returning the static not-found
page from `public/404.html`. Known gap: that page is served with status 200,
because Loco's static fallback is tower-http's `ServeFile`; a real 404 status
needs a handler of our own. Response headers must not contain `x-powered-by`.

### Islands in the browser

Open http://localhost:5150 with the developer console open. Expected: no
errors, no warning about a missing island function, and the wasm and JS
requests visible in the network tab. Once an island exists (a login form,
say), interact with it: a working island proves the whole chain, server
markers, bundle load, `hydrate()`, hydration.

### Rate limiting

Development allows a burst of 300 route requests per IP, refilling one per
second (`settings.rate_limit` in `config/development.yaml`). Against
`cargo loco start`:

```bash
seq 1 305 | xargs -I{} curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5150/robots.txt | sort | uniq -c
```

Expected: `300 200` and `5 429`, run in one go (the bucket refills at one
request per second). Then:

```bash
curl -i http://localhost:5150/robots.txt          # the 429 page: retry-after, x-ratelimit-*, cache-control: no-store
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5150/pkg/app.css   # 200: assets are never limited
cargo loco middleware -c                           # shows rate_limit with its numbers and key source
```

Locally the key is the TCP peer, so a `CF-Connecting-IP` header is ignored.
On staging and production the key is that header (the CDN sets it, Caddy
passes it through), so on the server a burst with `-H 'CF-Connecting-IP:
203.0.113.9'` against `http://127.0.0.1:<port>/` exhausts only that fake
visitor's bucket. Direct hits without the header all share the proxy's
address and therefore one bucket.

### Hashed asset names

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

Then `curl -s http://localhost:5150/ | grep -o 'href="/pkg/[^"]*"'` shows
`app.<hash>.css` and `app.<hash>.js`, both present in `target/site/pkg`, and
`cat target/release/hash.txt` shows the same hashes. Two refusals to check
(checked 2026-09-11): edit a hash in `target/release/hash.txt` and start the
release binary again, it must refuse with "the hash file is stale"; then run
`cargo loco start`, the debug binary has no hash file next to it while
`target/site` is hashed, it must refuse with "a hashed build must be deployed
together with its hash file". Finish with `cargo leptos build` to restore
plain names for development.

### Security headers

```bash
curl -sI http://localhost:5150/ | grep -iE '^(content-security|x-frame|referrer|permissions|cross-origin|strict-transport|x-content-type)'
```

Expected: all seven present; the CSP contains `'nonce-…'` and a second
request shows a different nonce. `/robots.txt`, `/pkg/app.css` and `/nope`
show the preset CSP (`default-src 'self' https: …`) instead. On the server
the same against `http://127.0.0.1:<port>/`. Under `cargo leptos watch -- start`
open the page with the console visible: no CSP violation, and live reload
still works (its websocket is allowed by the development `connect-src`).
After a staging deploy, re-scan on https://securityheaders.com and
https://observatory.mozilla.org: CSP and X-Frame-Options must not fail.
A CDN such as Cloudflare injects its own HSTS and `nosniff` in front of the
app, so those two appear on the live site even when the origin is down.

### Request timeout

Nothing in the app hangs, so there is no request to time out and no
automated test. To verify the 15 s `timeout_request` (checked 2026-09-11),
temporarily add a handler that sleeps longer than that to
`src/controllers/home.rs`, run the server and time a request:

```rust
async fn slow() -> Result<Response> {
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    format::text("too late")
}
// in routes(): .add("/slow", get(slow))
```

```bash
curl -s -o /dev/null -w 'status=%{http_code} seconds=%{time_total}\n' http://localhost:5150/slow
```

Expected: `status=408 seconds=15.0…`, the server log shows the request
finishing with `latency=15002 ms status=408`, and `/` still answers in
milliseconds. Remove the handler afterwards.

## Tests to Add With the Features They Cover

- One request test per new page: status, `lang`, one distinctive string.
- Not-found: status 404 and the rendered not-found page, once a real handler
  replaces the static fallback.
- The first island (a login or registration form): validation rejects bad
  input, a valid submission reaches the JSON API, a stricter per-route rate
  limit blocks a burst. The natural home for `rstest`.
