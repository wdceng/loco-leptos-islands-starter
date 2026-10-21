# Development

Day-to-day commands and a few traps. First time here? Start with "Run it in
five minutes" in `README.md`.

## Run

```bash
cargo leptos watch --split -- start                  # this Mac only
BINDING=0.0.0.0 cargo leptos watch --split -- start  # every device on the network
```

Open http://localhost:5150. Save a file and the page updates by itself, like
hot reload. Strictly it's live reload: a Rust change reloads the whole page,
a change to `style/tailwind.css` swaps in without one. Real hot reload needs
nightly Rust, so we don't use it.

Keep the `-- start`. Without it the app only prints its help.

Keep `--split` on every cargo-leptos command. Without it no island runs, and
the server refuses to start ("Lazy islands" below).

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
`PORT=5151 cargo leptos watch --split -- start`.

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
machine and in the server build. The one step that touches the database is
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
LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-release cargo leptos build --release --split   # build
LEPTOS_SITE_ROOT=target/site-release ./target/release/app start                                   # run it locally
```

Always keep `LEPTOS_HASH_FILES=true`. It puts a hash in the file names
(`app.<hash>.css`) and writes `hash.txt` next to the binary, so production
can cache files for a year. Without it, visitors get stale files.

Always keep `LEPTOS_SITE_ROOT=target/site-release` too. The watch loop owns
`target/site`; a release build there would replace its files with hashed
ones. Kept apart, both can run at once: give the release one
`PORT=5151`.

You get `target/release/app`, `target/release/hash.txt` and
`target/site-release/`.

Size, for reference: with no island the wasm is about 28 KB gzipped. The
first island brings in the Leptos runtime, about 52 KB gzipped. More islands
cost much less, and a lazy one only on the pages that render it.

### For a Linux server

The server part is built natively with `cargo zigbuild`: Zig links it for
Linux, no container.

```bash
LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-release cargo leptos build --release --split --frontend-only   # site files and hash.txt
CARGO_TARGET_DIR=target/zig cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.31                       # target/zig/x86_64-unknown-linux-gnu/release/app
```

- `.2.31` is the oldest glibc the binary needs (Step 0 in `production.md`).
- `target/zig` keeps this build apart from the Mac's own, so neither
  rebuilds the other.
- For the dev server, `--profile dev-server` instead of `--release`: it
  builds faster. Staging and production build `--release`.
- Full steps in `dev-server.md`, `staging.md` and `production.md`.

**Keep `strip = true` on every profile you deploy.** Zig's linker doesn't
merge debug strings: line tables made a sister app's binary 2 GB.

**`ignoring deprecated linker optimization setting "1"` is harmless.**
rustc passes `-O1`, Zig's linker ignores it.

## Lazy islands

`#[island(lazy)]` moves an island out of the main wasm into a chunk of its
own, fetched only by a page that renders it. `IslandsCheck` on the home page
is one (`src/islands.rs`).

```bash
tools/cargo-leptos/check.sh   # does this machine's cargo-leptos have both fixes?
tools/cargo-leptos/install.sh # if not: build and install it (~3 min)
```

- **`--split` on every cargo-leptos command:** the watch loop, local
  builds, deploys, CI. Without it the main script imports a module that
  doesn't exist, so no island runs on any page. The server checks at boot
  and refuses that build (`src/assets.rs`).
- **Lazy when only some pages use it.** An island on every page saves
  nothing as lazy. Render an island only where it's needed: rendered means
  fetched.
- **`hash.txt` gains `split:` and `manifest:` lines.** It travels with the
  binary as before.

**Why the patched cargo-leptos.** Released cargo-leptos (0.3.11) has two
bugs with `--split` and hashed names. The watch loop doesn't hash, so they
show only in release builds:

| Bug | Effect | Fix |
|---|---|---|
| [cargo-leptos #690](https://github.com/leptos-rs/cargo-leptos/issues/690) | The main `.js`/`.wasm` keep their old names after the loader's name is written into them. After a deploy, a cached main script imports a loader that's gone: no island runs until a hard refresh | [PR #691](https://github.com/leptos-rs/cargo-leptos/pull/691), commit `0fb3de86`. Merged 2026-10-16, not released |
| [wasm-split-prototype #79](https://github.com/WorldSEnder/wasm-split-prototype/issues/79) | The loader's imports come out in random order, so every build gets new names and visitors download it all again | `tools/cargo-leptos/wasm-split-sort.patch`, the same code as upstream's `cc582f1`. Merged 2026-10-16, not released, and released cargo-leptos still locks 0.2.3 |

`install.sh` builds cargo-leptos at that commit, against the patched crate.
CI uses the same script, and checks that two hashed builds give the same
names and that the main script names a loader that exists.

**Released yet?**

```bash
tools/upstream.sh   # every upstream fix the template waits for, and whether it's released
```

It watches four: the two above, leptos #4953 (islands disposed when they
leave the page) and htmx #4147 (Chrome dropping the comments Leptos
hydrates by). Fix 2 counts only once a cargo-leptos **release's own
`Cargo.lock`** locks a newer `wasm_split_cli_support`: `cargo install
--locked` takes that lock, not crates.io's latest. When both split fixes
are in a release it prints a two-build check that installs the plain
release into a scratch folder, so your working cargo-leptos stays.
**If that passes:** `cargo install --locked cargo-leptos@<version>`, the
same line in CI, and delete `tools/cargo-leptos/` ("Way back" in
`docs/decisions.md`).

