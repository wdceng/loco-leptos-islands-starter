# Deployment

The app runs as a Linux binary under systemd, behind Caddy. For the first
deploy, follow this file top to bottom. After that, start at "Routine
Deploy".

Replace these placeholders once per environment:

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
alone works too, see "Without Cloudflare" at the end. If `<user>` isn't
root, put `sudo` before the `systemctl` calls and allow them without a
password in sudoers.

## What Gets Deployed

Four things side by side in `<app-dir>`, plus the secrets file the unit
loads:

```text
<app-dir>/
├── app          <- Linux x86_64 build of the server binary
├── hash.txt     <- hashes of the bundle and stylesheet names; the app reads it next to the binary
├── config/      <- <env>.yaml only; Loco reads the one file named by LOCO_ENV
├── site/        <- the cargo-leptos output (target/site locally): hashed bundle and stylesheet, fonts, the 404.html the boot checks for
└── secrets.env  <- JWT_SECRET and MAILER_*, readable by <user> only, loaded by the unit
```

Release builds use `LEPTOS_HASH_FILES=true`, which gives the files hashed
names (`app.<hash>.css/.js/.wasm`) and lists them in `hash.txt`. New names
on every deploy are what let production cache files for a year. The app
won't start without a `hash.txt` from the same build as `site/`. The
release build also minifies everything, so there's no minify step. How it
works: `ARCHITECTURE.md`.

There's no `.env` file, Loco doesn't read one. Settings go in the systemd
unit, secrets in `secrets.env`. The unit loads both into the environment,
where `get_env` in the YAML picks them up.

## Environments

Each environment is named after its file in `config/`:

| `LOCO_ENV` | Where | Runs as | Bind | URL |
|---|---|---|---|---|
| `development` | this repo | `cargo leptos watch -- start` | `localhost:5150` | `http://localhost:5150` |
| `test` | this repo | `cargo test`, in-process | none | none |
| `staging` | `<app-dir>` | `<unit>.service` | `127.0.0.1:<port>` | `https://<domain>` |
| `production` | `<app-dir>` | `<unit>.service` | `127.0.0.1:<port>` | `https://<domain>` |

Staging and production take the same steps. Where they differ:

| | staging | production |
|---|---|---|
| Binary profile | `staging`: thin LTO, incremental, line tables kept | `release`: fat LTO, stripped |
| Secrets | placeholder defaults, `secrets.env` optional | no defaults, `secrets.env` required or it won't start |
| Static cache | 60 s | one year, `immutable` |
| Search engines | `robots.txt` says `Disallow: /`, `X-Robots-Tag: noindex, nofollow` | `robots.txt` says `Allow: /`, no `X-Robots-Tag` |
| Logs | compact, debug, backtraces on | JSON, info |
| Extra checks before deploy | none | `cargo audit` and the placeholder check below |

## Checks

```bash
cargo clippy --all-targets && cargo test &&
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

Before a **production** deploy, also run `cargo audit` and check that the
template's placeholders are gone (see "Make it yours" in `README.md`). It
passes when it prints nothing:

```bash
cargo audit && ! grep -n "example\.com\|SaaS Starter" config/production.yaml src/views/layout.rs public/favicon/site.webmanifest
```

To try the exact release build locally, run the release binary itself.
`cargo loco start` would run the debug one.

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

Check the page, the browser console and the smoke test below against
`localhost:5150`. Then run `cargo leptos build` or the watch loop before
you go back to development. The debug binary can't find the hashed files
in `target/site`.

## Build the Artifacts

cargo-leptos builds `site/`, which runs on any platform. `cross` builds the
Linux x86_64 binary in a Linux container. `Cross.toml` pins the image and
`PREREQUISITES.md` has the one-time pull. On Apple Silicon a cold build
takes a few minutes.

`<profile>` is `release` for production, fully optimised with a slow link,
or `staging`, with faster rebuilds and function names in backtraces. See
`Cargo.toml`. The frontend always builds with `--release`.

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --profile <profile> --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/<profile>/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site
```

`dist/` now holds `app`, `hash.txt` and `site/`, built fresh each time.
`hash.txt` always lands in `target/release/`, whatever the profile. It must
travel with the binary.

