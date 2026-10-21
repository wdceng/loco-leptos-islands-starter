# Rules for AI agents in this project

Read `AGENTS.md` and the Loco skill in `.claude/skills/loco/` first: they
explain how Loco works. This file is what this project adds on top, and
where it deliberately differs.

**This file holds only rules every change obeys, one short item each, and
stays under 8 KB.** Knowledge goes in `docs/` and is read when a task
touches it. A rule a test can check becomes a test, not a line here.

## Rule no. 1: one job, one implementation

The first check of every plan and every change. A visitor sees one way of
doing one thing.

- **Before planning,** look the job up in `docs/one-job.md`, then search
  the code. Build from what already does it.
- **A plan opens with it:** the rows it checked, then the component it
  reuses, or the new job and its new row.
- **A second mechanism only with a reason** the maintainer agrees to, and
  it still looks and moves the same.
- **A one-job break** (two ways for one job) found while working: say so,
  with a plan to make them one. Never add a third.
- **A break that could come back gets a guard** in `tests/one_job.rs`,
  failing on its pattern outside its one home.

## Where things are written down

| Read | When |
|---|---|
| `docs/architecture.md` | Why the project is built the way it is |
| `docs/decisions.md` | Before changing a choice: each one, dated, with its reason |
| `docs/tried-and-dropped.md` | Before suggesting something: what failed, and why. Never suggested again without a new reason |
| `docs/one-job.md` | Before adding a component, a constant or a helper |
| `docs/security.md` | Before touching auth, sessions, headers or the CSP; it's updated in the same commit |
| `docs/development.md`, `docs/testing.md` | Commands, environments, builds, checks |
| `docs/dev-server.md`, `docs/staging.md`, `docs/production.md` | Deploys. Parallel files: a change to one section goes into all three in the same pass |

## How to change files

- **Every change must show as a diff.** Edit files with your editing tools,
  never with `sed`, `perl` or scripts in the shell. The maintainer reviews
  each change in the editor. `git mv` and `git rm` change files too: hand
  them over as commands.
- **Generators are welcome, but never in the repo itself.** Run
  `cargo loco generate …`, `sea-orm-cli` and the like in a scratch
  `git worktree` of the current branch, then bring each new file and each
  change to an existing file (`src/app.rs`, `mod.rs`, migrations) over with
  your editing tools. Say what the generator wrote and what you changed
  after. Delete the worktree. A generated mailer's Tera templates are not
  brought over (the mailers row below).
- `cargo fmt` and `cargo sqlx prepare` may rewrite files; a `cargo fmt`
  goes in a commit of its own.
- **Keep Loco's starter code.** Extend what `loco new` generated, don't
  delete or unmount it, unless the maintainer asks.
- **Ask before changing a framework default** (Loco's `development`,
  `target/site`, port 5150, `LEPTOS_*`): say what depends on it first.

## Where this project differs from Loco's guide

| Loco's guide says | Here |
|---|---|
| Views are Tera templates or JSON (`src/views/`) | Pages are **Leptos** components (`src/views/`), rendered through `render_page` (`src/render.rs`). `src/views/auth.rs` still holds the auth API's JSON shapes |
| Mailers use Tera `.t` templates (`mail_template`) | The HTML part is a Leptos component (`src/views/mail.rs`), subject and text are `format!`, sent with `Mailer::mail`. `cargo loco generate mailer` writes Tera templates, which won't compile here; write new mails like `src/mailers/auth.rs` |
| Query through Sea-ORM | Loco's own models stay on Sea-ORM. **The app's own queries use SQLx `query!`** on `sql::pool(&ctx)`: checked against the schema at compile time ("Sea-ORM and SQLx" in `docs/architecture.md`) |
| Environments `development`, `test`, `production` (`LOCO_ENV`, default `development`) | The same three, plus **`dev-server`** and **`staging`**, which Loco reads as `Environment::Any`. `config/dev-server.yaml` must stay staging's copy apart from its defaults (`tests/config.rs`): change both together. Check "is this local?" with `settings::is_local` |
| Static files from `assets/static` | Files live in `public/` and are **linked only through `paths::assets`** (versioned constants from `build.rs`). A plain `"/favicon/..."` string fails the links test |

## Islands, htmx and Alpine

- **htmx for what needs the server, Alpine's CSP build for small page
  state, an island only for what neither can.** Links and forms stay
  plain (`href`, `action`); the shell's `#page` boosts them.
- **No `hx-on`, no full Alpine:** both need `'unsafe-eval'`, which the CSP
  refuses (`tests/config.rs`). No project JavaScript beyond
  `public/js/htmx-islands.js` (`src/paths.rs` tests it).
- **Islands live in `src/islands.rs`**, the one module both halves
  compile. An island anywhere else renders but never runs.
- **`--split` on every cargo-leptos command:** watch, build, deploys, CI.
  Without it no island runs; the server refuses to boot that build.
- **Lazy (`#[island(lazy)]`) when only some pages use an island,** and
  render every island only where it's needed: rendered means fetched.
- **cargo-leptos is the patched build** (`tools/cargo-leptos/`,
  `check.sh`) until `docs/decisions.md` says its fixes are released.
- **Thin islands:** content passed in as `children`, props plain data.

## Rules the compiler or tests can't enforce

- **Check, don't recall.** A claim about a crate is checked in its source in
  `~/.cargo/registry`, at the version in `Cargo.lock`. A web source is quoted
  only after it was opened. Say plainly what is unverified or untested.
- **A decision gets a line in `docs/decisions.md`,** dated, with its
  reason. Something tried and reverted gets one in
  `docs/tried-and-dropped.md`.
- **Never stage or print the secrets files** (`secrets*.env`). They hold
  real credentials and are git-ignored; check `git status` before every
  commit.
- **Never export `DATABASE_URL`.** Every config reads it, so `cargo test`
  would wipe that database. Set it on one command only.
- **Release builds go to `target/site-release`**
  (`LEPTOS_SITE_ROOT=target/site-release` on the build and the local run):
  `target/site` is the watch loop's. No cargo-leptos build at all while a
  watch loop runs in this repo (`pgrep -fl cargo-leptos`).
- **English** for every identifier, comment, doc, config key and CSS class.
  Only text a visitor reads on a page may be another language.
- **Docs in a plain human voice:** short sentences, few words, commands with
  short `#` comments. Gotchas are one bold line plus one sentence.
- **A config change that alters policy** (headers, CSP, rate limits, cache,
  timeout) updates `tests/config.rs` with it.
- **A new public page** gets a line in `/llms.txt`
  (`src/controllers/llms.rs`).

## Role

Act as an experienced senior Rust developer delivering production code:

- no `unwrap()` or `expect()` on the request path: return
  `loco_rs::Error` / `Result`;
- no `unsafe`, no `#[allow(...)]` to silence a warning;
- the boring, idiomatic solution over a clever one;
- every change verified by the checks below, not by reasoning that it
  should work.

## Before you hand over

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
cargo test
```

After a migration or a new or changed `query!`, refresh the SQLx cache
(`docs/development.md`) and commit `.sqlx/` with the change.

Commit messages are plain sentences about the change, with no tool or AI
named in them. The maintainer commits with
`git status --short && git add . && git commit -m "..." && git push`.

## Starting a new project from this template

1. Rename the app ("Make it yours" in `README.md`).
2. Replace this file's first line with what the project is, in one
   sentence.
3. Empty `docs/decisions.md` and `docs/tried-and-dropped.md` down to their
   headings, keeping the entries the new project still lives by.
4. Keep the rules. Grow `docs/one-job.md` from the first component.
