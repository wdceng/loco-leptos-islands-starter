# Dev-server

|            |                                          |
| ---------- | ---------------------------------------- |
| Server     | `<server>`, the SSH login, e.g. `root@203.0.113.10` |
| Directory  | `/srv/app/dev`                           |
| Unit, user | `app-dev.service`, `app-dev`             |
| Port       | 3102                                     |
| URL        | `https://<domain>`, e.g. `https://dev.example.com` |

Replace `<server>` and `<domain>` in the commands below. The directory, unit,
user and port are suggestions; if you change one, change it everywhere in
this file.

Deployed: the `app` binary, `hash.txt` next to it, `config/dev-server.yaml`,
`site/`, `secrets.env`.

The dev server is staging's twin: the same settings, its own host, port,
directory, unit, database and secrets. Deploy work in progress here and
keep staging for what is about to go live.

| | dev-server | staging |
|---|---|---|
| URL | `https://dev.example.com` | `https://stg.example.com` |
| Port | 3102 | 3101 |
| Directory, unit | `/srv/app/dev`, `app-dev` | `/srv/app/stg`, `app-stg` |
| Secrets | `secrets.dev-server.env` | `secrets.staging.env` |

Everything else is staging's (`docs/staging.md`), and the same differences
from production apply. The firewall, the origin key and the mail account
are set up once per server and described in `docs/production.md`.

## Deploy

### Checks

```bash
cargo clippy --all-targets && cargo test &&
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

The template's placeholders are fine on the dev server, not on production.

Try the release build locally first (http://localhost:5151). It builds into
`target/site-release`, so the watch loop on 5150 keeps running untouched:

```bash
LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-release cargo leptos build --release &&
LEPTOS_SITE_ROOT=target/site-release PORT=5151 ./target/release/app start
```

### Build and deploy

```bash
cargo clippy --all-targets && cargo test &&
LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-release cargo leptos build --release --frontend-only &&
cross build --profile staging --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/staging/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site-release dist/site &&
ssh <server> "mkdir -p /srv/app/dev/config" &&
ssh <server> "systemctl stop app-dev" &&
scp dist/app <server>:/srv/app/dev/app &&
scp dist/hash.txt <server>:/srv/app/dev/hash.txt &&
ssh <server> "rm -rf /srv/app/dev/site" &&
scp -r dist/site <server>:/srv/app/dev/ &&
ssh <server> "find /srv/app/dev/site -name '.DS_Store' -delete" &&
scp config/dev-server.yaml <server>:/srv/app/dev/config/ &&
scp secrets.dev-server.env <server>:/srv/app/dev/secrets.env &&
ssh <server> "chmod 600 /srv/app/dev/secrets.env && systemctl start app-dev" &&
ssh <server> "journalctl -u app-dev -f"
```

- A failing step stops the chain. A failed build never touches the server.
- The `staging` profile is faster to rebuild than `--release` and keeps
  line numbers in backtraces. The dev server shares it.
- `hash.txt` must travel with the binary and match `site/`, or the app
  won't start.
- The old `site/` is deleted first, so files you removed locally don't
  linger.

### Production build on the dev server

To run production's exact binary here, same chain, two lines changed
(always both together):

```bash
cross build --release --target x86_64-unknown-linux-gnu &&
cp target/x86_64-unknown-linux-gnu/release/app dist/app &&
```

### Smoke test

A Cloudflare challenge blocks curl from outside, so test on the server:

```bash
ssh <server> 'cd /srv/app/dev && for p in / /robots.txt $(ls site/pkg | grep -v "\.ts$" | sed "s|^|/pkg/|"); do printf "%-40s " "$p"; curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:3102$p"; done && curl -s http://127.0.0.1:3102/ | grep -o "href=\"/pkg/[^\"]*\"" && curl -s http://127.0.0.1:3102/robots.txt && curl -sI http://127.0.0.1:3102/ | grep -i x-robots-tag'
```

All 200, the linked `href` values among the listed files, `Disallow: /` and
`X-Robots-Tag: noindex, nofollow`. The app also logs in to SMTP once at
every start: `ssh <server> "journalctl -u app-dev -b | grep 'smtp login check'"`.
On placeholder mail settings that's a warning, which is expected until
the dev server should send real mail. Then open `https://<domain>` with the
browser console open: no errors.

### Config-only change

```bash
scp config/dev-server.yaml <server>:/srv/app/dev/config/ &&
ssh <server> "systemctl restart app-dev"
```

## Unit commands

```bash
ssh <server> "systemctl restart app-dev"
ssh <server> "systemctl status app-dev"
ssh <server> "journalctl -u app-dev -f"
ssh <server> "systemctl stop app-dev"             # down until the next reboot
ssh <server> "systemctl disable --now app-dev"    # down across reboots
ssh <server> "systemctl enable --now app-dev"     # back
```

## Server

Same box, Caddy and Cloudflare setup as production (`docs/production.md`),
plus:

- **A gate in front.** In the Cloudflare zone, add `<domain>` to the list
  in the custom rule `Staging hosts: challenge` (`docs/staging.md`).
  Visitors pass a check, scanners don't.
- To see what the app sends without Cloudflare's headers:
  `curl -sI http://127.0.0.1:3102/` on the server.

## Environment variables

In the unit: `LOCO_ENV`, `BINDING`, `PORT`, `LEPTOS_*`, `DATABASE_URL`. The
domain is `server.host` in `config/dev-server.yaml`; `HOST=` in the unit
overrides it. Change one:

