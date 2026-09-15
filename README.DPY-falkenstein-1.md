## Server

- Name: falkenstein-1
- Host: `root@188.34.179.70`
- OS: Debian 13 (trixie), glibc 2.41 (checked 2026-09-10 via `grep PRETTY /etc/os-release; ldd --version`)
- The box is shared with other projects (ws, ECQTA). This app touches only its own directory, unit, port, and Caddy block listed below. Never edit or restart the other projects' units.
- Caddy is installed on the box itself and shared. The compose path (`README.Docker.md`) brings its own Caddy and is for a box the app has to itself, not this one.
- The `ecqta.com` zone, including `app.ecqta.com`, is proxied through Cloudflare with a JavaScript challenge enabled zone-wide. Consequences: curl from outside gets a 403 challenge page (smoke-test on the server instead), Caddy still obtains its own certificate for the origin, and the visitor's real IP reaches the app only via Cloudflare's `CF-Connecting-IP` header, which Caddy passes through unchanged. `config/staging.yaml` and `production.yaml` therefore set `remote_ip.source: CfConnectingIp` (Loco 1.1 trusts exactly one source), and the `rate_limit` middleware keys on the same header. A request that reaches the origin IP directly, bypassing Cloudflare, could forge that header; the fix is Caddy's global `trusted_proxies` with Cloudflare's CIDRs plus `client_ip_headers CF-Connecting-IP`, a shared-Caddyfile change not done yet. Cloudflare also injects `Strict-Transport-Security` and `X-Content-Type-Options` on every response, so a header scan shows those two even when the app sends nothing; check the origin on the server (`curl -sI http://127.0.0.1:3201/`) to see what the app itself sends.

Each environment is named after its file in `config/`. The names below are the template's: replace `app` with the project's name (README.md, "After Creating a Project") and pick the project's own subdomain, domain and ports.

| `LOCO_ENV`    | Where               | Runs as                       | Bind             | URL                     |
| ------------- | ------------------- | ----------------------------- | ---------------- | ----------------------- |
| `development` | this repo           | `cargo leptos watch -- start` | `localhost:5150` | `http://localhost:5150` |
| `test`        | this repo           | `cargo test`, in-process      | none             | none                    |
| `staging`     | `/root/app/stg`     | `app-stg.service`             | `127.0.0.1:3201` | `https://app.ecqta.com` |
| `production`  | `/root/app/prod`    | `app-prod.service`            | `127.0.0.1:3200` | `https://example.com`   |

Production is the staging procedure below with the names swapped: folder `/root/app/prod`, unit `app-prod.service` with `LOCO_ENV=production` and `PORT=3200`, and the second Caddy block. Check both ports are free first (Step 0); other projects on the box own their ports.

Go-live checklist for the apex domain, once the site answers over https and `www` redirects: (1) if the domain is proxied through Cloudflare, set the zone's HSTS in SSL/TLS, Edge Certificates to max-age 12 months with "Apply to subdomains" and "Preload" on, matching the origin header in `config/production.yaml`; (2) submit the apex domain at https://hstspreload.org, which checks the live headers and queues the domain for the browsers' built-in list; (3) from then on every subdomain must be https, removal from the list takes months.

## What Gets Deployed

Four things, side by side in the deploy directory (details in `README.DEV.md`):

```
/root/app/stg/
├── app          <- Linux x86_64 build of the server binary
├── hash.txt     <- hashes of the bundle and stylesheet names; the app reads it next to the binary
├── config/      <- staging.yaml only; Loco reads the one file named by LOCO_ENV
└── site/        <- the cargo-leptos output (target/site locally): hashed bundle and stylesheet, fonts, 404 page
```

No `.env` file: Loco does not read one. Environment variables live in the systemd unit.

## Building the Linux Artifacts (from local terminal)

The server is x86_64; the Mac builds arm64 by default. The binary must be built for the server; `site/` is platform independent.

