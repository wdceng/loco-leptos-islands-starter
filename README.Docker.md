# Docker

The image is the Linux build of the site: `Dockerfile` compiles both halves
with cargo-leptos in a builder stage and ships only the binary, `config/` and
`site/` on Debian slim, running as an unprivileged user. `compose.yml` adds
Caddy in front for HTTPS (`Caddyfile` in the repo root). Image size is about
170 MB.

## Build

```bash
docker build -t app .
```

The first build compiles every crate and takes several minutes. Cache mounts
keep the cargo registry, the compiled `target/` and cargo-leptos' downloaded
tools between builds, so later builds take seconds plus the changed crates.

On Apple Silicon this produces an arm64 image. For an x86_64 server build on
the server, in CI, or with emulation (slow):
```bash
docker build --platform linux/amd64 -t app .
```

## Run the Site Container Alone

Staging config, no HTTPS, reachable on http://localhost:5152:
```bash
docker run -d --rm -p 5152:5150 -e LOCO_ENV=staging --name app-test app
```

Production config (host defaults to `https://example.com`, see `config/production.yaml`; the secrets it requires must be passed with `-e`):
```bash
docker run -d --rm -p 5152:5150 -e LOCO_ENV=production --name app-test app
```

Smoke test, stop:
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5152/ && curl -s -o /dev/null -w '%{http_code}\n' "http://localhost:5152$(curl -s http://localhost:5152/ | grep -o 'href="/pkg/app[^"]*\.css"' | cut -d'"' -f2)"
docker stop app-test
```

The second request fetches whatever stylesheet the page links: the image is a release build with hashed asset names (`pkg/app.<hash>.css`), so the name changes with every build.

## Compose: Site plus Caddy (HTTPS)

Values per deployment, as environment variables or in a `.env` file next to
`compose.yml`:

| Variable | Default | Meaning |
|---|---|---|
| `LOCO_ENV` | `staging` | which `config/<env>.yaml` the site uses |
| `HOST` | `https://staging.example.com` | public origin the site believes it is served at |
| `SITE_ADDRESS` | `staging.example.com` | domain Caddy serves and obtains a certificate for |

```bash
docker compose up -d --build     # build the site image, start both containers
docker compose logs -f           # follow both logs
docker compose down              # stop and remove containers (certificates persist in a volume)
```

Caddy needs ports 80 and 443 free on the host and a DNS record for
`SITE_ADDRESS` pointing at it; it then obtains the certificate itself. For a
local trial, `SITE_ADDRESS=localhost docker compose up -d --build` gives a
self-signed certificate on https://localhost.

## Container Management

```bash
docker ps                          # running containers
docker ps -a                       # including stopped
docker logs -f app-test            # follow one container's log
docker logs --tail 100 app-test    # last 100 lines
docker exec app-test id            # confirms the process runs as `app`, not root
docker stop app-test / docker start app-test / docker rm -f app-test
```

## Images and Cleanup

```bash
docker images app                  # local images of the site
docker pull caddy:2.10-alpine      # update the proxy image
docker system prune                # remove unused containers, networks, images
docker builder prune               # also drop the build cache mounts (next build is cold)
```