```bash
ssh -t <server> "nano /etc/systemd/system/app-dev.service" &&
ssh <server> "systemctl daemon-reload && systemctl restart app-dev"
```

- Keep `BINDING=127.0.0.1`. `0.0.0.0` would bypass Caddy.
- Secrets live in `secrets.dev-server.env` in the repo root on your
  machine. It's git-ignored and every deploy uploads it as `secrets.env`.
  Never use staging's or production's file here.
- An empty file boots on the placeholders in `config/dev-server.yaml`; mail
  then fails to send. A missing file stops the unit.

Create it once with its own `JWT_SECRET`. Add the `MAILER_*` lines when
the dev server should send real mail (details in `docs/production.md`):

```bash
umask 077 && printf 'JWT_SECRET=%s\n' "$(openssl rand -base64 48)" > secrets.dev-server.env
```

## One-time setup

```text
/srv/app/dev/
├── app          Linux binary
├── hash.txt     next to the binary
├── config/      dev-server.yaml only
├── site/        target/site-release
└── secrets.env  root-only, loaded by the unit
```

Everything is root's and read-only for the app. The only writable place is
`/var/lib/app-dev`, where the SQLite file lives.

### Step 0: Port free, glibc new enough?

```bash
ssh <server> "ss -tlnp | grep ':3102\b' || echo '3102 free'"
ssh <server> "ldd --version | head -1"
```

A different port goes in two places only: `PORT=` in the unit and
`reverse_proxy` in Caddy. For glibc, see Step 0 in `docs/production.md`.

### Step 1: Directory, user, files

```bash
ssh <server> "mkdir -p /srv/app/dev/config && useradd --system --user-group --no-create-home --shell /usr/sbin/nologin app-dev"
```

Build `dist/` with lines 2 to 7 of the deploy chain, then:

```bash
scp dist/app <server>:/srv/app/dev/app &&
scp dist/hash.txt <server>:/srv/app/dev/hash.txt &&
scp -r dist/site <server>:/srv/app/dev/ &&
scp config/dev-server.yaml <server>:/srv/app/dev/config/ &&
scp secrets.dev-server.env <server>:/srv/app/dev/secrets.env &&
ssh <server> "chmod +x /srv/app/dev/app && chmod 600 /srv/app/dev/secrets.env"
```

### Step 2: The unit

```bash
ssh <server> -t "nano /etc/systemd/system/app-dev.service"
```

```ini
[Unit]
Description=app dev-server
After=network.target

[Service]
Type=simple
User=app-dev
WorkingDirectory=/srv/app/dev
ExecStart=/srv/app/dev/app start
Restart=always
RestartSec=5
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
StateDirectory=app-dev
Environment=DATABASE_URL=sqlite:///var/lib/app-dev/app_dev-server.sqlite?mode=rwc
Environment=LOCO_ENV=dev-server
Environment=BINDING=127.0.0.1
Environment=PORT=3102
Environment=LEPTOS_OUTPUT_NAME=app
Environment=LEPTOS_SITE_ROOT=site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_ENV=PROD
EnvironmentFile=/srv/app/dev/secrets.env

[Install]
WantedBy=multi-user.target
```

- **Keep `start`.** Without it the binary prints its help and
  `Restart=always` loops.
- **Keep `Restart=always`.** The app stops itself every night at
  `nightly_restart.hour` (03:00 UTC as shipped) and the unit brings it
  back.

### Step 3: Start

```bash
ssh <server> "systemctl daemon-reload && systemctl enable --now app-dev && systemctl status app-dev"
```

| Boot error | Cause |
|---|---|
| `the site folder ... is missing` | `site/` not uploaded next to the binary |
| `a hashed build must be deployed together with its hash file` | `hash.txt` missing |
| `the hash file is stale` | `hash.txt` and `site/` from different builds |
| `settings.mail.from is not a sender` | `MAILER_FROM` must read `Name <address>` |
| `invalid settings: block: failed to parse timezone` | `nightly_restart.zone` isn't an IANA name (`UTC`, `Europe/Zagreb`) |
| `unable to open database file` | `StateDirectory=` or `DATABASE_URL=` missing |
| `version GLIBC_2.39 not found` | server too old, see Step 0 |

### Step 4: Caddy

Cloudflare DNS: an `A` record for `<domain>` to the server, proxied. The
origin key must already be set up (`docs/production.md`).

Add this block to `/etc/caddy/Caddyfile` (never replace the file):

```bash
ssh <server> -t "nano /etc/caddy/Caddyfile"
```

```text
<domain>:80, <domain>:443 {
        import origin_check
        reverse_proxy 127.0.0.1:3102 {
                header_up -X-Origin-Key
        }
        encode gzip zstd
}
```

```bash
ssh <server> "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy"
```

Then add `<domain>` to the Cloudflare rule `Staging hosts: challenge`. Until
then the host is open to scanners.

## Manual run (without systemd)

```bash
ssh <server> -t "systemctl stop app-dev && cd /srv/app/dev && { pkill -f '^/srv/app/dev/app ' || true; } && set -a && . ./secrets.env && set +a && LOCO_ENV=dev-server BINDING=127.0.0.1 PORT=3102 LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD DATABASE_URL=sqlite:///var/lib/app-dev/app_dev-server.sqlite?mode=rwc nohup runuser -u app-dev -- /srv/app/dev/app start > /srv/app/dev/app.log 2>&1 & sleep 1; tail -f /srv/app/dev/app.log"
```

**Always `pkill -f '^/full/path '`.** A bare `pkill app` also kills
staging and production.