cargo-leptos builds the site folder natively; `cross` builds the binary inside a Linux container (`ghcr.io/cross-rs/x86_64-unknown-linux-gnu`, Ubuntu 24.04, glibc 2.39, below the server's 2.41 as required; emulated on Apple Silicon, so allow a few minutes on a cold build):

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --release --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/release/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site
```

`dist/` then holds `app`, `hash.txt` and `site/`. `LEPTOS_HASH_FILES=true` makes cargo-leptos hash the asset names (so production can cache them for a year) and write `hash.txt` into `target/release/`; the app refuses to boot when `site/` holds hashed names and that file is missing next to the binary. Verified end to end: the binary boots on Debian 13 amd64 and names the wasm file correctly, because `cross` reads `.cargo/config.toml` (the compile-time `LEPTOS_OUTPUT_NAME`) and `Cross.toml` (which pins the build image; see `PREREQUISITES.md` for the one-time pull) inside its container. The container image (`README.Docker.md`) is not part of this path; it exists for the compose deployment on a box of the app's own.

## One-Time Setup (on server)

### Step 0: Check the Port Is Free

```bash
ss -tlnp | grep ':3201\b' || echo "3201 free"
```

If something is listening, pick a free port and change it in two places only: `PORT=` in the unit file, and `reverse_proxy` in the Caddy block. Nothing else hardcodes it.

### Step 1: Create the Directory and Upload Files

On the server:

```bash
mkdir -p /root/app/stg/config
```

From the local terminal, after building `dist/`:

```bash
scp dist/app root@188.34.179.70:/root/app/stg/app &&
scp dist/hash.txt root@188.34.179.70:/root/app/stg/hash.txt &&
scp -r dist/site root@188.34.179.70:/root/app/stg/ &&
scp config/staging.yaml root@188.34.179.70:/root/app/stg/config/
```

Back on the server:

```bash
chmod +x /root/app/stg/app
```

### Step 2: Create the Service

```bash
nano /etc/systemd/system/app-stg.service
```

Paste:

```ini
[Unit]
Description=app staging
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/root/app/stg
ExecStart=/root/app/stg/app start
Restart=always
RestartSec=5
# Which config/<env>.yaml to load
Environment=LOCO_ENV=staging
# Loopback only: nothing but the shared Caddy on this box may reach it
Environment=BINDING=127.0.0.1
Environment=PORT=3201
# Leptos options (no Cargo.toml on the server); site/ is next to the binary
Environment=LEPTOS_OUTPUT_NAME=app
Environment=LEPTOS_SITE_ROOT=site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_ENV=PROD
# Secrets. Staging boots on the placeholder defaults in its config file;
# production has no defaults and needs all four.
# Environment=JWT_SECRET=
# Environment=MAILER_HOST=
# Environment=MAILER_USER=
# Environment=MAILER_PASSWORD=

[Install]
WantedBy=multi-user.target
```

`start` after the binary path is Loco's subcommand. Without it the binary prints its help and exits, and `Restart=always` would loop forever.

### Step 3: Enable and Start

```bash
systemctl daemon-reload && systemctl enable --now app-stg && systemctl status app-stg
```

If the unit fails at once with `one of the static path are not found`, the `site/` upload is missing or incomplete: `staging.yaml` refuses to boot without `site/` and `site/404.html`. If it fails with `a hashed build must be deployed together with its hash file`, `hash.txt` was not uploaded; if with `the hash file is stale`, `hash.txt` and `site/` come from different builds.

### GeoLite2

The shared `/root/data/GeoLite2/` directory on this box holds MaxMind's databases for the other projects; a project that needs GeoIP reads from there rather than shipping its own copy:

```bash
ls -la /root/data/GeoLite2/
```

## Regular Deploy (from local terminal)

Build `dist/` as above, then:

```bash
ssh root@188.34.179.70 "systemctl stop app-stg" &&
scp dist/app root@188.34.179.70:/root/app/stg/app &&
scp dist/hash.txt root@188.34.179.70:/root/app/stg/hash.txt &&
ssh root@188.34.179.70 "rm -rf /root/app/stg/site" &&
scp -r dist/site root@188.34.179.70:/root/app/stg/ &&
ssh root@188.34.179.70 "find /root/app/stg/site -name '.DS_Store' -delete" &&
scp config/staging.yaml root@188.34.179.70:/root/app/stg/config/ &&
ssh root@188.34.179.70 "systemctl start app-stg && systemctl status app-stg --no-pager"
```

`site/` is removed before upload so files deleted locally do not linger on the server, and macOS Finder files that cargo-leptos copied from `public/` are deleted after the upload.

## app-stg Commands (on server)

```bash
systemctl stop app-stg
systemctl start app-stg
systemctl restart app-stg
systemctl status app-stg
journalctl -u app-stg -f                 # logs
ss -tln | grep -E ':(80|443|3201)\b'     # ports
```

## Manual Run (without systemd)

Stop the unit first, otherwise `Restart=always` keeps a second copy fighting for the port:

```bash
systemctl stop app-stg
cd /root/app/stg
pkill -f /root/app/stg/app
LOCO_ENV=staging BINDING=127.0.0.1 PORT=3201 LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD nohup ./app start > app.log 2>&1 &
tail -f app.log
```

Always `pkill -f` with the full path. A bare `pkill app` would also kill any other process on the box whose name contains `app`.

## Caddy Config (`/etc/caddy/Caddyfile`)

The Caddyfile is shared with the other projects on this box. Add the app's block; never replace the file.

### Edit Caddyfile (on the server)

```bash
nano /etc/caddy/Caddyfile
```

### Edit Caddyfile (from local terminal)

```bash
ssh root@188.34.179.70 -t "nano /etc/caddy/Caddyfile"
```

### App Blocks

Staging:

```bash
app.ecqta.com:80, app.ecqta.com:443 {
        reverse_proxy 127.0.0.1:3201
        encode gzip zstd
}
```

Production, once deployed (`www.` redirects to the bare domain):

```bash
www.example.com {
        redir https://example.com{uri} permanent
}

example.com:80, example.com:443 {
        reverse_proxy 127.0.0.1:3200
        encode gzip zstd
}
```

Compression happens here, so Loco's compression middleware stays off in the config.

### Validate Caddy Config (on server)

```bash
ssh root@188.34.179.70 "caddy validate --config /etc/caddy/Caddyfile"
```

### Reload Caddy (from local terminal)

For Caddyfile changes, validate first, then graceful zero-downtime reload:

```bash
ssh root@188.34.179.70 "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy"
```

If validation fails, the reload won't run and Caddy stays up on the old config.

Use `restart` (not `reload`) only when updating the Caddy binary itself or recovering from a broken process:

```bash
ssh root@188.34.179.70 "systemctl restart caddy"
```
