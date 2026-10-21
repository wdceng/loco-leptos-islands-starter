# Decisions

One line per choice, dated, with its reason. Read before changing one; a
change of mind gets a new line, and the old one moves to
`tried-and-dropped.md` if it was tried.

## Stack

- 2026-09-28: **Loco's Rest API starter as the base**, not "SaaS App with
  server side rendering". Leptos makes the pages, so the SaaS starter's
  Tera views, translations and `/static` would only be removed.
- 2026-09-28: **Loco 1.2, Sea-ORM 2.0.4, Rust 1.94 or newer.**
- 2026-10-02: **SQLx `query!` for the app's own queries**, on Sea-ORM's
  pool, checked at compile time. Loco's own models stay on Sea-ORM.
- 2026-10-02: **Mails, error answers and the 429 page are Leptos**, not
  Tera: compile-checked and escaped. Tera stays only in Loco's YAML configs.
- 2026-10-04: **Tailwind's default look.** The template is public, so it
  carries no house style: no brand colour, no self-hosted font, no shape
  roles. An app adds its own.
- 2026-10-17: **Tailwind scans `src/` only** (`source("../src")` on the
  import). Its default scanned the docs and configs too, and their prose
  made about a quarter of the CSS.

## Environments and builds

- 2026-10-06: **Five environments**: development, test, dev-server,
  staging, production. The dev-server is work in progress on a server,
  staging the last look before production. `config/dev-server.yaml` stays
  staging's copy, pinned by `tests/config.rs`.
- 2026-10-07: **The local environment is Loco's `development`**, its
  default when `LOCO_ENV` is unset. A rename to `dev-local` needed an
  export in every shell for a name alone.
- 2026-10-08: **Release builds go to `target/site-release`**
  (`LEPTOS_SITE_ROOT`), so a release or deploy build never replaces the
  watch loop's `target/site`.
- 2026-10-08: **Deploys start with a check that their secrets file
  exists**, before anything is built: a missing file would fail the upload
  with the service already stopped.
- 2026-10-21: **The cross image is pinned by digest**, the 2026-10-09
  `main` build (`Cross.toml`), so the toolchain that builds the server
  binary cannot change under a moving tag. ecs and wdc use the same one.

## Islands

- 2026-10-15: **Lazy islands with `--split` on every cargo-leptos
  command.** Apps built on the template are island-heavy; a lazy island's
  wasm is fetched only by the pages that render it. `IslandsCheck` on the
  home page keeps the split path in use, so CI builds and checks it. The
  server refuses a build without `--split` (`src/assets.rs`).
