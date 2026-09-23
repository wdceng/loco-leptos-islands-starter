# Development Guide

The commands you will use day to day, and the things that trip people up.
If you have not run the project yet, start with "Run it in five minutes" in
`README.md`; this file assumes that worked. Tools are listed in
`PREREQUISITES.md`, the reasoning behind the stack in `ARCHITECTURE.md`.

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

Handy when something does not behave as expected: what URLs exist, which
middlewares are on, whether the config is valid.

```bash
cargo loco routes                          # every URL the app answers
cargo loco middleware                      # middleware on/off per config
cargo loco middleware -c                   # the same, with each middleware's settings
cargo loco doctor                          # config and environment check
LOCO_ENV=staging cargo loco middleware     # same, for another environment
LOCO_CONFIG_FOLDER=/path/to/copy cargo loco start   # boot against an edited copy of config/, e.g. to see a refusal
```

## Lint and Format

Clippy is Rust's linter and catches real bugs, not just style; `cargo fmt`
formats the code the standard way. CI runs both, so run them before a push.

```bash
cargo clippy --all-targets
cargo fmt
```

Lint the browser half on its own. It catches a server-only dependency
leaking out of the `ssr` gate, and it is the only pass that sees code under
`hydrate` (the islands); the normal clippy run never compiles it:
```bash
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

## Database and Tasks

```bash
cargo loco db status              # which migrations have run
cargo loco db migrate             # apply pending migrations
cargo loco db reset               # drop every table and reapply all migrations (development only)
cargo loco db entities            # regenerate src/models/_entities/ from the schema (needs sea-orm-cli)
cargo loco task user_create       # run a task; `cargo loco task` alone lists them
```
`auto_migrate: true` in every config applies pending migrations at boot, so
`migrate` by hand is for a stopped server or a fresh database file.
`entities` is the one command that needs an extra tool, `sea-orm-cli`
(`PREREQUISITES.md`).

## Tests

```bash
cargo test                        # all
cargo test home_renders_html      # one
```
Request tests boot the app in-process with `config/test.yaml`; no server or
browser needed. Details in `TESTING.md`.

## Release Builds

The dev loop builds fast and unoptimised. A release build is the opposite:
slow to build, small and fast to run, and it is what you put on a server.
The only extra thing to remember is the `LEPTOS_HASH_FILES=true` prefix,
explained below; forget it and production will cache stale files.

### Local
```bash
LEPTOS_HASH_FILES=true cargo leptos build --release
```
Produces `target/release/app`, `target/release/hash.txt` and `target/site/`
with hashed names (`pkg/app.<hash>.css` and so on). The wasm uses the
`wasm-release` profile plus wasm-opt. For reference: with no island the
bundle is the hydration loader alone, about 66 KB raw, 28 KB gzipped. The
first island brings in the Leptos reactive runtime, about 125 KB raw, 52 KB
gzipped, 44 KB Brotli, plus 14 KB of JS glue (4.5 KB gzipped). Later islands
share that runtime, so they cost far less than the first.

`LEPTOS_HASH_FILES=true` is what every release build uses: cargo-leptos
renames its outputs after their content hash and writes the hashes to
`hash.txt` next to the binary. At boot `src/assets.rs` looks for that file
beside the executable; if present, the page links the hashed names, if
absent, the plain ones. Hashing is never used in the watch loop, which only
hashes on its first build and would go stale afterwards.

### Linux server
A binary built on your Mac or Windows machine will not run on a Linux server,
so the server binary is built for Linux with `cross`, which does the build
inside a Linux container. The `site/` folder and `hash.txt` are just files
and work anywhere, so they are built natively. The full walkthrough is in
`DEPLOYMENT.md`; the two build steps are:

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only   # target/site + target/release/hash.txt, native
cross build --release --target x86_64-unknown-linux-gnu               # target/x86_64-unknown-linux-gnu/release/app
```

For staging, swap `--release` on the second line for `--profile staging`
and the binary lands in `target/x86_64-unknown-linux-gnu/staging/app`. The
`staging` profile in `Cargo.toml` links faster, rebuilds incrementally and
keeps line tables for readable backtraces; `release` is the fully optimised
production build. The frontend half always uses `--release`.

The cross-built binary names the wasm file correctly because `cross` reads
`.cargo/config.toml` (and `Cross.toml`, which pins its build image) inside
its container. On a host that has to emulate x86_64 (Apple Silicon, for
one), allow a few minutes for a cold build.

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
secrets.env         <- JWT_SECRET and MAILER_*, loaded by the systemd unit (DEPLOYMENT.md)
```
Start from that folder:
```bash
LOCO_ENV=staging LEPTOS_OUTPUT_NAME=app LEPTOS_SITE_ROOT=site LEPTOS_SITE_PKG_DIR=pkg ./app start
```
Loco serves plain HTTP on 5150; TLS is the reverse proxy's job.

## Gotchas

Things that cost us time once, so they do not cost you time twice.

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
- An island only hydrates if it lives in `src/islands.rs`, the one module
  compiled for the browser. An `#[island]` in `src/views/` renders on the
  server, then the browser warns about a missing island function and
  nothing happens.
- Staging and production stop themselves once a day (`settings.nightly_restart`
  in the config, `src/maintenance.rs`) and rely on the unit's
  `Restart=always` to come back. Development and test never do, even with
  `enable: true`, so a `cargo leptos watch` left running overnight is still
  there in the morning.
