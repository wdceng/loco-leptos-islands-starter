# Loco + Leptos Islands Starter

[![CI](https://github.com/wdceng/loco-leptos-islands-starter/actions/workflows/ci.yaml/badge.svg)](https://github.com/wdceng/loco-leptos-islands-starter/actions/workflows/ci.yaml)

A whole web app in one Rust project: the server, the pages, the database, user accounts, e-mail. No JavaScript to write, no Node to install. Clone it, run one command, and you have a working site to build on.

This is a GitHub template. Press **Use this template** at the top of the page and you get your own copy to start from.

## What you get

- **A web server** built on Loco (which sits on Axum, the most used Rust web framework). It serves your pages and a small JSON API.
- **Pages written in Rust** with Leptos. The server turns them into plain HTML, so they load fast and work even before any code runs in the browser.
- **Interactive parts when you want them.** Mark a component with `#[island]` and it runs in the browser as WebAssembly. Only those components are sent to the browser, nothing else. Everything without the mark stays plain HTML.
- **Tailwind CSS** for styling. You write class names, the build tool produces the stylesheet.
- **A SQLite database** with migrations, through Sea-ORM. It is a single file next to your code. Nothing to install, nothing to run.
- **User accounts**: registration, e-mail verification, login, password reset and magic links, all from Loco's SaaS starter.
- **E-mail sending** with text and HTML templates.
- **Security already set up**: safe HTTP headers, a Content Security Policy, a rate limit per visitor, and secrets kept out of the code. Tests check that none of it quietly disappears.

## Run it in five minutes

You need Rust. If you do not have it yet, install it from https://rustup.rs and come back. Then, in a terminal inside your copy of the project:

```sh
rustup target add wasm32-unknown-unknown   # lets Rust compile for the browser
cargo install --locked cargo-leptos        # the build tool for Leptos projects
cargo leptos watch -- start                # build everything and run the server
```

The first build takes a few minutes, because every dependency compiles once. After that, builds take seconds. When the terminal says the server is listening, open http://localhost:5150.

Now open `src/views/home.rs`, change the text, save. The page in your browser reloads by itself.

A file called `app_development.sqlite` appeared in the project folder. That is your database. You can delete it whenever you like; it is recreated on the next start.

Two things you may want soon:

- **Mail.** Registering a user sends an e-mail, and on your machine there is no mail server to receive it. Either run a fake one with `docker run -p 1025:1025 -p 8025:8025 axllent/mailpit` and read the mail at http://localhost:8025, or open `config/development.yaml` and set `stub: true` under `mailer:` so mail is kept in memory instead of sent.
- **Tests.** `cargo test` runs them all. They take about a second.

## Where things are

| You want to... | Look in |
|---|---|
| Change what a page shows | `src/views/` |
| Add something interactive (an island) | `src/islands.rs` |
| Add a URL or decide what it answers | `src/controllers/` |
| Change the database or add a table | `src/models/` and `migration/` |
| Change the e-mails the app sends | `src/mailers/auth/` |
| Change styling | Tailwind classes in the views; `style/tailwind.css` for fonts and colours |
| Add an image, a font, a static file | `public/` |
| Change settings per environment | `config/development.yaml`, `staging.yaml`, `production.yaml` |

## Your first page

Every page is two small files: a view (what it looks like) and a controller (which URL shows it). The home page is the example to copy.

1. Copy `src/views/home.rs` to `src/views/about.rs`, rename the component, change the content. Add `pub mod about;` to `src/views/mod.rs`.
2. Copy `src/controllers/home.rs` to `src/controllers/about.rs`. Point it at your new component and change the URL in `routes()` to `/about`. Add `pub mod about;` to `src/controllers/mod.rs`.
3. In `src/app.rs`, find `fn routes` and add `.add_route(controllers::about::routes())` next to the home one.

Save, and http://localhost:5150/about is live. `cargo loco routes` prints every URL the app knows, if you want to check.

## Your first island

Say you want a button that shows and hides a paragraph. That needs code running in the browser.

Write the component in `src/islands.rs` with `#[island]` instead of `#[component]`, then use it from a view like any other component. The file matters: `src/islands.rs` is the one module compiled for the browser as well as the server. A component in `src/views/` is compiled for the server only, so an `#[island]` there renders fine but never wakes up in the browser.

Three rules keep it small and working. Pass the content in as `children` (`pub fn ShowMore(children: Children)`): children are rendered on the server and never travel to the browser, so the island stays a thin wrapper around them. Anything you pass as a prop has to be plain data (a `String`, a number, a struct with `#[derive(Serialize, Deserialize)]`). And an island cannot touch server-only things like the database; fetch what it needs from the JSON API instead.

The page wraps every island in a `<leptos-island>` element; the stylesheet already makes those invisible to layout, so grids and flex rows are not disturbed. Check that both halves compile cleanly with `cargo clippy --all-targets` and `cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features hydrate`.

The template ships without any island on purpose, so the first one is yours.

## How it fits together

When the browser asks for a page, Loco finds the controller for that URL. The controller hands a Leptos component to the renderer, which produces the finished HTML and sends it back. The browser shows it right away. Then a small WebAssembly file loads and switches on the islands, if the page has any. Links are normal links: clicking one requests a new page from the server, like a classic website. There is no client-side router to learn.

If you are curious why things are built the way they are, `ARCHITECTURE.md` explains it.

## Make it yours

The project is called `app` in a handful of places. To rename it, search the repo for `app` in `Cargo.toml`, `.cargo/config.toml`, `src/bin/main.rs`, `examples/playground.rs`, the `tests/` folder, `public/404.html`, `src/middleware/rate_limit.html`, the `app_<env>.sqlite` lines in `config/*.yaml`, the `LEPTOS_OUTPUT_NAME=app` and `ExecStart=<app-dir>/app start` lines of the unit in `DEPLOYMENT.md`, and the `target/release/app` and `target/debug/app` commands in `DEVELOPMENT.md` and `TESTING.md`. Keep the binary named exactly like the package: cargo-leptos trips over Loco's default `-cli` suffix, and a `LEPTOS_OUTPUT_NAME` that does not match the new name makes production link asset files that do not exist. Or leave it; nothing breaks if you keep the name.

What people see: `APP_NAME` in `src/views/layout.rs` sets the title, header and footer. The page description is in `src/controllers/home.rs`, the tagline in `src/views/home.rs`, the icons in `public/favicon/`, and the app name for phone home screens in `public/favicon/site.webmanifest`. Colours live in `style/tailwind.css`: one brand colour (`--color-primary`) and a few role tokens on top of Tailwind's default palette. The page background is repeated as a hex value in two places, the `theme-color` tag in `src/views/layout.rs` and `background_color` in the manifest, so change all three together.

One thing to know: magic-link login only accepts `@example.com` and `@gmail.com` addresses, because that is how Loco's starter ships. The list is `EMAIL_DOMAIN_RE` in `src/controllers/auth.rs`.

Staging and production restart themselves once a day, at 03:00 UTC as shipped, so no process runs for weeks on end. The hour and the time zone are the `nightly_restart` block in `config/staging.yaml` and `config/production.yaml`; set the zone to your own, for example `Europe/Zagreb`.

## Going further

- `DEVELOPMENT.md`: every command, what each environment does, release builds, and the gotchas we ran into.
- `TESTING.md`: what the tests cover and how to check the security features by hand.
- `DEPLOYMENT.md`: putting it on a Linux server, step by step, with Caddy for HTTPS. Read this before your first deploy: the staging and production configs assume a reverse proxy in front of the app, and the file explains what to change if yours is different.
- `ARCHITECTURE.md`: the reasoning. The security headers and what each one does, why the database is SQLite, what every crate is for.
- `PREREQUISITES.md`: every tool, including the optional ones.

## Known gaps

- The "page not found" page is served with status 200, not 404, until a proper not-found handler exists.
- There is no island yet. The wiring is done; the first component is up to you.
- Behind a proxy, a request that reaches the server directly could fake its IP address for the rate limiter. `DEPLOYMENT.md` explains the fix.

## License

MIT or Apache-2.0, your choice (`LICENSE-MIT`, `LICENSE-APACHE`). The Inter font in `public/fonts/` has its own license, the SIL Open Font License, in `public/fonts/OFL.txt`.
