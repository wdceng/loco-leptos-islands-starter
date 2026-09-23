# Deployment

The app deploys as a bare Linux binary behind Caddy, run by systemd. The
first deploy is this file in order: checks, build, the one-time server
setup, the smoke test, go-live. Every later deploy starts at "Routine
Deploy". The guide uses placeholders; substitute them once per environment:

| Placeholder | Meaning | Example |
|---|---|---|
| `<user>@<host>` | SSH login to the server | `deploy@203.0.113.10` |
| `<app-dir>` | Deploy directory on the server | `/srv/app/stg` |
| `<unit>` | systemd unit name | `app-staging` |
| `<port>` | Loopback port Loco listens on | `3000` |
| `<domain>` | Public host name | `staging.example.com` |
| `<env>` | `staging` or `production` | `staging` |
| `<profile>` | Cargo profile of the server binary: `staging` or `release` | `staging` |

The reference setup is Cloudflare in front of Caddy in front of Loco. Caddy
alone works too; see "Without Cloudflare" at the end. If `<user>` is not
root, prefix the `systemctl` calls below with `sudo` and allow them without
a password in sudoers.

## What Gets Deployed

Four things, side by side in `<app-dir>`, plus the secrets file the unit
loads:

```text
<app-dir>/
├── app          <- Linux x86_64 build of the server binary
├── hash.txt     <- hashes of the bundle and stylesheet names; the app reads it next to the binary
├── config/      <- <env>.yaml only; Loco reads the one file named by LOCO_ENV
├── site/        <- the cargo-leptos output (target/site locally): hashed bundle and stylesheet, fonts, 404 page
└── secrets.env  <- JWT_SECRET and MAILER_*, readable by <user> only, loaded by the unit
```

`hash.txt` exists because release builds run with `LEPTOS_HASH_FILES=true`:
cargo-leptos names the outputs `app.<hash>.css/.js/.wasm` and records the
hashes in that file, which the app reads at boot (`src/assets.rs`) to link
the right names. Every deploy is therefore a new URL for each asset, which is
what lets production cache them for a year. The binary refuses to boot when
`site/` holds hashed names and the file is missing next to it, or when the
two come from different builds.

No `.env` file: Loco does not read one. The non-secret variables live in the
systemd unit and the secrets in `secrets.env`, which the unit puts into the
environment; the YAML's `get_env` picks them up from there.

## Environments

Each environment is named after its file in `config/`:

| `LOCO_ENV` | Where | Runs as | Bind | URL |
|---|---|---|---|---|
| `development` | this repo | `cargo leptos watch -- start` | `localhost:5150` | `http://localhost:5150` |
| `test` | this repo | `cargo test`, in-process | none | none |
| `staging` | `<app-dir>` | `<unit>.service` | `127.0.0.1:<port>` | `https://<domain>` |
| `production` | `<app-dir>` | `<unit>.service` | `127.0.0.1:<port>` | `https://<domain>` |

Staging and production follow the same steps with their own values. Where
they differ:

| | staging | production |
|---|---|---|
| Binary profile | `staging`: thin LTO, incremental, line tables kept | `release`: fat LTO, stripped |
| Secrets | placeholder defaults in the file; `secrets.env` optional | no defaults; `secrets.env` required, the boot refuses without |
| Static cache | 60 s | one year, `immutable` |
| Search engines | `robots.txt` says `Disallow: /`, `X-Robots-Tag: noindex, nofollow` | `robots.txt` says `Allow: /`, no `X-Robots-Tag` |
| Logs | compact, debug, backtraces on | JSON, info |
| Extra gates before deploy | none | `cargo audit` and the leftovers check below |

## Checks

```bash
cargo clippy --all-targets && cargo test &&
cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

Before a **production** deploy, two more gates. `cargo audit`, because
production is what an advisory actually threatens. And the template's
placeholder values (the `example.com` host, the `SaaS Starter` name; see
"Make It Yours" in `README.md`) must be gone: the `!` makes "nothing found"
the success, so this must print nothing and succeed:

```bash
cargo audit && ! grep -n "example\.com\|SaaS Starter" config/production.yaml src/views/layout.rs public/favicon/site.webmanifest
```

To try the exact release build locally, served by the release binary itself
(`cargo loco start` would run the debug binary instead):

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

Check the page, the browser console and the smoke-test loop below against
`localhost:5150`. Afterwards `target/site` holds hashed files that the debug
binary cannot name, so run `cargo leptos build` (or the watch loop) before
going back to development.

## Build the Artifacts

The `site/` folder is platform independent and built natively by
cargo-leptos. The binary is built for Linux x86_64 by `cross` inside a Linux
container (`Cross.toml` pins the image; `PREREQUISITES.md` has the one-time
pull). On Apple Silicon the container is emulated, so allow a few minutes on
a cold build.

Two Cargo profiles build the server binary (`Cargo.toml`): `release` for
production, fully optimised with a slow single-threaded link, and `staging`,
the same optimisation class but with a parallel thin-LTO link, incremental
rebuilds and line tables kept, so a staging deploy is quick to rebuild and
its backtraces show function names. The frontend half always builds with
`--release`, whichever profile the binary uses.

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --profile <profile> --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/<profile>/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site
```