The server's glibc must be at least as new as the one in the `cross` image.
Right now the `:main` tag in `Cross.toml` needs 2.39: Debian 13 and
Ubuntu 24.04 work, Debian 12 and Ubuntu 22.04 fail with
`version GLIBC_2.39 not found`. `:main` moves, so check both sides:

```bash
strings dist/app | grep -oE 'GLIBC_[0-9.]+' | sort -Vu | tail -1   # what the binary needs
ssh <user>@<host> "ldd --version | head -1"                        # what the server has; must be at least that
```

## One-Time Server Setup

### Step 0: Check the Port and the glibc (on the server)

```bash
ss -tlnp | grep ':<port>\b' || echo "<port> free"
ldd --version | head -1    # at least the GLIBC_ version the binary needs (see "Build the Artifacts")
```

Port taken? Pick another and change it in two places only: `PORT=` in the
unit and `reverse_proxy` in the Caddy block.

### Step 1: Create the Directory and Upload (from the local terminal)

Build `dist/` as above first. The routine deploy can't run yet, there's no
unit.

```bash
ssh <user>@<host> "mkdir -p <app-dir>/config" &&
scp dist/app <user>@<host>:<app-dir>/app &&
scp dist/hash.txt <user>@<host>:<app-dir>/hash.txt &&
scp -r dist/site <user>@<host>:<app-dir>/ &&
scp config/<env>.yaml <user>@<host>:<app-dir>/config/ &&
ssh <user>@<host> "chmod +x <app-dir>/app"
```

### Step 2: Create the Secrets File (on the server)

`config/production.yaml` has no defaults for `JWT_SECRET`, `MAILER_HOST`,
`MAILER_USER`, `MAILER_PASSWORD` and `MAILER_FROM`. Miss one and the app
won't start. They go in a file only `<user>` can read. Not in the unit,
anyone can read those, and not in the repo. `.gitignore` covers
`secrets.env` and `*.secrets.env` if you keep a local copy.

Run this once, as `<user>`, then put your real mail account in the
`MAILER_*` values:

```bash
umask 077 && printf 'JWT_SECRET=%s\nMAILER_HOST=replace-me\nMAILER_USER=replace-me\nMAILER_PASSWORD=replace-me\nMAILER_FROM="App <replace-me>"\n' "$(openssl rand -base64 48)" > <app-dir>/secrets.env
```

`MAILER_FROM` is the sender, `Name <address>`. Keep the double quotes,
systemd and "Manual Run" below both need them. A sender the mailer would
reject, such as a name with a bare comma, stops the boot.

- **Gmail:** `MAILER_USER` is the address and `MAILER_PASSWORD` an app
  password (Google account, Security, 2-Step Verification, App passwords),
  never the account password. `MAILER_FROM` must use the same address, or
  Gmail rewrites it.
- **Postmark, SES, Mailgun** and similar services want a sender they've
  verified.
- **Port:** 587 with STARTTLS is the default. For implicit TLS on 465, add
  `MAILER_PORT=465` to this file and set `tls: implicit` under
  `mailer.smtp` in the config.

`JWT_SECRET` is made on the server and never leaves it. Give staging and
production different ones. Staging runs without the file, on the
placeholders in `config/staging.yaml`, until you want it to send real mail.

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
# Loopback only, so just Caddy on this box can reach it
Environment=BINDING=127.0.0.1
Environment=PORT=<port>
# Public origin, no port: links in mail start with it
Environment=HOST=https://<domain>
# Leptos settings (no Cargo.toml on the server), site/ next to the binary
Environment=LEPTOS_OUTPUT_NAME=app
Environment=LEPTOS_SITE_ROOT=site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_ENV=PROD
# JWT_SECRET and MAILER_*, readable by <user> only. Never in the unit:
# anyone can read unit files. The dash lets staging start without the
# file; production won't start without the variables.
EnvironmentFile=-<app-dir>/secrets.env

