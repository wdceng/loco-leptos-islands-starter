# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A GitHub template for full-stack Rust web apps: Loco's SaaS starter (users, JWT auth under `/api/auth`, mailers, SQLite via Sea-ORM) fused with Leptos in **islands mode** for server-rendered pages, plus a security baseline (headers, nonce CSP, rate limiting, hashed assets) pinned by tests. No hand-written JavaScript. The only page is `/`; no island exists yet. Stack decisions and the crate split (what Loco already provides vs. what this template adds) are documented in README.md; do not duplicate them here, keep README.md current when they change.

Stack: Loco (`SaaS` starter) on the server, Leptos in islands mode for views, Tailwind v4 built by cargo-leptos, SQLite.

## Docs map

- `README.md`: stack, security table, crate split, what to rename after creating a project. Keep current when a decision changes.
- `README.DEV.md`: commands, environments and cache values, deploy layout, gotchas.
- `README.TESTING.md`: what each test covers, manual checks (rate limit, timeout, headers, hashed names).
- `README.DEPLOY.md`: bare-binary deploy with placeholders: systemd unit, Caddy, Cloudflare as the reference proxy.
- `README.DPY-falkenstein-1.md`, `README.DPY.STG.md`: the same deploy on the falkenstein-1 server (shared box, units, Caddy blocks, the staging pipeline). `README.Docker.md`, `Caddyfile`: the container path.
- `PREREQUISITES.md`: one-time tool setup.
- `AGENTS.md`: Loco's generic agent guide (generators, `AppContext`); this file takes precedence where they differ.

## Toolchain and commands

One-time machine setup is in `PREREQUISITES.md` (`loco`, `cargo-leptos`, the wasm32 target, `cargo-watch`, `cross`, mailpit).

- `cargo leptos watch -- start` – the dev loop: server build (`ssr`), browser WASM build (`hydrate`), Tailwind, `target/site`, live reload. The `-- start` passes Loco's `start` subcommand to the binary; without it the Loco CLI prints help and exits.
- `cargo loco start` – server only, on port 5150, against whatever `target/site` holds. `cargo loco watch` (needs `cargo-watch`) rebuilds only the server; neither produces `target/site`. Both dev loops use port 5150; never run two at once, and use `PORT=5151` for ad-hoc checks.
- `LEPTOS_HASH_FILES=true cargo leptos build --release` – release build with hashed asset names and `target/release/hash.txt`. The prefix is deliberate: `hash-files` in `Cargo.toml` would also hash the watch loop, which only hashes on its first build. After a hashed build, `target/site` is hashed and `cargo loco start` refuses to boot until `cargo leptos build` restores plain names.
- `cross build --release --target x86_64-unknown-linux-gnu` – Linux binary for the server; the full pipeline is in `README.DEPLOY.md`.
- `cargo loco routes` / `cargo loco middleware -c` / `cargo loco doctor` – inspect routes, the resolved middleware stack per `LOCO_ENV`, config.
- `cargo test` – all tests; `cargo test <name>` – one. Also `cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate` to prove nothing leaked out of the `ssr` gate.
- `LOCO_ENV` selects `config/<env>.yaml` (development, test, staging, production); no `.env` file is loaded. `LOCO_CONFIG_FOLDER` points at another config folder, useful for negative boot checks with edited copies.

## Architecture

**One crate, two builds.** The same crate compiles natively with the `ssr` feature (Loco server) and to wasm32 with the `hydrate` feature (browser). Loco, tokio, axum, `leptos_axum`, sea-orm, `tower_governor` and every other server-only dependency must be `optional = true` and pulled in only by the `ssr` feature; server-only modules (`app.rs`, controllers, models, mailers) must be `#[cfg(feature = "ssr")]`. Anything that fails to compile for wasm32 is a sign it leaked out of the `ssr` gate. The crate, binary and asset bundle are named `app`; `README.md` lists every place the name lives.