`dist/` then holds `app`, `hash.txt` and `site/`. It is rebuilt from scratch
so nothing from an older build survives. `hash.txt` is written by the
frontend build into `target/release/` whatever the binary's profile
(cargo-leptos puts it where the native release binary would be) and must
travel with the binary. `cross` reads `.cargo/config.toml` inside its
container, so the compile-time `LEPTOS_OUTPUT_NAME` is the same as in a
native build.

## One-Time Server Setup

### Step 0: Check the Port Is Free (on the server)

```bash
ss -tlnp | grep ':<port>\b' || echo "<port> free"
```

If something is listening, pick another port and change it in two places
only: `PORT=` in the unit file, and `reverse_proxy` in the Caddy block.
Nothing else hardcodes it.

### Step 1: Create the Directory and Upload (from the local terminal)

With `dist/` built as above (the unit does not exist yet, so the routine
pipeline cannot run):

```bash
ssh <user>@<host> "mkdir -p <app-dir>/config" &&
scp dist/app <user>@<host>:<app-dir>/app &&
scp dist/hash.txt <user>@<host>:<app-dir>/hash.txt &&
scp -r dist/site <user>@<host>:<app-dir>/ &&
scp config/<env>.yaml <user>@<host>:<app-dir>/config/ &&
ssh <user>@<host> "chmod +x <app-dir>/app"
```

### Step 2: Create the Secrets File (on the server)

`config/production.yaml` takes no defaults for `JWT_SECRET`, `MAILER_HOST`,
`MAILER_USER` and `MAILER_PASSWORD`; a missing one fails the boot at once.
They live in a file only `<user>` can read, not in the unit (unit files are
world-readable) and not in the repo. Run this as `<user>`, once, then
replace the `MAILER_*` values with the real account:

```bash
umask 077 && printf 'JWT_SECRET=%s\nMAILER_HOST=replace-me\nMAILER_USER=replace-me\nMAILER_PASSWORD=replace-me\n' "$(openssl rand -base64 48)" > <app-dir>/secrets.env
```

`JWT_SECRET` is generated on the spot and never leaves the server; give
staging and production different ones. Staging boots without the file on the
placeholder defaults in `config/staging.yaml`; create one there too once the
staging copy should send real mail.

### Step 3: Create the Service (on the server)

`/etc/systemd/system/<unit>.service`:

```ini
[Unit]
Description=<unit>
After=network.target

[Service]
Type=simple
User=<user>
WorkingDirectory=<app-dir>
ExecStart=<app-dir>/app start
Restart=always
RestartSec=5
# Which config/<env>.yaml to load
Environment=LOCO_ENV=<env>
# Loopback only: nothing but Caddy on this box may reach it
Environment=BINDING=127.0.0.1
Environment=PORT=<port>
# Leptos options (no Cargo.toml on the server); site/ is next to the binary
Environment=LEPTOS_OUTPUT_NAME=app
Environment=LEPTOS_SITE_ROOT=site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_ENV=PROD
# JWT_SECRET and MAILER_*: a file only <user> can read, never in the unit
# (unit files are world-readable). The leading dash lets staging boot
# without the file on its placeholder defaults; production refuses to boot
# without the variables anyway.
EnvironmentFile=-<app-dir>/secrets.env

[Install]
WantedBy=multi-user.target
```

`start` after the binary path is Loco's subcommand. Without it the binary
prints its help and exits, and `Restart=always` would loop forever. Keep
`BINDING=127.0.0.1`: `0.0.0.0` would bind the port publicly and bypass
Caddy.

The app also stops itself once a day, at `nightly_restart.hour` in
`config/<env>.yaml` (03:00 UTC as shipped; `src/maintenance.rs`): it sends
itself the same SIGTERM as `systemctl stop`, shuts down cleanly with status
0, and `Restart=always` is what starts it again: keep that line.
`RestartSec=5` is the nightly downtime. The journal shows `nightly restart
scheduled for 03:00 UTC` at boot and, at the hour, `nightly restart:
stopping` followed by Loco's `shutting down...` and a fresh boot.

### Step 4: Enable and Start (on the server)

