# Prerequisites

Everything is Rust. There is no Node.js toolchain: Tailwind, wasm-bindgen and
wasm-opt are standalone binaries that cargo-leptos downloads on first use.
Install commands below are the tool's own; substitute your package manager
where you prefer one.

## 1. Required

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 1.1 | **C toolchain** | Linker and system libraries for the native build | Xcode Command Line Tools on macOS (`xcode-select --install`), `build-essential` on Debian/Ubuntu, Visual Studio Build Tools on Windows |
| 1.2 | **Rust** (stable, 1.85+) | Server and browser halves | https://rustup.rs |
| 1.3 | **wasm32 target** | Compiles the islands bundle | `rustup target add wasm32-unknown-unknown` |
| 1.4 | **cargo-leptos** | Builds both halves, runs Tailwind, dev loop with live reload | `cargo install --locked cargo-leptos` |

`clippy` and `rustfmt` come with the Rust toolchain.

## 2. Optional Cargo Tools

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 2.1 | **cargo-watch** | Only for `cargo loco watch` (server-only reload); `cargo leptos watch -- start` is the normal loop | `cargo install --locked cargo-watch` |
| 2.2 | **cargo-audit** | Known-vulnerability scan of `Cargo.lock`; CI runs it, `.cargo/audit.toml` lists the ignored advisories | `cargo install --locked cargo-audit` |
| 2.3 | **cross** | Linux x86_64 binary for a server from another platform (`DEPLOY.md`). Needs Docker; `Cross.toml` pins the build image | `cargo install --locked cross` |
| 2.4 | **cargo-binstall** | Installs cargo tools from prebuilt binaries instead of compiling them; CI uses it for cargo-leptos and cargo-audit | `cargo install --locked cargo-binstall` |
| 2.5 | **loco** | The Loco CLI, only for `loco new` and its generators. Not needed to run this project: `cargo loco` is a cargo alias in `.cargo/config.toml` that runs the app binary | `cargo install --locked loco` |

On an arm64 host (Apple Silicon and others) `cross` emulates x86_64 and the
default image tag has no arm64 manifest; pull the pinned one once with
`docker pull --platform linux/amd64 ghcr.io/cross-rs/x86_64-unknown-linux-gnu:main`.

## 3. Local Services

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 3.1 | **SMTP catcher** | `config/development.yaml` sends mail to `localhost:1025`; registration and password reset need something listening there. Mailpit, MailHog and maildev all default to 1025 with a web inbox on 8025 | `docker run -d -p 1025:1025 -p 8025:8025 axllent/mailpit`, or a native build from https://github.com/axllent/mailpit. Or set `stub: true` under `mailer:` to record mail in memory instead |
| 3.2 | **sqlite3** | Inspect the database files (`app_<env>.sqlite`); `.backup` makes a consistent copy while the app runs | Package manager, or https://sqlite.org/download.html |

## 4. Downloaded by cargo-leptos

Nothing to install. Fetched into `~/.cache/cargo-leptos` on first build; the
version is derived from `Cargo.lock` where one applies.

| # | Tool | Purpose | Pin a version |
|-----|------|---------|---------------|
| 4.1 | **tailwindcss** | Compiles `style/tailwind.css` | `LEPTOS_TAILWIND_VERSION=v4.x.y` |
| 4.2 | **wasm-bindgen** | JS glue for the wasm bundle | matched to the crate in `Cargo.lock` |
| 4.3 | **wasm-opt** | Shrinks the release wasm | `LEPTOS_WASM_OPT_VERSION=version_NNN` |

## 5. On a Server Only

| # | Tool | Purpose |
|-----|------|---------|
| 5.1 | **Docker** | The engine behind `cross` (2.3) on the build machine; nothing else in this template needs it |
| 5.2 | **Caddy** | Reverse proxy with automatic HTTPS in front of Loco (`DEPLOY.md`); nothing to run locally |

## 6. Editor (VS Code)

| # | Extension | Purpose |
|-----|-----------|---------|
| 6.1 | **rust-analyzer** | Rust language support |
| 6.2 | **Tailwind CSS IntelliSense** (`bradlc.vscode-tailwindcss`) | Class completion and hover previews inside `view!` macros; `.vscode/settings.json` already maps Rust files for it |

## Verify

```bash
rustup target list --installed | grep wasm32 && cargo leptos --version
```

## Updates

```bash
rustup update                              # Rust, clippy, rustfmt
cargo install --locked cargo-leptos        # reinstalling a cargo tool updates it
```
