# Rules for AI agents in this project

Read `AGENTS.md` and the Loco skill in `.claude/skills/loco/` first: they
explain how Loco works. This file is what this project adds on top, and
where it deliberately differs. The reasons are in `docs/architecture.md`.

## How to change files

- **Every change must show as a diff.** Edit files with your editing tools,
  never with `sed`, `perl` or scripts in the shell. The maintainer reviews
  each change in the editor.
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
  `cargo clippy`, `cargo leptos build`, `cargo sqlx prepare`.
- **Keep Loco's starter code.** Extend what `loco new` generated, don't
  delete or unmount it, unless the maintainer asks.

## Where this project differs from Loco's guide

| Loco's guide says | Here |
|---|---|
| Views are Tera templates or JSON (`src/views/`) | Pages are **Leptos** components (`src/views/`), rendered through `render_page` (`src/render.rs`). `src/views/auth.rs` still holds the auth API's JSON shapes |
| Mailers use Tera `.t` templates (`mail_template`) | The HTML part is a Leptos component (`src/views/mail.rs`), subject and text are `format!`, sent with `Mailer::mail`. `cargo loco generate mailer` writes Tera templates and won't compile here (no `include_dir` crate); write new mails like `src/mailers/auth.rs` |
| Query through Sea-ORM | Loco's own models stay on Sea-ORM. **The app's own queries use SQLx `query!`** on `sql::pool(&ctx)`: checked against the schema at compile time ("Sea-ORM and SQLx" in `docs/architecture.md`) |
| Environments `development`, `test`, `production` (`LOCO_ENV`, default `development`) | The same three, plus **`dev-server`** and **`staging`**, which Loco reads as `Environment::Any`. `config/dev-server.yaml` must stay staging's copy apart from its defaults (`tests/config.rs`): change both together. Check "is this local?" with `settings::is_local` |
| Static files from `assets/static` | Files live in `public/` and are **linked only through `paths::assets`** (versioned constants from `build.rs`). A plain `"/favicon/..."` string fails the links test |

## Rules the compiler or tests can't enforce

- **Never stage or print the secrets files** (`secrets.env`,
  `secrets.dev-server.env`, `secrets.staging.env`,
  `secrets.production.env`). They hold real
  credentials and are git-ignored; check `git status` before every commit.
- **Never export `DATABASE_URL`.** Every config reads it, so `cargo test`
  would wipe that database. Set it on one command only.
- **English** for every identifier, comment, doc, config key and CSS class.
  Only text a visitor reads on a page may be another language.
- **Docs in a plain human voice:** short sentences, few words, commands with
  short `#` comments. Gotchas are one bold line plus one sentence.
- **A config change that alters policy** (headers, CSP, rate limits, cache,
  timeout) updates `tests/config.rs` with it.
- **A new public page** gets a line in `/llms.txt`
  (`src/controllers/llms.rs`).

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