[Install]
WantedBy=multi-user.target
```

**Keep `start` after the binary path.** Without it the binary prints its
help and exits, and `Restart=always` loops forever.

**Keep `BINDING=127.0.0.1`.** `0.0.0.0` opens the port to everyone and
bypasses Caddy.

**Keep `Restart=always`.** The app stops itself once a day and needs it to
come back.

That stop comes at `nightly_restart.hour` in `config/<env>.yaml`, 03:00 UTC
as shipped. It's a clean shutdown, the same as `systemctl stop`, and
`RestartSec=5` is the downtime. The journal shows
`nightly restart scheduled for 03:00 UTC` at boot, then at the hour
`nightly restart: stopping`, Loco's `shutting down...` and a fresh boot.
Details in `ARCHITECTURE.md`.

### Step 4: Enable and Start (on the server)

```bash
systemctl daemon-reload && systemctl enable --now <unit> && systemctl status <unit>
```

If it won't start, the message tells you why:

| Message | Cause |
|---|---|
| `one of the static path are not found` | `site/` missing or incomplete (`must_exist: true` in staging and production) |
| `a hashed build must be deployed together with its hash file` | `hash.txt` not uploaded |
| `the hash file is stale` | `hash.txt` and `site/` come from different builds |
| `invalid settings: block: failed to parse timezone` | `nightly_restart.zone` isn't an IANA zone name (`UTC`, `Europe/Zagreb`) |
| `settings.nightly_restart.hour must be 0 to 23` | restart hour out of range |
| `settings.mail.from is not a sender` | `MAILER_FROM` isn't `Name <address>` or an address the mailer accepts, such as a name with a bare comma |
| a `get_env` error naming `JWT_SECRET` or `MAILER_*` | production only: `secrets.env` missing, incomplete, or not readable by `<user>` |

### Step 5: Add the Caddy Block (on the server)

Add a block to `/etc/caddy/Caddyfile`. Never replace the file, other sites
may share it. Edit it on the server with `nano /etc/caddy/Caddyfile`, or
from your machine with `ssh <user>@<host> -t "nano /etc/caddy/Caddyfile"`.

```text
<domain>:80, <domain>:443 {
        reverse_proxy 127.0.0.1:<port>
        encode gzip zstd
}
```

Production usually adds a redirect from `www` to the bare domain:

```text
www.<domain> {
        redir https://<domain>{uri} permanent
}
```

Caddy gets the certificate itself, so it needs ports 80 and 443 and a DNS
record for `<domain>`. It also compresses, so Loco's `compression`
middleware stays off.

Validate, then reload with no downtime. If validation fails, Caddy keeps
running on the old config:

```bash
ssh <user>@<host> "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy"
```

Use `restart` instead of `reload` only to update the Caddy binary or to
recover a broken process.

## Smoke Test After a Deploy

Run it on the server, straight at the Loco port, so a CDN challenge page
can't get in the way:

```bash
ssh <user>@<host> 'cd <app-dir> && for p in / /robots.txt $(ls site/pkg | grep -v "\.ts$" | sed "s|^|/pkg/|"); do printf "%-40s " "$p"; curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:<port>$p"; done && curl -s http://127.0.0.1:<port>/ | grep -o "href=\"/pkg/[^\"]*\"" && curl -s http://127.0.0.1:<port>/robots.txt && echo "X-Robots-Tag:" && curl -sI http://127.0.0.1:<port>/ | grep -i x-robots-tag; true'
```

Every line should say 200, and each `href` must be one of the hashed files
listed above it. Then:

- Staging prints `Disallow: /` and `X-Robots-Tag: noindex, nofollow`.
- Production prints `Allow: /` and nothing after the `X-Robots-Tag` line.

Last, open `https://<domain>` with the browser console open. No errors, and
the wasm and JS requests show in the network tab. On production,
`https://www.<domain>` should land on `https://<domain>`.

## Go-Live (once)

After the first production deploy passes the smoke test:

1. Point DNS for `<domain>` and `www.<domain>` at the server, proxied
   through Cloudflare if you use it.
2. Add the Caddy blocks from Step 5, validate and reload.
3. Check that the site answers over https and `www` redirects.

Then HSTS. It comes last because it's hard to undo.

1. If the domain goes through Cloudflare, set the zone's HSTS (SSL/TLS,
   Edge Certificates) to max-age 12 months with "Apply to subdomains" and
   "Preload" on. That matches the header in `config/production.yaml`.
