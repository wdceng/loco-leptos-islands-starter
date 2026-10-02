# Production

|            |                                          |
| ---------- | ---------------------------------------- |
| Server     | `<server>`, the SSH login, e.g. `root@203.0.113.10` |
| Directory  | `/srv/app/prod`                          |
| Unit, user | `app-prod.service`, `app-prod`           |
| Port       | 3100                                     |
| URL        | `https://<domain>`, e.g. `https://example.com` |

Replace `<server>` and `<domain>` in the commands below. The directory, unit,
user and port are suggestions; if you change one, change it everywhere in
this file.

Deployed: the `app` binary, `hash.txt` next to it, `config/production.yaml`,
`site/`, `secrets.env`.

The reference setup is Cloudflare in front of Caddy in front of Loco. Caddy
alone works too, see "Without Cloudflare" at the end.

## Deploy

### Checks

```bash
cargo clippy --all-targets && cargo test &&
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings &&
cargo audit
```

The template's placeholders must be gone ("Make it yours" in `README.md`),
and `secrets.production.env` must exist and hold real values. Both lines
must print nothing:

```bash
grep -n "example\.com\|SaaS Starter" config/production.yaml src/views/layout.rs public/favicon/site.webmanifest
test -s secrets.production.env && grep -n "replace-me" secrets.production.env
```

The second line prints nothing when the file is ready. If the file is
missing it says nothing either, so check that `ls secrets.production.env`
finds it. The deploy chain below checks both.