```bash
systemctl daemon-reload && systemctl enable --now <unit> && systemctl status <unit>
```

Boot refusals and what they mean:

| Message | Cause |
|---|---|
| `one of the static path are not found` | `site/` upload missing or incomplete (`must_exist: true` in staging and production) |
| `a hashed build must be deployed together with its hash file` | `hash.txt` not uploaded |
| `the hash file is stale` | `hash.txt` and `site/` come from different builds |
| `invalid settings: block: failed to parse timezone` | `nightly_restart.zone` is not an IANA zone name (`UTC`, `Europe/Zagreb`) |
| `settings.nightly_restart.hour must be 0 to 23` | the restart hour is out of range |
| a `get_env` error naming `JWT_SECRET` or `MAILER_*` | production only: `secrets.env` missing, incomplete, or not readable by `<user>` |

### Step 5: Add the Caddy Block (on the server)

Add a block to `/etc/caddy/Caddyfile`; never replace a file other sites
share. Edit it there with `nano /etc/caddy/Caddyfile`, or from the local
terminal with `ssh <user>@<host> -t "nano /etc/caddy/Caddyfile"`.

```text
<domain>:80, <domain>:443 {
        reverse_proxy 127.0.0.1:<port>
        encode gzip zstd
}
```

Production usually adds a `www` redirect to the bare domain:

```text
www.<domain> {
        redir https://<domain>{uri} permanent
}
```

Caddy obtains the certificate itself, so it needs ports 80 and 443 and a DNS
record for `<domain>`. Compression happens here, which is why Loco's
`compression` middleware stays off in the config. Validate first, then a
graceful zero-downtime reload; if validation fails, the reload does not run
and Caddy stays up on the old config:

```bash
ssh <user>@<host> "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy"
```

Use `restart` instead of `reload` only when updating the Caddy binary itself
or recovering from a broken process.

## Smoke Test After a Deploy

On the server, straight at the Loco port (a CDN challenge page would
otherwise get in the way):

```bash
ssh <user>@<host> 'cd <app-dir> && for p in / /robots.txt $(ls site/pkg | grep -v "\.ts$" | sed "s|^|/pkg/|"); do printf "%-40s " "$p"; curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:<port>$p"; done && curl -s http://127.0.0.1:<port>/ | grep -o "href=\"/pkg/[^\"]*\"" && curl -s http://127.0.0.1:<port>/robots.txt && echo "X-Robots-Tag:" && curl -sI http://127.0.0.1:<port>/ | grep -i x-robots-tag; true'
```

All 200, and the `href` values must be among the hashed files listed above
them (the page links what the deploy shipped). Then the two environments
part ways: staging prints `Disallow: /` and `X-Robots-Tag: noindex,
nofollow`; production prints `Allow: /` and nothing after the `X-Robots-Tag`
line. Finally open `https://<domain>` with the browser console open: the
page renders, no errors, and the wasm and JS requests show in the network
tab; on production, `https://www.<domain>` lands on `https://<domain>`.

## Go-Live (once)

After the first production deploy passes the smoke test: DNS for `<domain>`
and `www.<domain>` to the server, proxied through Cloudflare if that is the
setup; the Caddy blocks from Step 5, validated and reloaded. Then, once the
site answers over https and `www` redirects, the HSTS steps, deliberately
last because they are hard to undo: (1) if the domain is proxied through
Cloudflare, set the zone's HSTS (SSL/TLS, Edge Certificates) to max-age 12
months with "Apply to subdomains" and "Preload" on, matching the origin
header in `config/production.yaml`; (2) submit `<domain>` at
https://hstspreload.org, which checks the live headers and queues the domain
for the browsers' built-in list; (3) from then on every subdomain must be
https, and removal from the list takes months.

## Routine Deploy (from the local terminal)

One chain from checks to a running server; any failing step aborts it, so a
failed build never touches the server. The previous binary, hash file and
site are kept on the server as `*.prev` for the rollback below. Before a
production deploy, put the two production gates from "Checks" in front of
the first line, and allow the full emulated build time: `release` is the
slow fat-LTO profile.

```bash
cargo clippy --all-targets && cargo test &&
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --profile <profile> --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/<profile>/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site &&
ssh <user>@<host> "systemctl stop <unit>" &&
ssh <user>@<host> "cd <app-dir> && { [ -f app ] && cp app app.prev; [ -f hash.txt ] && cp hash.txt hash.txt.prev; [ -d site ] && rm -rf site.prev && mv site site.prev; true; }" &&
scp dist/app <user>@<host>:<app-dir>/app &&
scp dist/hash.txt <user>@<host>:<app-dir>/hash.txt &&
scp -r dist/site <user>@<host>:<app-dir>/ &&
ssh <user>@<host> "find <app-dir>/site -name '.DS_Store' -delete" &&
scp config/<env>.yaml <user>@<host>:<app-dir>/config/ &&
ssh <user>@<host> "systemctl start <unit>" &&
ssh <user>@<host> "journalctl -u <unit> -f"
```

