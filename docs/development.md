# Development

Day-to-day commands and a few traps. First time here? Start with "Run it in
five minutes" in `README.md`.

## Run

```bash
cargo leptos watch -- start                  # this Mac only
BINDING=0.0.0.0 cargo leptos watch -- start  # every device on the network
```

Open http://localhost:5150. Save a file and the page updates by itself, like
hot reload. Strictly it's live reload: a Rust change reloads the whole page,
a change to `style/tailwind.css` swaps in without one. Real hot reload needs
nightly Rust, so we don't use it.

Keep the `-- start`. Without it the app only prints its help.

**On your phone:** use the second line, get your Mac's address with
`ipconfig getifaddr en0` and open `http://<address>:5150`. Allow `app` if
the firewall asks. The phone doesn't reload by itself, so refresh by hand.
Only do this on a network you trust.

**Server only**, without rebuilding the wasm or the CSS:

```bash
cargo loco start    # run once
cargo loco watch    # restart on every change (needs cargo-watch)
```

Run one loop at a time, they share port 5150. Another port:
`PORT=5151 cargo leptos watch -- start`.

```bash
ps aux | grep 'app start'   # find the running app
top -pid <PID>              # watch it
```

## Inspect

```bash
cargo loco routes                                  # every URL
cargo loco middleware                              # which middleware is on
cargo loco middleware -c                           # the same, with settings
cargo loco doctor                                  # check config and environment
LOCO_ENV=staging cargo loco middleware             # another environment
LOCO_CONFIG_FOLDER=/path/to/copy cargo loco start  # try an edited copy of config/
```

## Lint and Format

CI runs these, so run them before you push:

```bash
cargo fmt
cargo clippy --all-targets
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

The last one checks the browser half. It's the only one that sees island
code.

## Database and Tasks

```bash
cargo loco db status          # which migrations ran
cargo loco db migrate         # run new ones
cargo loco db reset           # wipe and rebuild (development only)
cargo loco db entities        # regenerate src/models/_entities/ (needs sea-orm-cli)
cargo loco task user_create   # run a task; `cargo loco task` lists them
```

Migrations also run on every start, so you rarely need `migrate`.

### After a migration, or a new or changed query

The app's own queries are SQLx `query!` macros, checked at compile time
against the schema (`architecture.md`, "Sea-ORM and SQLx"). Every build
reads that schema from the committed `.sqlx/` cache, never a database:
`SQLX_OFFLINE = "true"` in `.cargo/config.toml` makes sure of it, on every
machine and in `cross`. The one step that touches the database is
`cargo sqlx prepare`, so after a schema or query change, refresh the cache:

```bash
cargo loco db migrate                                                          # the schema first
DATABASE_URL=sqlite://app_development.sqlite cargo sqlx prepare -- --all-targets   # then the cache
```

Commit `.sqlx/` with the change. CI fails if you forget.

- **Never `export DATABASE_URL`.** Every config reads it, so `cargo test`
  would run against that database and wipe it. Set it on the one command.
- **Always `-- --all-targets`.** Without it the tests' queries drop out of
  the cache.
- **Checking against the live database** while you write a query: put
  `SQLX_OFFLINE=false DATABASE_URL=sqlite://app_development.sqlite` in front
  of `cargo check --all-targets`. A wrong column then fails the build with
  `no such column`.

## Tests

```bash
cargo test                     # all
cargo test home_renders_html   # one
```

No server or browser needed. More in `testing.md`.

## Release Builds

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release
```

Always keep `LEPTOS_HASH_FILES=true`. It puts a hash in the file names
(`app.<hash>.css`) and writes `hash.txt` next to the binary, so production
can cache files for a year. Without it, visitors get stale files.

You get `target/release/app`, `target/release/hash.txt` and `target/site/`.

Size, for reference: with no island the wasm is about 28 KB gzipped. The
first island brings in the Leptos runtime, about 52 KB gzipped. More islands
cost much less.

### For a Linux server

Your Mac can't build a Linux binary directly, so the server part is built
with `cross`, inside a Linux container:

```bash
LEPTOS_HASH_FILES=true cargo leptos build --release --frontend-only   # site files and hash.txt
cross build --release --target x86_64-unknown-linux-gnu               # target/x86_64-unknown-linux-gnu/release/app
```

For staging, use `--profile staging` instead of `--release` on the second
line: it builds faster and keeps readable backtraces. The first `cross`
build on Apple Silicon takes a few minutes. Full steps in `staging.md` and `production.md`.

## Environments

`LOCO_ENV` picks the file in `config/`. There's no `.env` file, secrets come
from environment variables.

| Environment | For | Files from | Browser cache | Host |
|---|---|---|---|---|
| `development` | your machine (default) | `target/site` | rechecks every time | `http://localhost:5150` |
| `test` | `cargo test` | none | none | `http://localhost:5150` |
| `staging` | test copy online | `site/` | 60 s | `server.host` in the config |
| `production` | live site | `site/` | one year | `server.host` in the config |

The host is where links in e-mails point. It's used exactly as written, no
port added, so if you run locally on another port, change it in
`config/development.yaml` too.

With a one-year cache, a changed file needs a new URL:

- **CSS, JS, wasm:** release builds put a hash in the name.
- **Everything in `public/`:** `build.rs` hashes each file and generates a
  constant for it in `paths::assets`, in modules that follow the folders
  (`paths::assets::favicon::APPLE_TOUCH_ICON_PNG`). Drop a file anywhere in
  `public/`, link it through its constant, done. Replacing it changes only
  its own `?v=`. A page that links a `public/` file as a plain string fails
  the links test.
- **A file named in `style/tailwind.css`** (a self-hosted font, say)
  carries the `?v=` written out. Replace it and `cargo test` fails with the
  exact value to paste.

### On the server

Deploying, the server layout and the systemd unit: `staging.md` and
`production.md`. Each one is a single file you follow top to bottom.

## Gotchas

- **Won't start after a release build?** `target/site` now has hashed names
  the debug build can't find. Run `cargo leptos build` or the watch loop.
- **Keep `LEPTOS_OUTPUT_NAME` in `.cargo/config.toml`.** Leptos needs it to
  name the wasm file the same way cargo-leptos does.
- **Some crates recompile** when you switch between `cargo leptos` and plain
  `cargo`. It only costs a few seconds.
- **`target/site` is wiped on every build.** Put static files in `public/`.
- **Two `Error` types.** Loco and Leptos both export one. In files that use
  both, import Leptos items by name, not `leptos::prelude::*`.
- **`as` and `type` are keywords.** In `view!`, write `r#as` and `r#type`.
- **Islands go in `src/islands.rs`.** One in `src/views/` renders, but never
  runs in the browser.
- **Staging and production restart every night.** Development never does,
  so a watch loop left on overnight is still running in the morning.