Try the release build locally first (http://localhost:5150), then
`cargo leptos build` to go back to development:

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

### Build and deploy

```bash
cargo audit && ! grep -n "example\.com\|SaaS Starter" config/production.yaml src/views/layout.rs public/favicon/site.webmanifest &&
test -s secrets.production.env && ! grep -q "replace-me" secrets.production.env &&
cargo clippy --all-targets && cargo test &&
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --release --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/release/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site &&
ssh <server> "mkdir -p /srv/app/prod/config" &&
ssh <server> "systemctl stop app-prod" &&
ssh <server> "cd /srv/app/prod && { [ -f app ] && cp app app.prev; [ -f hash.txt ] && cp hash.txt hash.txt.prev; [ -d site ] && rm -rf site.prev && mv site site.prev; true; }" &&
scp dist/app <server>:/srv/app/prod/app &&
scp dist/hash.txt <server>:/srv/app/prod/hash.txt &&
scp -r dist/site <server>:/srv/app/prod/ &&
ssh <server> "find /srv/app/prod/site -name '.DS_Store' -delete" &&
scp config/production.yaml <server>:/srv/app/prod/config/ &&
scp secrets.production.env <server>:/srv/app/prod/secrets.env &&
ssh <server> "chmod 600 /srv/app/prod/secrets.env && systemctl start app-prod" &&
ssh <server> "journalctl -u app-prod -f"
```

- The first two lines are the production gates: `cargo audit`, no
  template placeholders, and a `secrets.production.env` that exists and
  has no `replace-me` left. A placeholder mail account would boot fine and
  only fail when the first mail is sent. A missing file would fail at the
  upload, after the service was already stopped.
- A failing step stops the chain. A failed build never touches the server.
- `--release` is a full, slow build every time.
- The previous deploy is kept as `*.prev` for the rollback below.
- `hash.txt` must travel with the binary and match `site/`, or the app
  won't start.

### Rollback

```bash
ssh <server> "systemctl stop app-prod && cd /srv/app/prod && mv app app.bad && mv app.prev app && mv hash.txt hash.txt.bad && mv hash.txt.prev hash.txt && rm -rf site.bad && mv site site.bad && mv site.prev site && systemctl start app-prod && systemctl status app-prod --no-pager"
```

The binary, `hash.txt` and `site/` always move together.

### Smoke test

On the server, straight at the app, so a Cloudflare challenge can't get in
the way:

```bash
ssh <server> 'cd /srv/app/prod && for p in / /robots.txt $(ls site/pkg | grep -v "\.ts$" | sed "s|^|/pkg/|"); do printf "%-40s " "$p"; curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:3100$p"; done && curl -s http://127.0.0.1:3100/ | grep -o "href=\"/pkg/[^\"]*\"" && curl -s http://127.0.0.1:3100/robots.txt && echo "X-Robots-Tag (must be empty):" && curl -sI http://127.0.0.1:3100/ | grep -i x-robots-tag; true'
```

- All 200, the linked `href` values among the listed files, `robots.txt`
  says `Allow: /`, no `X-Robots-Tag`.
- `https://<domain>` in the browser with the console open: no errors.
  `https://www.<domain>` lands on `https://<domain>`.
- The mail account logs in: right after every start the app logs in to
  SMTP once, sending nothing, and the journal says how it went.
  `ssh <server> "journalctl -u app-prod -b | grep 'smtp login check'"`
  should show `the mail server accepted the login`. A failure is a warning
  with the reason (wrong host, port or password) and the site keeps
  running, so look for it.
- Register a test account: the welcome mail arrives. If not,
  `journalctl -u app-prod` says why.

### Config-only change

```bash
scp config/production.yaml <server>:/srv/app/prod/config/ &&
ssh <server> "systemctl restart app-prod"
```

## Unit commands

```bash
ssh <server> "systemctl restart app-prod"
ssh <server> "systemctl status app-prod"
ssh <server> "journalctl -u app-prod -f"
ssh <server> "systemctl stop app-prod"             # down until the next reboot
ssh <server> "systemctl disable --now app-prod"    # down across reboots
ssh <server> "systemctl enable --now app-prod"     # back
```

## Server

- Debian 13 or Ubuntu 24.04 or newer (see the glibc check in Step 0).
- Caddy installed on the box, shared by every site on it.
- Cloudflare in front, proxied, SSL/TLS mode Full (strict).
- The firewall lets in only Cloudflare's addresses, and the origin key
  blocks other Cloudflare accounts. Both are below.
- The app keys each visitor on `CF-Connecting-IP`
  (`remote_ip.source: CfConnectingIp` in the config).
- Cloudflare adds HSTS and `X-Content-Type-Options` itself. To see what the
  app sends: `curl -sI http://127.0.0.1:3100/` on the server.
- Shared with other projects? Touch only this app's directory, unit, port
  and Caddy block.

| `LOCO_ENV` | Where           | Bind             | URL                        |
| ---------- | --------------- | ---------------- | -------------------------- |
| staging    | `/srv/app/stg`  | `127.0.0.1:3101` | `https://staging.<domain>` |
| production | `/srv/app/prod` | `127.0.0.1:3100` | `https://<domain>`         |

## Environment variables

In the unit: `LOCO_ENV`, `BINDING`, `PORT`, `LEPTOS_*`, `DATABASE_URL`. The
domain is `server.host` in `config/production.yaml`; `HOST=` in the unit
overrides it. Change one:

```bash
ssh -t <server> "nano /etc/systemd/system/app-prod.service" &&
ssh <server> "systemctl daemon-reload && systemctl restart app-prod"
```

- Keep `BINDING=127.0.0.1`. `0.0.0.0` would bypass Caddy.
- Secrets live in `secrets.production.env` in the repo root on your
  machine. It's git-ignored and every deploy uploads it as `secrets.env`.
  Staging has its own, `secrets.staging.env`. Never share one between the
  two.
- Production has no defaults for `JWT_SECRET`, `MAILER_HOST`,
  `MAILER_USER`, `MAILER_PASSWORD` and `MAILER_FROM`. A missing one stops
  the boot.

Create the file once, then put your real mail account in:

```bash
umask 077 && printf 'JWT_SECRET=%s\nMAILER_HOST=replace-me\nMAILER_USER=replace-me\nMAILER_PASSWORD=replace-me\nMAILER_FROM="App <replace-me>"\n' "$(openssl rand -base64 48)" > secrets.production.env
```

- `MAILER_FROM` is the sender, `Name <address>`, in double quotes: the
  manual run reads the file with the shell. A sender the mailer would
  reject, such as a name with a bare comma, stops the boot.
- **Gmail:** `MAILER_USER` is the address, `MAILER_PASSWORD` an app
  password (Google account, Security, 2-Step Verification, App passwords),
  never the account password. `MAILER_FROM` must use the same address, or
  Gmail rewrites it.
- **Postmark, SES, Mailgun** and the like want a sender they've verified.
- **Port:** 587 with STARTTLS. For implicit TLS on 465, add
  `MAILER_PORT=465` to the file and set `tls: implicit` under
  `mailer.smtp` in the config.
- A changed secret goes up with the next deploy, or right away:
  `scp secrets.production.env <server>:/srv/app/prod/secrets.env && ssh <server> "chmod 600 /srv/app/prod/secrets.env && systemctl restart app-prod"`.

## One-time setup

```text
/srv/app/prod/
├── app          Linux binary
├── hash.txt     next to the binary
├── config/      production.yaml only
├── site/        target/site
└── secrets.env  root-only, loaded by the unit
```

Everything is root's and read-only for the app. The only writable place is
`/var/lib/app-prod`, where the SQLite file lives. Back it up with
`sqlite3 /var/lib/app-prod/app_production.sqlite ".backup <copy>"`, never by
copying the file.

### Step 0: Port free, glibc new enough?

```bash
ssh <server> "ss -tlnp | grep ':3100\b' || echo '3100 free'"
ssh <server> "ldd --version | head -1"
```

A different port goes in two places only: `PORT=` in the unit and
`reverse_proxy` in Caddy.

The server's glibc must be at least the one the binary needs. With the
`:main` image in `Cross.toml` that's 2.39 today: Debian 13 and Ubuntu 24.04
work, Debian 12 and Ubuntu 22.04 fail with `version GLIBC_2.39 not found`.
Once `dist/` is built, this prints what the binary needs:

```bash
strings dist/app | grep -oE 'GLIBC_[0-9.]+' | sort -Vu | tail -1
```

### Step 1: Directory, user, files

```bash
ssh <server> "mkdir -p /srv/app/prod/config && useradd --system --user-group --no-create-home --shell /usr/sbin/nologin app-prod"
```

Build `dist/` with lines 4 to 9 of the deploy chain, then:

```bash
scp dist/app <server>:/srv/app/prod/app &&
scp dist/hash.txt <server>:/srv/app/prod/hash.txt &&
scp -r dist/site <server>:/srv/app/prod/ &&
scp config/production.yaml <server>:/srv/app/prod/config/ &&
scp secrets.production.env <server>:/srv/app/prod/secrets.env &&
ssh <server> "chmod +x /srv/app/prod/app && chmod 600 /srv/app/prod/secrets.env"
```

### Step 2: The unit

```bash
ssh <server> -t "nano /etc/systemd/system/app-prod.service"
```

```ini
[Unit]
Description=app production
After=network.target

[Service]
Type=simple
User=app-prod
WorkingDirectory=/srv/app/prod
ExecStart=/srv/app/prod/app start
Restart=always
RestartSec=5
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
StateDirectory=app-prod
Environment=DATABASE_URL=sqlite:///var/lib/app-prod/app_production.sqlite?mode=rwc
Environment=LOCO_ENV=production
Environment=BINDING=127.0.0.1
Environment=PORT=3100
Environment=LEPTOS_OUTPUT_NAME=app
Environment=LEPTOS_SITE_ROOT=site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_ENV=PROD
EnvironmentFile=/srv/app/prod/secrets.env

[Install]
WantedBy=multi-user.target
```

- **Keep `start`.** Without it the binary prints its help and
  `Restart=always` loops.
- **Keep `Restart=always`.** The app stops itself every night at
  `nightly_restart.hour` (03:00 UTC as shipped) and the unit brings it
  back. The journal shows `nightly restart scheduled for 03:00 UTC` at
  boot.
- **Secrets never go in the unit.** Anyone on the box can read unit files.

### Step 3: Start

```bash
ssh <server> "systemctl daemon-reload && systemctl enable --now app-prod && systemctl status app-prod"
```

| Boot error | Cause |
|---|---|
| `one of the static path are not found` | `site/` missing or incomplete |
| `a hashed build must be deployed together with its hash file` | `hash.txt` missing |
| `the hash file is stale` | `hash.txt` and `site/` from different builds |
| a `get_env` error naming `JWT_SECRET` or `MAILER_*` | `secrets.env` missing or incomplete |
| `settings.mail.from is not a sender` | `MAILER_FROM` must read `Name <address>` |
| `production won't start: MAILER_HOST, … empty or still replace-me` | a secret is set but not filled in (`src/deploy_checks.rs`) |
| `invalid settings: block: failed to parse timezone` | `nightly_restart.zone` isn't an IANA name (`UTC`, `Europe/Zagreb`) |
| `settings.nightly_restart.hour must be 0 to 23` | restart hour out of range |
| `unable to open database file` | `StateDirectory=` or `DATABASE_URL=` missing |
| `version GLIBC_2.39 not found` | server too old, see Step 0 |

### Step 4: Caddy

The origin key (below) must exist on the box and in the domain's Cloudflare
zone first, or Caddy drops every request.

Add this block to `/etc/caddy/Caddyfile`. Never replace the file, other
sites may share it. Without `www`, leave out the two `www.` addresses and
the redirect:

```bash
ssh <server> -t "nano /etc/caddy/Caddyfile"
```

```text
<domain>:80, <domain>:443, www.<domain>:80, www.<domain>:443 {
        import origin_check
        @www host www.<domain>
        redir @www https://<domain>{uri} permanent
        reverse_proxy 127.0.0.1:3100 {
                header_up -X-Origin-Key
        }
        encode gzip zstd
}
```

```bash
ssh <server> "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy"
```

If validation fails, Caddy keeps running on the old config.

## Firewall (once per server)

Only Cloudflare may reach ports 80 and 443. Otherwise a request straight to
the server's IP can fake `CF-Connecting-IP` and dodge the rate limiter.

```bash
ssh <server> 'apt install -y ufw && ufw allow OpenSSH && for ip in $(curl -s https://www.cloudflare.com/ips-v4) $(curl -s https://www.cloudflare.com/ips-v6); do ufw allow proto tcp from $ip to any port 80,443; done && ufw --force enable && ufw status'
```

- **`ufw allow OpenSSH` comes first.** Without it, enabling the firewall
  locks you out.
- Shared box? Every site on it now needs Cloudflare in front.
- Cloudflare's list rarely changes. If it does, run the loop again.

## The origin key (once per server)

Cloudflare adds a secret header, `X-Origin-Key`, to every request. Caddy's
`origin_check` snippet drops requests without it. This stops another
Cloudflare account from reaching your server through Cloudflare's own
addresses.

Create the key and the snippet on the box:

```bash
ssh <server> 'K=$(openssl rand -hex 32) && printf "(origin_check) {\n\t@no_key not header X-Origin-Key %s\n\tabort @no_key\n}\n" "$K" > /etc/caddy/origin-check.caddy && chown root:caddy /etc/caddy/origin-check.caddy && chmod 640 /etc/caddy/origin-check.caddy'
```

Then add `import /etc/caddy/origin-check.caddy` near the top of
`/etc/caddy/Caddyfile`, once: after the global options block `{ ... }` if
there is one, before any site block.

**The key lives only on the server and in Cloudflare's rules.** Never in
git, a chat or a screenshot. A new key means editing every zone's rule.

In each Cloudflare zone that points at this server: Rules, Create rule,
Request Header Transform Rule, name `Origin key`, all incoming requests,
set static header `X-Origin-Key`, value from the clipboard:

```bash
ssh <server> 'grep -o "[0-9a-f]\{64\}" /etc/caddy/origin-check.caddy' | tr -d '\n' | pbcopy && echo "key copied to the clipboard"
```

Check it, with the key masked in the output: `000` without the key, `200`
with it.

```bash
ssh <server> 'K=$(grep -o "[0-9a-f]\{64\}" /etc/caddy/origin-check.caddy); h=<domain>; curl -sk -m 5 --resolve $h:443:127.0.0.1 -o /dev/null -w "without key: %{http_code}\n" https://$h/; curl -sk -m 5 --resolve $h:443:127.0.0.1 -H "X-Origin-Key: $K" -o /dev/null -w "with key: %{http_code}\n" https://$h/'
```

## Manual run (without systemd)

```bash
ssh <server> -t "systemctl stop app-prod && cd /srv/app/prod && { pkill -f '^/srv/app/prod/app ' || true; } && set -a && . ./secrets.env && set +a && LOCO_ENV=production BINDING=127.0.0.1 PORT=3100 LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD DATABASE_URL=sqlite:///var/lib/app-prod/app_production.sqlite?mode=rwc nohup runuser -u app-prod -- /srv/app/prod/app start > /srv/app/prod/app.log 2>&1 & sleep 1; tail -f /srv/app/prod/app.log"
```

- **Always `pkill -f '^/full/path '`.** A bare `pkill app` also kills
  staging.
- At the nightly restart hour the app exits and stays down. Fine for a
  short session; `systemctl start app-prod` when done.

## Go-live (once)

1. The origin-key rule in the domain's Cloudflare zone.
2. DNS for `<domain>` (and `www`) to the server, proxied. Leave any MX,
   SPF and verification records alone.
3. The Caddy block (Step 4), validated and reloaded.
4. Once https works: Cloudflare, SSL/TLS, Edge Certificates, HSTS 12
   months, subdomains and preload on. That matches the header in
   `config/production.yaml`.
5. Submit `<domain>` at https://hstspreload.org. **Last on purpose:** from
   then on every subdomain must be https, and removal takes months.

## Without Cloudflare

With only Caddy in front, `CfConnectingIp` breaks the rate limiter: any
client can send its own `CF-Connecting-IP` and pick its bucket, and every
client that sends none shares Caddy's. The limiter also refuses to start a
deployed environment keyed on the TCP peer, which behind a proxy is the
proxy.

- Leave out the firewall loop, `import origin_check` and the `header_up`
  line.
- Switch to `X-Forwarded-For`, which Loco calls `RightmostXForwardedFor`:
  extend `KeySource` and its config-to-key mapping in
  `src/middleware/rate_limit.rs`, and update the limiter's request tests and
  `tests/config.rs`, which pin the current behaviour.