- 2026-10-15: **cargo-leptos is a patched build** (`tools/cargo-leptos/`):
  0.3.11 at commit `0fb3de86` (PR #691, issue #690: stale names after a
  hashed split build) against a sorted `wasm_split_cli_support` 0.2.3
  (wasm-split-prototype #79: new names on every build). Both fixes were
  merged upstream on 2026-10-16 (#79 as `cc582f1`, the same code as our
  patch); neither is released yet. **Released yet?** `tools/upstream.sh`.
  **Way back:** once it prints the cargo-leptos test and that passes,
  `cargo install --locked cargo-leptos@<version>` in the README,
  `prerequisites.md` and CI, delete `tools/cargo-leptos/` and the
  `check.sh` gates in the deploy chains, and add a line here.
- 2026-10-18: **Moves in place with htmx 4 and Alpine's CSP build**, as
  the apps built on the template already did on their own: one tested
  version here instead of one per app. htmx for what needs the server,
  Alpine for small page state, an island for what neither can. Vendored
  byte for byte from npm (`htmx.org` 4.0.0, `@alpinejs/csp` 3.17.4), each
  checked against the package before it came in.
- 2026-10-18: **The `parseHTMLUnsafe` workaround** in
  `public/js/htmx-islands.js` (htmx #4147: Chrome drops the comments
  Leptos hydrates by).
- 2026-10-19: **It passes options instead of deleting the function**, the
  htmx maintainer's way: Chrome 150 to 156 drops the comments only when
  called with one argument. #4147 closed without a change in htmx.
  **Way back:** once Chrome 157 is everywhere, drop it.
- 2026-10-19: **Leptos from a fork, pinned by commit**
  (`[patch.crates-io]` in `Cargo.toml`): 0.8.22 as published plus one
  line, the islands loader's `requestIdleCallback` given a 2 s timeout, as
  its own hydration script has. Chrome 114 to 123 can stop giving idle
  time after many whole loads in one tab, found in an islands app: then no
  island starts. The same fix as that app's, in the loader itself, not a
  wrapper of every idle callback on the page (`tried-and-dropped.md`).
  `the_islands_loader_waits_for_idle_two_seconds_at_most` checks the
  loader. **Way back:** once a Leptos release's loader passes a timeout
  (leptos #4959, `tools/upstream.sh`), drop the block; a newer Leptos
  makes Cargo warn the patch went unused.
- 2026-10-20: **A form drops a second press and looks busy while sent.**
  htmx 4's default without `hx-sync` queues one more request: two presses
  on the reset form sent two posts (a browser test proved it), the second
  with a token the first used up. `hx-sync="this:drop"` on the form, not
  the region: a second link press should win, not be dropped. The look,
  from an islands app: the `busy:` variant on htmx's request class, after
  200 ms, `aria-busy` through Alpine; no `disabled`, which drops the
  button's name and value from the post.
- 2026-10-19: **No htmx settle** (`"defaultSettleDelay":0` in
  `HTMX_CONFIG`). Its default gives each new element whose `id` matches an
  old one the old one's attributes for a frame, which only serves CSS
  transitions from old to new, and `transitions` is off. An islands app's
  menu, same `id` on every page, arrived open and faded shut after each
  move. The template has no such element yet; the first app's would.
- 2026-10-18: **The `Navigation` island** ports four behaviours from the
  sister app's `Chores`: focus and announcement, Back's scroll, a newer
  build loading plainly, a gateway's error loading plainly. Its
  announcement is read a tick after the swap: htmx 4 sets the title after
  `htmx:after:swap` (found in Chrome on 2026-10-18; the sister app read the
  old title).
- 2026-10-18: **The island collector** (`src/collect.rs`, from the sister
  app): every island's body through `collected`, so an island htmx swaps
  out is freed instead of kept until the next full load. **Way back:**
  once leptos PR #4953 is released (`tools/upstream.sh`), its `$$dispose`
  on each island element can be called from `public/js/htmx-islands.js`
  before a swap, and `collect.rs` with every `collected(…)` can go; the
  memory tests prove the swap.
- 2026-10-18: **Lazy islands are plain in a test build**
  (`cfg_attr(test, island)`): the collector's browser tests run a plain
  wasm, which can't load a lazy island's split import.
- 2026-10-18: **Browser tests in a crate of their own** (`e2e/`,
  thirtyfour, Chrome only), the sister app's harness cut down to one lane:
  the moves in place and the memory tests against a real `--split` build,
  run locally (`ci.yaml` has a job for them, for projects that use
  Actions; this repo doesn't). Their own environment `e2e` (port 5160, `target/e2e`), so a
  run never touches the watch loop. DevTools' own socket (`cdp-events`)
  hears the page's errors and changes an answer in flight; chromedriver
  passes no events on.
- 2026-10-19: **A low-bar lane in Chrome 114** (`e2e/run.sh low`), the
  oldest Chrome the template supports, about 30 months back: the same
  tests, local only. Firefox 128, iOS 17.5 and Android 15 are left to the
  apps (`docs/testing.md`, "The Low Bar"): the harness rests on Chrome's
  DevTools.
- 2026-10-18: **The memory tests turn DevTools' log off** while they
  measure: it keeps every element an entry names, and Chrome's notice
  about the reset form kept one old `<main>` per Back. Without DevTools,
  Chrome kept none (a probe).

## Security

- 2026-10-18: **A security review of the starter auth**, its 13 findings
  and their status in `docs/security.md`. That page is updated with every
  change to auth, sessions, headers or the CSP.
- 2026-10-18: **JWTs live one day.** They can't be revoked; logout and
  longer sessions come with cookie sessions, an app's step.
- 2026-10-18: **Passwords: length only, 8 to 128 characters,** as NIST SP
  800-63B advises. No composition rules; a breached-password check is an
  app's choice.
- 2026-10-18: **The reset link carries its token in the query**
  (`/reset?token=…`), not Loco's `#token`: the page is server-rendered with
  no JavaScript. Its answers send `no-referrer` and `no-store`.
- 2026-10-18: **The security contact lives in the config**, not the
  environment: it isn't a secret. Production refuses to boot while it's
  still `example.com`.

## Working

- 2026-10-03: **Generators run in a scratch `git worktree`**, their files
  brought over with editing tools, so every change shows as a diff.
- 2026-10-05: **The maintainer commits:**
  `git status --short && git add . && git commit -m "..." && git push`.
  Commit messages name no tool or AI.
- 2026-10-15: **CLAUDE.md holds rules only, under 8 KB.** Knowledge lives
  in `docs/`, read when a task touches it; a rule a test can check becomes
  a test.
