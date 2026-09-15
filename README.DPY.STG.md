# Deployment (staging)

Target: falkenstein-1 (`root@188.34.179.70`), directory `/root/app/stg`, unit `app-stg.service`, port 3201, https://app.ecqta.com. The `app` names are the template's; a project replaces them with its own (README.md, "After Creating a Project").

One-time server setup (directory, service, Caddy block, first upload): see `README.DPY-falkenstein-1.md`. What gets deployed is four things: the `app` binary, `hash.txt` next to it, `config/`, and `site/` (the cargo-leptos output). Minification is not a step: a release build minifies the stylesheet, the JS glue and the wasm itself.

`hash.txt` exists because release builds run with `LEPTOS_HASH_FILES=true`: cargo-leptos names the outputs `app.<hash>.css/.js/.wasm` and records the hashes in that file, which the app reads at boot (`src/assets.rs`) to link the right names. Every deploy is therefore a new URL for each asset, which is what lets production cache them for a year. The binary refuses to boot when `site/` holds hashed names and the file is missing next to it, or when the two come from different builds.

## Before Deploying

### Lint, Tests, Browser-Half Check

```bash
cargo clippy --all-targets && cargo test &&
cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

### Try the Release Build Locally

The same optimised build that ships, served on http://localhost:5150 by the release binary itself (`cargo loco start` would rebuild and run the debug binary instead):

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release && ./target/release/app start
```

Check the page, the browser console, and the smoke-test loop at the bottom of this file against `localhost:5150` before uploading. The release binary finds `target/release/hash.txt` next to itself and serves the hashed names. Afterwards `target/site` holds hashed files that the debug binary cannot name, so run `cargo leptos build` (or the watch loop) before going back to development. The day-to-day dev loop, `cargo leptos watch -- start`, is in `README.DEV.md`.

## Deploy

### Build and Deploy (one pipeline)

The first half builds the Linux artifacts into `dist/`: the site folder (wasm, stylesheet, `public/` files) is platform independent and built natively by cargo-leptos; the server binary is built for Linux x86_64 by `cross` inside a Linux container (`ghcr.io/cross-rs/x86_64-unknown-linux-gnu`, emulated on Apple Silicon, so allow a few minutes on a cold build). The second half stops the unit, uploads `dist/` and `config/`, starts the unit and follows its log. Any failing step aborts the chain, so a failed build never touches the server.

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only &&
cross build --release --target x86_64-unknown-linux-gnu &&
rm -rf dist && mkdir dist &&
cp target/x86_64-unknown-linux-gnu/release/app dist/app &&
cp target/release/hash.txt dist/hash.txt &&
cp -r target/site dist/site &&
ssh root@188.34.179.70 "systemctl stop app-stg" &&
scp dist/app root@188.34.179.70:/root/app/stg/app &&
scp dist/hash.txt root@188.34.179.70:/root/app/stg/hash.txt &&
ssh root@188.34.179.70 "rm -rf /root/app/stg/site" &&
scp -r dist/site root@188.34.179.70:/root/app/stg/ &&
ssh root@188.34.179.70 "find /root/app/stg/site -name '.DS_Store' -delete" &&
scp config/staging.yaml root@188.34.179.70:/root/app/stg/config/ &&
ssh root@188.34.179.70 "systemctl start app-stg" &&
ssh root@188.34.179.70 "journalctl -u app-stg -f"
```

The `find` after the upload removes macOS Finder files from the server's `site/`: cargo-leptos copies `public/` verbatim, and `.gitignore` only keeps such files out of git, not out of the copy. `dist/` is rebuilt from scratch so nothing from an older build survives, and `site/` on the server is removed before upload for the same reason. `hash.txt` is written by the frontend build into `target/release/` (cargo-leptos puts it where the native binary would be) and must travel with the binary: the app looks for it next to the executable. Only `config/staging.yaml` is uploaded: Loco reads the one file named by `LOCO_ENV` and nothing else, so the other environments' files stay off the server.

Verified end to end: the `cross`-built binary boots on Debian 13 amd64 and names the wasm file correctly, because `cross` reads `.cargo/config.toml` (the compile-time `LEPTOS_OUTPUT_NAME`) and `Cross.toml` (which pins the build image; see `PREREQUISITES.md` for the one-time pull) inside its container. The container image (`README.Docker.md`) is not part of this path; it exists for the compose deployment on a box of the app's own.

### Config-Only Change

Same as above without the binary and site steps:

```bash
scp config/staging.yaml root@188.34.179.70:/root/app/stg/config/ &&
ssh root@188.34.179.70 "systemctl restart app-stg"
```

Loco reads `config/staging.yaml` at boot, so a restart is required.

## Environment Variables

There is no `.env` file; Loco does not read one. The variables live in the systemd unit (`LOCO_ENV`, `BINDING`, `PORT`, `LEPTOS_*`, and the secrets `JWT_SECRET`, `MAILER_*`). To change one:

```bash
ssh -t root@188.34.179.70 "nano /etc/systemd/system/app-stg.service" &&
ssh root@188.34.179.70 "systemctl daemon-reload && systemctl restart app-stg"
```

Keep `BINDING=127.0.0.1` on the server: `0.0.0.0` would bind the port publicly and bypass Caddy.

## GeoLite2

A project that needs GeoIP reads the shared `/root/data/GeoLite2/` on the server; see `README.DPY-falkenstein-1.md`.

## app-stg Commands (from local terminal)

```bash
ssh root@188.34.179.70 "systemctl stop app-stg"
ssh root@188.34.179.70 "systemctl start app-stg"
ssh root@188.34.179.70 "systemctl restart app-stg"
ssh root@188.34.179.70 "systemctl status app-stg"
ssh root@188.34.179.70 "journalctl -u app-stg -f"
```

## Smoke Test After Deploy

`app.ecqta.com` is proxied through Cloudflare with its JavaScript challenge on for the whole `ecqta.com` zone, so curl from outside gets a 403 "Just a moment..." page regardless of what the site does. Test on the server, straight at the Loco port:

```bash
ssh root@188.34.179.70 'cd /root/app/stg && for p in / /robots.txt $(ls site/pkg | grep -v "\.ts$" | sed "s|^|/pkg/|"); do printf "%-40s " "$p"; curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:3201$p"; done && curl -s http://127.0.0.1:3201/ | grep -o "href=\"/pkg/[^\"]*\""'
```

All 200, and the `href` values printed last must be among the hashed files listed above them (the page links what the deploy shipped). Then open https://app.ecqta.com in a browser with the console open: the page renders, no errors, and the wasm and JS requests show in the network tab.
