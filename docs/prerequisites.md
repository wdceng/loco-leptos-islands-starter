# Prerequisites

It's all Rust, no Node.js. Tailwind, wasm-bindgen and wasm-opt are standalone
binaries that cargo-leptos downloads on first use.

Install commands are each tool's own. Your package manager works too.

## 1. Required

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 1.1 | **C toolchain** | Linker and system libraries for the native build | Xcode Command Line Tools on macOS (`xcode-select --install`), `build-essential` on Debian/Ubuntu, Visual Studio Build Tools on Windows |
| 1.2 | **Rust**, stable 1.94+ | Compiles both halves. Loco 1.2 and Sea-ORM 2.0.4 need 1.94+ | https://rustup.rs |
| 1.3 | **wasm32 target** | Compiles the islands bundle | `rustup target add wasm32-unknown-unknown` |
| 1.4 | **cargo-leptos** | Builds both halves, runs Tailwind and the live-reload dev loop. Use 0.3.8 to match CI (`.github/workflows/ci.yaml`) | `cargo install --locked cargo-leptos@0.3.8` |

`clippy` and `rustfmt` come with Rust.

## 2. Optional Cargo Tools

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 2.1 | **cargo-watch** | Only for `cargo loco watch`, which reloads just the server. The usual loop is `cargo leptos watch -- start` | `cargo install --locked cargo-watch` |
| 2.2 | **cargo-audit** | Scans `Cargo.lock` for known vulnerabilities. CI runs 0.22.2. `.cargo/audit.toml` lists ignored advisories | `cargo install --locked cargo-audit@0.22.2` |
| 2.3 | **cross** | Builds a Linux x86_64 server binary on another platform (`staging.md`, `production.md`). Needs Docker. `Cross.toml` pins the image | `cargo install --locked cross` |
| 2.4 | **cargo-binstall** | Installs prebuilt cargo tools instead of compiling them. CI uses it for cargo-leptos and cargo-audit | `cargo install --locked cargo-binstall` |
| 2.5 | **loco** | The Loco CLI, only for `loco new` and generators. Not needed here: `cargo loco` is a cargo alias in `.cargo/config.toml` that runs the app binary | `cargo install --locked loco` |
| 2.6 | **cargo-nextest** | One process per test (`cargo nextest run`). `.config/nextest.toml` keeps app-booting tests on one thread | `cargo install --locked cargo-nextest` |
| 2.7 | **cargo-sweep** | Clears old native, wasm32 and cross builds out of `target/` | `cargo install --locked cargo-sweep` |
| 2.8 | **sea-orm-cli** | Only for `cargo loco db entities`, which regenerates `src/models/_entities/`. `cargo loco doctor` flags it if missing | `cargo install --locked sea-orm-cli` |

On arm64, like Apple Silicon, `cross` emulates x86_64 and the default image
tag has no arm64 manifest. Pull the pinned image once:

```bash
docker pull --platform linux/amd64 ghcr.io/cross-rs/x86_64-unknown-linux-gnu:main
```

## 3. Local Services

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 3.1 | **SMTP catcher** | `config/development.yaml` sends mail to `localhost:1025`. Registration and password reset need something listening. Mailpit, MailHog and maildev all default to 1025, with a web inbox on 8025 | `docker run -d -p 1025:1025 -p 8025:8025 axllent/mailpit`, or a native build from https://github.com/axllent/mailpit. Or set `stub: true` under `mailer:` to keep mail in memory |
| 3.2 | **sqlite3** | Inspect the database files (`app_<env>.sqlite`). `.backup` makes a consistent copy while the app runs | Package manager, or https://sqlite.org/download.html |

## 4. Downloaded by cargo-leptos

Nothing to install. They land in `~/.cache/cargo-leptos` on the first build.

| # | Tool | Purpose | Pin a version |
|-----|------|---------|---------------|
| 4.1 | **tailwindcss** | Compiles `style/tailwind.css` | `LEPTOS_TAILWIND_VERSION=v4.x.y` |
| 4.2 | **wasm-bindgen** | JS glue for the wasm bundle | matched to the crate in `Cargo.lock` |
| 4.3 | **wasm-opt** | Shrinks the release wasm | `LEPTOS_WASM_OPT_VERSION=version_NNN` |

## 5. On a Server Only

| # | Tool | Purpose |
|-----|------|---------|
| 5.1 | **Docker** | `cross` (2.3) needs it on the build machine. Nothing else does |
| 5.2 | **Caddy** | Reverse proxy with automatic HTTPS in front of Loco (`staging.md`, `production.md`). Not needed locally |

## 6. Editor (VS Code)

| # | Extension | Purpose |
|-----|-----------|---------|
| 6.1 | **rust-analyzer** | Rust language support |
| 6.2 | **Tailwind CSS IntelliSense** (`bradlc.vscode-tailwindcss`) | Class completion and hover previews inside `view!`. `.vscode/settings.json` already enables it for Rust files |

`.vscode/extensions.json` recommends both, so VS Code offers them when you
open the folder.

## Verify

You're set if this prints the target and a version:

```bash
rustup target list --installed | grep wasm32 && cargo leptos --version
```

## Updates

```bash
rustup update                                     # Rust, clippy, rustfmt
cargo install --locked cargo-leptos@<version>     # then the same version in .github/workflows/ci.yaml
```
