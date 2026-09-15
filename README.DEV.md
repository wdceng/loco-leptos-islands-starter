# Development Guide

Tools are listed in `PREREQUISITES.md`; stack and architecture in `README.md`
and `CLAUDE.md`. This file is the day-to-day command reference.

## Run

### Full dev loop (recommended)
```bash
cargo leptos watch -- start
```
Rebuilds the server, the wasm bundle and the Tailwind stylesheet on every
change, restarts Loco, and reloads the browser tab. Serves on
http://localhost:5150. The `-- start` is Loco's subcommand; without it the
binary prints its help and exits.

### Server only
```bash
cargo loco start          # one run
cargo loco watch          # rebuild + restart on change (needs cargo-watch)
```
Picks up Rust changes, including page markup, but does not rebuild the wasm
bundle or the stylesheet. Fine for content work after one `cargo leptos build`.

Both loops start the same binary on the same port, so run one at a time.
Another port: `PORT=5151 cargo leptos watch -- start`.

### Find and monitor the process
```bash
ps aux | grep 'app start'
top -pid <PID>
```

## Inspect

```bash
cargo loco routes                          # every URL the app answers
cargo loco middleware                      # middleware on/off per config
cargo loco doctor                          # config and environment check
LOCO_ENV=staging cargo loco middleware     # same, for another environment
```

## Lint and Format

```bash
cargo clippy --all-targets
cargo fmt
```

Compile check of the browser half on its own. Catches a server-only
dependency leaking out of the `ssr` gate, which the normal build would not:
```bash
cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

## Tests

```bash
cargo test                        # all
cargo test home_renders_html      # one
```
Request tests boot the app in-process with `config/test.yaml`; no server or
browser needed. Details in `README.TESTING.md`.

## Release Builds

### Local (macOS)
```bash
LEPTOS_HASH_FILES=true cargo leptos build --release
```
Produces `target/release/app`, `target/release/hash.txt` and `target/site/`
with hashed names (`pkg/app.<hash>.css` and so on). The wasm uses the
`wasm-release` profile plus wasm-opt; for reference the bundle is about 66 KB
raw, 28 KB gzipped, before any islands exist.

`LEPTOS_HASH_FILES=true` is what every release build uses: cargo-leptos
renames its outputs after their content hash and writes the hashes to
`hash.txt` next to the binary. At boot `src/assets.rs` looks for that file
beside the executable; if present, the page links the hashed names, if
absent, the plain ones. Hashing is never used in the watch loop, which only
hashes on its first build and would go stale afterwards.

### Linux server
The server binary must be built for Linux; the `site/` folder and `hash.txt`
are platform independent. The deploy path (details in `README.DEPLOY.md`):

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only   # target/site + target/release/hash.txt, native
cross build --release --target x86_64-unknown-linux-gnu               # target/x86_64-unknown-linux-gnu/release/app
```

The cross-built binary names the wasm file correctly because `cross` reads
`.cargo/config.toml` (and `Cross.toml`, which pins its build image) inside
its container. About three minutes for a cold build under emulation on Apple
Silicon.

## Environments

`LOCO_ENV` selects `config/<env>.yaml`. No `.env` file is read; secrets come
from environment variables through `get_env` inside the YAML.

| Environment | Used by | Static folder | Static cache | Host |
|---|---|---|---|---|
| `development` | local runs (default) | `target/site` | `no-cache`: every refresh revalidates | localhost |
| `test` | `cargo test` | none | none | none |
| `staging` | online test copy | `site/` | 60 s | `https://staging.example.com` (default, set `HOST`) |
| `production` | live site | `site/` | one year, immutable (names are hashed) | `https://example.com` (default, set `HOST`) |

The cache value is `static.cache_control` in each `config/<env>.yaml`.
Production's year is safe only because release builds hash the asset names;
it also covers the fonts, so a changed font file needs a new file name.

### Deploy layout
```
app                 <- target/release/app, Linux build
hash.txt            <- target/release/hash.txt, must sit next to the binary
config/<env>.yaml
site/               <- target/site, renamed
```
Start from that folder:
```bash
LOCO_ENV=staging LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg ./app start
```
Loco serves plain HTTP on 5150; TLS is the reverse proxy's job.

## Gotchas

- After a hashed release build, `target/site` holds `app.<hash>.css` and
  friends, which the debug binary (no `hash.txt` beside it) cannot name:
  `cargo loco start` refuses to boot and says so. Run `cargo leptos build`
  or the watch loop first; both rewrite `target/site` with plain names.
- `.cargo/config.toml` sets `LEPTOS_OUTPUT_NAME` for every cargo command in
  this project. Leptos bakes the wasm file name in at compile time from it;
  without it a plain `cargo` build names the file `app_bg.wasm` while
  cargo-leptos writes `app.wasm`. Do not remove it.
- Switching between a cargo-leptos build and a plain cargo build recompiles a
  handful of crates because cargo-leptos sets a few more compile-time
  variables. Costs seconds, not correctness.
- `target/site` is wiped on every cargo-leptos build. Static files belong in
  `public/`, which is copied in.
- Two preludes export a type called `Error`. In files that use both Loco and
  Leptos, import Leptos items by name instead of `leptos::prelude::*`.
- `as` and `type` are Rust keywords: inside `view!` write `r#as` and `r#type`.
