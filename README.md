# Loco + Leptos Islands Starter

[![CI](https://github.com/wdceng/loco-leptos-islands-starter/actions/workflows/ci.yaml/badge.svg)](https://github.com/wdceng/loco-leptos-islands-starter/actions/workflows/ci.yaml)

A whole web app in one Rust project: server, pages, database, user accounts,
e-mail. No JavaScript to write, no Node to install.

It's a GitHub template. Press **Use this template** at the top of the page to
get your own copy.

## What it is built from

| Part | Comes from |
|---|---|
| Server, database, user accounts, e-mail, background jobs | Loco's **Rest API** starter: `loco new` with "Rest API (with DB and user auth)", SQLite, async jobs |
| Pages and interactivity | **Leptos in islands mode**, added on top: pages render on the server, only `#[island]` components run in the browser |
| **Security**: HTTP headers, a Content Security Policy with a new nonce on every page, rate limits per visitor (stricter on sign-up and login, 404s counted apart), a 64 KB body limit, a 15 s timeout | This template |
| **Page basics**: page shell with favicons, web manifest and theme colour, the home page, a rendered 404 page, `robots.txt`, Tailwind v4, the self-hosted Inter font | This template |
| **Running it**: a staging environment next to development, test and production, settings checked at boot, hashed asset names for a one-year cache in production, a nightly restart, SQLite only | This template |
| **Mail fixes**: sender set in config, links to the public address without the server's port, names escaped in HTML mail | This template |
| **Checks**: tests for the headers, the CSP, the rate limits, the 404 page and all four config files; CI with fmt, clippy on both halves, the tests, a full build and `cargo audit` | This template |
| **Docs**: `docs/DEVELOPMENT.md`, `docs/TESTING.md`, `docs/DEPLOYMENT.md` (systemd, Caddy, Cloudflare), `docs/ARCHITECTURE.md` | This template |

Nothing Loco generated was removed. Some files are extended, so Loco's own
docs still apply. `docs/ARCHITECTURE.md` covers every part in detail.

Why not Loco's "SaaS App with server side rendering"? It has the same
accounts, e-mail and database code, plus its own way of making pages: Tera
templates, translation files and a `/static` folder. Here Leptos makes the
pages, so the Rest API starter left nothing to remove.

To see what Loco generated, install the Loco CLI (see `docs/PREREQUISITES.md`)
and run `loco new -n app --db sqlite --bg async --assets none` in an empty
folder.

## What you get

- **A web server** on Loco, which runs on Axum, the most used Rust web
  framework. It serves your pages and a small JSON API.
- **Pages in Rust** with Leptos, turned into plain HTML on the server. They
  load fast and work before any browser code runs.
- **Interactive parts when you want them.** A component marked `#[island]`
  runs in the browser as WebAssembly. Only islands go to the browser,
  everything else stays plain HTML.
- **Tailwind CSS.** You write class names, the build makes the stylesheet.
- **A SQLite database** with migrations, through Sea-ORM. One file next to
  your code, nothing to install or run.
- **User accounts**: registration, e-mail verification, login, password reset,
  magic links.
- **E-mail** with text and HTML templates.
- **Security already set up**: headers, a Content Security Policy, rate limits,
  secrets kept out of the code. Tests make sure none of it quietly disappears.

## Run it in five minutes

You need Rust. If you don't have it, install it from https://rustup.rs. Then,
in a terminal inside your copy of the project:

```sh
rustup target add wasm32-unknown-unknown      # compile for the browser
cargo install --locked cargo-leptos@0.3.8     # Leptos build tool, same version as CI
cargo leptos watch -- start                   # build and run the server
```

The first build takes a few minutes while every dependency compiles. After
that, seconds. When the terminal says the server is listening, open
http://localhost:5150.

Change the text in `src/views/home.rs` and save. The page reloads by itself.

The new `app_development.sqlite` in the project folder is your database.
Delete it whenever you like, the next start makes a new one.

Two things you'll want soon:

**Mail.** Registering a user sends an e-mail, and your machine has no mail
server. Run a fake one and read the mail at http://localhost:8025:

```sh
docker run -p 1025:1025 -p 8025:8025 axllent/mailpit
```

Or set `stub: true` under `mailer:` in `config/development.yaml` to keep mail
in memory instead. The sender is `from:` under `settings: mail:` in the same
file.

**Tests.** `cargo test` runs them all in about a second.

## Where things are

| You want to... | Look in |
|---|---|
| Change what a page shows | `src/views/` |
| Add something interactive (an island) | `src/islands.rs` |
| Add a URL or change what it answers | `src/controllers/` |
| Change the database or add a table | `src/models/` and `migration/` |
| Change the e-mails or their sender | `src/mailers/auth/`, sender: `settings.mail.from` in `config/` |
| Change styling | Tailwind classes in the views, fonts and colours in `style/tailwind.css` |
| Add an image, a font, a static file | `public/` |
| Change settings per environment | `config/development.yaml`, `staging.yaml`, `production.yaml` |

## Your first page

A page is two small files: a view (what it looks like) and a controller
(which URL shows it). Copy the home page:

1. Copy `src/views/home.rs` to `src/views/about.rs`, rename the component,
   change the content. Add `pub mod about;` to `src/views/mod.rs`.
2. Copy `src/controllers/home.rs` to `src/controllers/about.rs`. Point it at
   your new component and change the URL in `routes()` to `/about`. Add
   `pub mod about;` to `src/controllers/mod.rs`.
3. In `src/app.rs`, find `fn routes` and add
   `.add_route(controllers::about::routes())` next to the home one.

Save, and http://localhost:5150/about is live. `cargo loco routes` lists
every URL if you want to check.

## Your first island

Say you want a button that shows and hides a paragraph. That needs code in
the browser.

Write the component in `src/islands.rs` with `#[island]` instead of
`#[component]`, then use it in a view like any other component.

**Islands go in `src/islands.rs`.** It's the only file also compiled for the
browser, so an island in `src/views/` renders but never runs.

Three rules keep it small and working:

- **Pass content in as `children`**, like
  `pub fn ShowMore(children: Children)`. Children render on the server and
  never go to the browser, so the island stays a thin wrapper.
- **Props must be plain data**: a `String`, a number, a struct with
  `#[derive(Serialize, Deserialize)]`.
- **No server-only things** like the database. Fetch what you need from the
  JSON API.

Each island is wrapped in a `<leptos-island>` element. The stylesheet already
makes it invisible to layout, so grids and flex rows still work.

Check that both halves compile cleanly:

```sh
cargo clippy --all-targets
cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
```

## How it fits together

The browser asks for a page. Loco finds the controller for that URL, the
controller renders a Leptos component to HTML, and the browser shows it right
away. Then a small WebAssembly file loads and switches on the islands, if
there are any.

Links are normal links. Each click loads a new page from the server, like a
classic website. There's no client-side router to learn.

The reasons are in `docs/ARCHITECTURE.md`.

## Make it yours

The project is called `app`. You can keep that, nothing breaks. To rename it,
search for `app` in:

- `Cargo.toml`
- `.cargo/config.toml`
- `src/bin/main.rs`
- `examples/playground.rs`
- the `tests/` folder
- `public/404.html`
- `src/middleware/rate_limit.html`
- the `app_<env>.sqlite` lines in `config/*.yaml`
- the commands in `docs/DEVELOPMENT.md`, `docs/TESTING.md` and `docs/DEPLOYMENT.md`: the
  `target/release/app` and `target/debug/app` paths, `./app start`,
  `LEPTOS_OUTPUT_NAME=app`, and the unit's `ExecStart=<app-dir>/app start`

**Name the binary exactly like the package.** cargo-leptos trips over Loco's
default `-cli` suffix.

**`LEPTOS_OUTPUT_NAME` must match the new name.** Otherwise production links
to asset files that don't exist.

What people see:

| What | Where |
|---|---|
| Title, header and footer | `APP_NAME` in `src/views/layout.rs` |
| Page description | `src/controllers/home.rs` |
| Tagline | `src/views/home.rs` |
| Icons | `public/favicon/` |
| App name on phone home screens | `public/favicon/site.webmanifest` |
| Colours | `style/tailwind.css`: one brand colour, `--color-primary`, and a few role tokens on top of Tailwind's default palette |
| Mail sender, `SaaS Starter <noreply@example.com>` | `config/development.yaml` and `config/test.yaml`, the staging default in `config/staging.yaml`, production reads `MAILER_FROM` |

**Change the page background in all three places.** It's in
`style/tailwind.css`, and repeated as a hex value in the `theme-color` tag in
`src/views/layout.rs` and in `background_color` in the manifest.

**Magic-link login only accepts `@example.com` and `@gmail.com`.** That's how
Loco's starter ships, and the list is `EMAIL_DOMAIN_RE` in
`src/controllers/auth.rs`.

**Staging and production restart every night at 03:00 UTC.** Set the hour and
your own time zone, for example `Europe/Zagreb`, in the `nightly_restart`
block of `config/staging.yaml` and `config/production.yaml`.

## Going further

- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): every command, the environments, release builds and the
  gotchas.
- [docs/TESTING.md](docs/TESTING.md): what the tests cover and how to check the security by hand.
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md): a Linux server step by step, with Caddy for HTTPS. Read it
  before your first deploy. Staging and production assume a reverse proxy in
  front of the app, and it says what to change if yours is different.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): the reasoning. What each security header does, why
  SQLite, what every crate is for.
- [docs/PREREQUISITES.md](docs/PREREQUISITES.md): every tool, optional ones too.

## Known gaps

- **No island yet.** The wiring is done, the first one is yours.
- **The password-reset mail links to a `/reset` page that doesn't exist yet.**
  Loco's starter leaves that page to your front end. The reset itself works
  through the JSON API, `POST /api/auth/reset`.
- **Behind a proxy, a request that reaches the server directly can fake its IP
  for the rate limiter.** `docs/DEPLOYMENT.md` explains the fix.

## License

MIT or Apache-2.0, your choice: `LICENSE-MIT`, `LICENSE-APACHE`. The Inter
font in `public/fonts/` has its own license, the SIL Open Font License, in
`public/fonts/OFL.txt`.
