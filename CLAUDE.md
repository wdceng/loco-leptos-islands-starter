# Rules for AI agents in this project

Read `AGENTS.md` and the Loco skill in `.claude/skills/loco/` first: they
explain how Loco works. This file is what this project adds on top, and
where it deliberately differs.

**This file holds only rules every change obeys, one short item each, and
stays under 8 KB.** Knowledge goes in `docs/` and is read when a task
touches it. A rule a test can check becomes a test, not a line here.

## Where things are written down

| Read | When |
|---|---|
| `docs/architecture.md` | Why the project is built the way it is |
| `docs/decisions.md` | Before changing a choice: each one, dated, with its reason |
| `docs/tried-and-dropped.md` | Before suggesting something: what failed, and why. Never suggested again without a new reason |
| `docs/one-job.md` | Before adding a component, a constant or a helper |
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
  after. Delete the worktree. When the generator makes a mailer, its Tera
  templates (`.t` files) and the `include_dir` dependency are not brought
  over: the HTML part becomes a component in `src/views/mail.rs`, and the
  mail is sent like `AuthMailer` (`src/mailers/auth.rs`).
- Building, testing and checking run in the repo as normal: `cargo test`,
  `cargo clippy`, `cargo leptos build --split`, `cargo sqlx prepare`.
  `cargo fmt` may rewrite files, in a commit of its own.
- **Keep Loco's starter code.** Extend what `loco new` generated, don't
  delete or unmount it, unless the maintainer asks.
- **Ask before changing a framework default** (Loco's `development`,
  `target/site`, port 5150, `LEPTOS_*`): say what depends on it first.

## Where this project differs from Loco's guide

| Loco's guide says | Here |
|---|---|
| Views are Tera templates or JSON (`src/views/`) | Pages are **Leptos** components (`src/views/`), rendered through `render_page` (`src/render.rs`). `src/views/auth.rs` still holds the auth API's JSON shapes |
| Mailers use Tera `.t` templates (`mail_template`) | The HTML part is a Leptos component (`src/views/mail.rs`), subject and text are `format!`, sent with `Mailer::mail`. `cargo loco generate mailer` writes Tera templates and won't compile here (no `include_dir` crate); write new mails like `src/mailers/auth.rs` |
| Query through Sea-ORM | Loco's own models stay on Sea-ORM. **The app's own queries use SQLx `query!`** on `sql::pool(&ctx)`: checked against the schema at compile time ("Sea-ORM and SQLx" in `docs/architecture.md`) |
| Environments `development`, `test`, `production` (`LOCO_ENV`, default `development`) | The same three, plus **`dev-server`** and **`staging`**, which Loco reads as `Environment::Any`. `config/dev-server.yaml` must stay staging's copy apart from its defaults (`tests/config.rs`): change both together. Check "is this local?" with `settings::is_local` |
| Static files from `assets/static` | Files live in `public/` and are **linked only through `paths::assets`** (versioned constants from `build.rs`). A plain `"/favicon/..."` string fails the links test |

## Islands

- **Islands live in `src/islands.rs`**, the one module both halves
  compile. An island anywhere else renders but never runs.
- **`--split` on every cargo-leptos command:** watch, build, deploys, CI.
  Without it no island runs; the server refuses to boot that build.
- **Lazy (`#[island(lazy)]`) when only some pages use an island,** and
  render every island only where it's needed: rendered means fetched.
- **cargo-leptos is the patched build** from `tools/cargo-leptos/`
  (`check.sh` says whether it's installed). Never a plain
  `cargo install cargo-leptos` until `docs/decisions.md` says the fixes
  are released.
- **Thin islands:** content passed in as `children`, props plain data.

## Rules the compiler or tests can't enforce

- **One job, one implementation.** Something that does the job of an
  existing component or constant is built from it, never from classes of
  its own. Look it up in `docs/one-job.md`; a new job gets a row there when
  it's built.
- **Check, don't recall.** A claim about a crate is checked in its source in
  `~/.cargo/registry`, at the version in `Cargo.lock`. A web source is quoted
  only after it was opened. Say plainly what is unverified or untested.
- **A decision gets a line in `docs/decisions.md`,** dated, with its
  reason. Something tried and reverted gets one in
  `docs/tried-and-dropped.md`.
- **Never stage or print the secrets files** (`secrets.env`,
  `secrets.dev-server.env`, `secrets.staging.env`,
  `secrets.production.env`). They hold real
  credentials and are git-ignored; check `git status` before every commit.
- **Never export `DATABASE_URL`.** Every config reads it, so `cargo test`
  would wipe that database. Set it on one command only.
- **Release builds go to `target/site-release`.** Put
  `LEPTOS_SITE_ROOT=target/site-release` on every
  `cargo leptos build --release` and on the release binary run locally.
  `target/site` belongs to the maintainer's watch loop: a build there
  replaces its files. No cargo-leptos build at all while a watch loop runs
  in this repo (`pgrep -fl cargo-leptos`).
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