**Request flow.** Loco owns all URL routes: one controller per page, plus the auth controller answering JSON under `/api/auth`. A page controller renders a Leptos page component to HTML on the server and returns it. The page shell includes `<HydrationScripts options islands=true/>`; the browser then loads the small WASM bundle and `leptos::mount::hydrate_islands()` hydrates only components marked `#[island]`. There is no client-side router; navigation is a full request.

**Where Leptos meets Loco.** `src/app.rs` and `src/render.rs`. In `app.rs`, `after_context` starts Leptos's task executor (leptos_axum only does that inside its own router helper, which we bypass), loads `LeptosOptions` from `[package.metadata.leptos]` (or the `LEPTOS_*` env vars) into `ctx.shared_store`, and parses the `settings:` block into `Settings` (also in the shared store). `after_context` also runs `assets::detect` (`src/assets.rs`): if cargo-leptos's `hash.txt` sits next to the executable, asset names are hashed, `options.hash_files` is switched on and the stylesheet path is resolved once into `Assets` (shared store); the chosen stylesheet must exist in the site folder, so a stale hash file or a hashed site without one refuses to boot (not in the test environment, which serves no assets and must pass whatever `target/site` holds). `render.rs` has `render_page`, which every page controller calls: it takes the options, settings and assets from the store, generates the CSP nonce, hands it to Leptos through `render_app_to_stream_with_context` (so the inline scripts carry it), and sets the page's Content-Security-Policy on the response. cargo-leptos writes the compiled bundle and stylesheet to `target/site`, which Loco's `static` middleware serves. Release builds always run with `LEPTOS_HASH_FILES=true` (never the watch loop, which hashes only on its first build); production caches static files for a year on the strength of those hashed names, development uses `no-cache`.