`site/` is moved aside before the upload so files deleted locally do not
linger on the server, and macOS Finder files that cargo-leptos copied from
`public/` are deleted after it (`.gitignore` keeps them out of git, not out
of the copy). Only the one config file is uploaded: Loco reads the file named
by `LOCO_ENV` and nothing else, so the other environments' files stay off the
server. Run the smoke test above afterwards.

### Rollback

The previous deploy is on the server as `app.prev`, `hash.txt.prev` and
`site.prev`. Putting it back is a swap and a restart, no build. The three
must move together: the app refuses to boot when `hash.txt` and `site/` come
from different builds.

```bash
ssh <user>@<host> "systemctl stop <unit> && cd <app-dir> && mv app app.bad && mv app.prev app && mv hash.txt hash.txt.bad && mv hash.txt.prev hash.txt && rm -rf site.bad && mv site site.bad && mv site.prev site && systemctl start <unit> && systemctl status <unit> --no-pager"
```

### Config-Only Change

Loco reads `config/<env>.yaml` at boot, so a restart is required:

```bash
scp config/<env>.yaml <user>@<host>:<app-dir>/config/ && ssh <user>@<host> "systemctl restart <unit>"
```

### Changing a Unit Variable or a Secret

A change to the unit needs a reload of systemd and a restart; a change to
`secrets.env` needs only the restart:

```bash
ssh -t <user>@<host> "nano /etc/systemd/system/<unit>.service" &&
ssh <user>@<host> "systemctl daemon-reload && systemctl restart <unit>"
```

## Operating

```bash
ssh <user>@<host> "systemctl status <unit>"
ssh <user>@<host> "systemctl restart <unit>"
ssh <user>@<host> "journalctl -u <unit> -f"
ssh <user>@<host> "ss -tln | grep -E ':(80|443|<port>)\b'"
```

### Manual Run (without systemd)

Stop the unit first, otherwise `Restart=always` keeps a second copy fighting
for the port. The secrets have to be exported by hand, since only the unit
loads the file. Without systemd the nightly stop is just a stop: at the
restart hour the process exits and nothing starts it again, which is fine
for a short session.

```bash
systemctl stop <unit>
cd <app-dir>
pkill -f <app-dir>/app
set -a && . ./secrets.env && set +a
LOCO_ENV=<env> BINDING=127.0.0.1 PORT=<port> LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD nohup ./app start > app.log 2>&1 &
tail -f app.log
```

Always `pkill -f` with the full path. A bare `pkill app` would also kill any
other process with that name, staging and production on the same box for a
start.

## Cloudflare

`config/staging.yaml` and `config/production.yaml` set
`remote_ip.source: CfConnectingIp`, and the rate limiter keys on the same
header, because the reference deployment proxies the domain through
Cloudflare. Cloudflare overwrites `CF-Connecting-IP` at the edge; Caddy
passes it through unchanged. Consequences:

- `curl` from outside may hit a challenge page if one is enabled for the
  zone; smoke-test on the server instead.
- Caddy still obtains its own certificate for the origin; Cloudflare
  terminates TLS at the edge and connects to it over https.
- Cloudflare injects `Strict-Transport-Security` and
  `X-Content-Type-Options` on every response, so a header scan of the live
  site shows those even when the origin is down. Check the origin on the
  server (`curl -sI http://127.0.0.1:<port>/`) to see what the app itself
  sends.
- A request that reaches the origin IP directly, bypassing Cloudflare, could
  forge `CF-Connecting-IP`. The fix is Caddy's global `trusted_proxies` with
  Cloudflare's CIDRs plus `client_ip_headers CF-Connecting-IP`, or a firewall
  that only admits Cloudflare's ranges. Neither is configured by this
  template.

### Without Cloudflare

With Caddy alone in front of the app, the peer address Loco sees is Caddy's,
so `CfConnectingIp` would key every visitor on a header nobody sets and
`ConnectInfo` would put every visitor into one rate-limit bucket. The rate
limiter refuses to boot a deployed environment keyed on the peer. Supporting
`X-Forwarded-For` (Loco's `XForwardedFor` source) means extending `KeySource`
in `src/middleware/rate_limit.rs` and its config-to-key mapping; the request
tests for the limiter and `tests/config.rs` pin the current behaviour and
must change with it.
