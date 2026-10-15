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

## Islands

- 2026-10-15: **Lazy islands with `--split` on every cargo-leptos
  command.** Apps built on the template are island-heavy; a lazy island's
  wasm is fetched only by the pages that render it. `IslandsCheck` on the
  home page keeps the split path in use, so CI builds and checks it. The
  server refuses a build without `--split` (`src/assets.rs`).
- 2026-10-15: **cargo-leptos is a patched build** (`tools/cargo-leptos/`):
  0.3.11 at commit `0fb3de86` (PR #691, issue #690: stale names after a
  hashed split build) against a sorted `wasm_split_cli_support` 0.2.3
  (wasm-split-prototype #79: new names on every build). **Released yet?**
  `tools/cargo-leptos/upstream.sh`. **Way back:** once it says "maybe" and
  its test passes, `cargo install --locked cargo-leptos@<version>` in the
  README, `prerequisites.md` and CI, delete `tools/cargo-leptos/` and the
  `check.sh` gates in the deploy chains, and add a line here.

## Working

- 2026-10-03: **Generators run in a scratch `git worktree`**, their files
  brought over with editing tools, so every change shows as a diff.
- 2026-10-05: **The maintainer commits:**
  `git status --short && git add . && git commit -m "..." && git push`.
  Commit messages name no tool or AI.
- 2026-10-15: **CLAUDE.md holds rules only, under 8 KB.** Knowledge lives
  in `docs/`, read when a task touches it; a rule a test can check becomes
  a test.