**Config-driven middleware.** Security headers, payload limits, timeouts, panic catching, static files and compression are Loco middlewares toggled in `config/*.yaml`, not code; `cargo loco middleware -c` shows the result, and `tests/config.rs` is the canary that pins the agreed values per environment, so a config change that alters policy must update that test too. Static headers use `secure_headers` with the `github` preset plus overrides (X-Frame-Options `DENY`, Referrer-Policy, Permissions-Policy, Cross-Origin-Opener-Policy, Cross-Origin-Resource-Policy `same-origin`, Cross-Origin-Embedder-Policy `require-corp`, Strict-Transport-Security in the preload form `max-age=31536000; includeSubDomains; preload`; staging adds `X-Robots-Tag: noindex, nofollow`). A CDN overwrites HSTS at the edge with the zone setting; the hstspreload.org submission is a one-time, apex-domain step after go-live. COEP means every resource a page loads must be same-origin or opt in; a cross-origin embed (map, video) cannot simply be added. `timeout_request` is 15 s everywhere, `server.ident: ""` drops `X-Powered-By`, `compression` stays off because the reverse proxy compresses, `fallback` stays off. Static caching is `static.cache_control`: `no-cache` in development, 60 s on staging, a year plus `immutable` in production. The page CSP is a template under `settings.security.content_security_policy` with a `{nonce}` placeholder, validated at boot in `src/settings.rs` and filled per request in `render_page`; Loco's middleware only adds headers the response lacks, so the preset CSP is the fallback for non-page responses. `style-src 'self'` means no inline `style=` attributes anywhere, and `default-src 'none'` means a new resource kind (video, iframe, worker) needs its own directive in all four configs before it loads. Rate limiting is the project's own `MiddlewareLayer` (`src/middleware/rate_limit.rs`, added to Loco's default stack in `Hooks::middlewares`, listed by `cargo loco middleware`): a `tower_governor` token bucket per visitor IP applied with `route_layer`, so static assets are exempt; numbers under `settings.rate_limit`; the key source is derived from `remote_ip.source` (`CfConnectingIp` behind Cloudflare, the reference proxy; `ConnectInfo` locally) and a deployed environment keyed on the peer refuses to boot. A stricter per-route bucket (login, registration) goes on that controller's `Routes::layer`, reusing `VisitorIp` and `too_many_requests`.

**Secrets.** Production reads `JWT_SECRET` and `MAILER_*` from the environment with no defaults and refuses to boot without them; staging has public placeholder defaults; development and test have values in the file. `tests/config.rs` renders staging and production with placeholders instead of touching the process environment.

## Conventions

- Add an `#[island]` only when a component genuinely needs browser interactivity; everything else stays a server-rendered `#[component]`.
- Styling is Tailwind utility classes inside `view!` macros; the Tailwind input file is `style/tailwind.css`. Never a `style=` attribute: the CSP is `style-src 'self'` and the request test fails on one.
- The document shell is `src/views/layout.rs` (`shell`); pages supply only what goes inside `<main>`. Per-page values travel in `PageMeta`. `APP_NAME` there is the one place the app's display name lives. The head already has favicons (`public/favicon/`, plus a copy at `public/favicon.ico` for the root probe), the manifest, `theme-color`, the iOS install tags and the safe-area viewport; Open Graph tags wait for a 1200×630 image.
- Everything under `public/` is copied into `site/` and served, in production with a year-long cache: a file whose content changes must change its name, and no `.DS_Store` or scratch files (the deploy deletes Finder files on the server after upload).
- Static pages outside Leptos (`public/404.html`, `src/middleware/rate_limit.html`) use the same Tailwind classes as the shell so the scanner keeps them in the stylesheet.
- Generated Sea-ORM entities in `src/models/_entities/` are not hand-edited; model logic goes in `src/models/*.rs`. Prefer editing files directly over `cargo loco generate` unless a generator's wiring (a new model with migration and entity) is what you need.
- Known gaps, documented rather than hidden: the static 404 page answers with status 200 (and in production is cached for the URL that missed) until a real not-found handler replaces Loco's static fallback; direct hits to the origin bypassing the CDN could forge `CF-Connecting-IP` until Caddy `trusted_proxies` or a firewall rule is configured (`README.DEPLOY.md`).

## Reference

- Loco + Leptos SSR showcase (the integration recipe this template follows, including the binary-name change from `app-cli` to `app` that cargo-leptos needs): https://github.com/loco-rs/loco/discussions/1748
- Leptos islands: https://book.leptos.dev/islands.html
- cargo-leptos: https://book.leptos.dev/ssr/21_cargo_leptos.html

## Working conventions

- Commit messages: one short imperative line, no `Co-Authored-By` or generated-by trailers.
- When handing over terminal commands, give a single `&&` pipeline to paste, not a list of separate commands.
- Make every change to a repo file with the Edit or Write tool, never through Bash (`cat > file`, `sed -i`, scripts). The maintainer reviews each change as a rendered diff in the VS Code agent window; Bash-written files show only the command. Bash is for reads, builds, tests and git.
- When asked to go file by file, change one file, explain it, and wait for a go before the next.
- Verify with `cargo clippy` / `cargo test`, not `cargo build`; it is faster and the difference is noticed.
- Questions are questions: "check", "audit", "explain" mean report and stop. When a request is ambiguous, confirm before acting; a misread once led to a revert.
- The maintainer runs the deploys and the server commands; supply the command and, where a result matters, the check that proves it worked.

## Role

Act as an experienced senior Rust developer delivering production-grade code. Concretely: no `unwrap()` or `expect()` on the request path (return `loco_rs::Error` / `Result`), no `unsafe`, no `#[allow(...)]` to silence warnings, `cargo clippy` and `cargo fmt` clean before handing over, and every change verified by actually building both halves (`cargo leptos build`) and running `cargo test`, not by reasoning that it should work. Prefer the boring, idiomatic solution over a clever one, and say plainly when something is untested or uncertain.