## Environments

`LOCO_ENV` picks the file in `config/`. There's no `.env` file, secrets come
from environment variables.

| Environment | For | Files from | Browser cache | Host |
|---|---|---|---|---|
| `development` | your machine (default) | `target/site` | rechecks every time | `http://localhost:5150` |
| `test` | `cargo test` | none | none | `http://localhost:5150` |
| `e2e` | the browser tests (`e2e/run.sh`) | `target/e2e/site` | rechecks every time | `http://localhost:5160` |
| `dev-server` | development copy online, staging's twin | `site/` | 60 s | `server.host` in the config |
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

Deploying, the server layout and the systemd unit: `dev-server.md`,
`staging.md` and `production.md`. Each one is a single file you follow top
to bottom.

## Gotchas

- **Won't start after a release build?** It was built without
  `LEPTOS_SITE_ROOT=target/site-release`, so `target/site` has hashed names
  the debug build can't find. Run `cargo leptos build --split` or the watch
  loop.
- **Keep `LEPTOS_OUTPUT_NAME` in `.cargo/config.toml`.** Leptos needs it to
  name the wasm file the same way cargo-leptos does.
- **Some crates recompile** when you switch between `cargo leptos` and plain
  `cargo`. It only costs a few seconds.
- **`target/site` and `target/site-release` are wiped on every build.** Put
  static files in `public/`.
- **Two `Error` types.** Loco and Leptos both export one. In files that use
  both, import Leptos items by name, not `leptos::prelude::*`.
- **`as` and `type` are keywords.** In `view!`, write `r#as` and `r#type`.
- **Islands go in `src/islands.rs`.** One in `src/views/` renders, but never
  runs in the browser.
- **Keep `overflow-anchor: none` on `<html>`** (`style/tailwind.css`). With
  a toast or a loading bar after the page's region, Chrome and Firefox
  anchor on it and a move flashes the new page's footer.
- **With `scroll-behavior: smooth` a move's jump to the top glides.**
  Scroll it from the `Navigation` island, instantly, not by `show:top`.
- **`class={CONST} {..spread}` in `view!`, braces on the constant.**
  `class=CONST {..spread}` parses as a struct literal.
- **Leptos writes `&` as `&amp;` in an attribute.** A test asserts
  `[&amp;_option]`, not `[&_option]`.
- **A substring test hits class names.** `contains("checked")` passes on
  `checked:bg-…`; test the attribute (` checked` then a space, `>` or `=`).
- **Alpine's `.outside` does nothing on a `display: contents` element.**
  It skips an element with no size, and islands are `contents`: listen on
  the window and test `$el.contains($event.target)` yourself.
- **A popover counts as outside its opener.** It lives in the top layer:
  leave a tap inside `[popover]` out of a close-on-outside.
- **A morph keeps Alpine's state; a changed `x-data` resets it.** State the
  server writes into `x-data` is thrown away on every swap.
- **Safari's autofill grabs a field named `name` or holding "address".**
  Name it for what it is.
- **A value a script sets never counts as `tooShort`.** A test that types by
  script checks no `minlength`.
- **An empty live region is truly empty,** no space, or some readers
  announce it.
- **`leptos` comes from a fork, pinned by commit** (`[patch.crates-io]` in
  `Cargo.toml`): 0.8.22 with the island loader's idle timeout. On a Leptos
  upgrade, drop the block if the release has the timeout, else move the
  fork to the new version; `the_islands_loader_waits_for_idle_two_seconds_at_most`
  checks the loader either way.
- **Staging and production restart every night.** Development never does,
  so a watch loop left on overnight is still running in the morning.
- **The home-screen app reaches the glass.** Opened from a phone's home
  screen, the page has no Safari bars around it: give controls at the top
  and the foot room with `standalone:max-md:…` (`style/tailwind.css`).
- **No safe-area insets without `viewport-fit=cover`.**
  `env(safe-area-inset-*)` reads 0 here; use the `standalone:` variant
  instead.
- **No test sees the home-screen app.** WebDriver can't open one, and
  DevTools doesn't truly emulate `display-mode` (`matchMedia` stays false).
  Check it by hand on a phone.
- **Swipe the home-screen app away after a deploy, then reopen it.** A
  backgrounded app keeps its old CSS.