2. Submit `<domain>` at https://hstspreload.org. It checks the live headers
   and queues the domain for the browsers' built-in list.

**From then on, every subdomain must be https.** Getting off the list takes
months.

## Routine Deploy (from the local terminal)

One chain from checks to a running server. Any failure stops it, so a
failed build never touches the server. The previous `app`, `hash.txt` and
`site/` are kept as `*.prev` for rollback.

For production, put the two extra checks from "Checks" in front of the
first line, and expect a slow build.

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

The old `site/` is moved aside, so files you deleted locally don't linger
on the server, and macOS `.DS_Store` files copied from `public/` are
removed. Only `<env>.yaml` goes up, Loco reads no other. Then run the smoke
test.

### Rollback

The previous deploy is on the server as `app.prev`, `hash.txt.prev` and
`site.prev`. Rolling back is a swap and a restart, no build.

**Move all three together.** The app won't start if `hash.txt` and `site/`
come from different builds.

```bash
ssh <user>@<host> "systemctl stop <unit> && cd <app-dir> && mv app app.bad && mv app.prev app && mv hash.txt hash.txt.bad && mv hash.txt.prev hash.txt && rm -rf site.bad && mv site site.bad && mv site.prev site && systemctl start <unit> && systemctl status <unit> --no-pager"
```

### Config-Only Change

Loco reads `config/<env>.yaml` at boot, so upload it and restart:

```bash
scp config/<env>.yaml <user>@<host>:<app-dir>/config/ && ssh <user>@<host> "systemctl restart <unit>"
```

### Changing a Unit Variable or a Secret

A unit change needs a systemd reload and a restart. A change to
`secrets.env` needs only the restart.

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

Stop the unit first, or `Restart=always` starts a second copy that fights
for the port. Only the unit loads `secrets.env`, so the `set -a` line loads
it by hand. At the nightly restart hour the app exits and stays down.
That's fine for a short session.

```bash
systemctl stop <unit>
cd <app-dir>
pkill -f <app-dir>/app
set -a && . ./secrets.env && set +a
LOCO_ENV=<env> BINDING=127.0.0.1 PORT=<port> HOST=https://<domain> LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD nohup ./app start > app.log 2>&1 &
tail -f app.log
```

**Always `pkill -f` with the full path.** A bare `pkill app` kills every
process with that name, staging and production on the same box included.

## Cloudflare

`config/staging.yaml` and `config/production.yaml` set
`remote_ip.source: CfConnectingIp`, and the rate limiter keys on the same
header. Cloudflare overwrites `CF-Connecting-IP` at the edge and Caddy
passes it on. Loco trusts that one header, with no list of trusted proxies,
so it has to be trustworthy end to end. Why: `ARCHITECTURE.md`. What it
means for you:

- `curl` from outside may hit a challenge page if the zone has one.
  Smoke-test on the server instead.
- Caddy still gets its own certificate. Cloudflare ends TLS at the edge and
  connects to Caddy over https.
- Cloudflare adds `Strict-Transport-Security` and `X-Content-Type-Options`
  to every response, even when the origin is down. To see what the app
  itself sends, run `curl -sI http://127.0.0.1:<port>/` on the server.

**Requests that skip Cloudflare can fake their IP.** One that reaches the
server's IP directly can forge `CF-Connecting-IP`. Fix it with Caddy's
global `trusted_proxies` set to Cloudflare's CIDRs plus
`client_ip_headers CF-Connecting-IP`, or a firewall that only lets in
Cloudflare's ranges. This template sets up neither.

### Without Cloudflare

With only Caddy in front, `CfConnectingIp` breaks the rate limiter. Any
client can send its own `CF-Connecting-IP` and pick its bucket, and every
client that sends none shares Caddy's. `ConnectInfo` is no better behind a
proxy, so the limiter won't start a deployed environment keyed on the peer.

Use `X-Forwarded-For` instead, which Loco calls `RightmostXForwardedFor`.
That means extending `KeySource` and its config-to-key mapping in
`src/middleware/rate_limit.rs`, and updating the limiter's request tests
and `tests/config.rs`, which pin the current behaviour.
