# Prerequisites

Everything is Rust. There is no Node.js toolchain: Tailwind, wasm-bindgen and
wasm-opt are standalone binaries that cargo-leptos downloads on first use.

## 1. Required

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 1.1 | **Xcode CLI** | macOS build tools | `xcode-select --install` |
| 1.2 | **Bash** | Modern shell (v5+) | `brew install bash` |
| 1.3 | **Rust** | Server and browser halves | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| 1.4 | **wasm32 target** | Compiles the islands bundle | `rustup target add wasm32-unknown-unknown` |
| 1.5 | **cargo-leptos** | Builds both halves, runs Tailwind, dev loop with live reload | `cargo install --locked cargo-leptos` |

## 2. Rust Tools

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 2.1 | **clippy** | Rust linter | included with Rust |
| 2.2 | **loco** | Loco CLI; only needed for `loco new` (the scaffold already exists) | `cargo install --locked loco` |
| 2.3 | **cargo-watch** | Only for `cargo loco watch` (server-only reload); `cargo leptos watch -- start` is the normal loop | `cargo install --locked cargo-watch` |
| 2.4 | **cross** | Linux builds for the server from macOS. Needs Docker and the build image, pulled once with the platform flag on Apple Silicon (`Cross.toml` pins this tag) | `cargo install cross && docker pull --platform linux/amd64 ghcr.io/cross-rs/x86_64-unknown-linux-gnu:main` |
| 2.5 | **cargo-audit** | Security vulnerability scanner | `cargo install cargo-audit` |
| 2.6 | **cargo-outdated** | Check outdated dependencies | `cargo install cargo-outdated` |
| 2.7 | **cargo-update** | Update all cargo binaries | `cargo install cargo-update` |
| 2.8 | **cargo-sweep** | Incremental target/ cleanup (delete artifacts unused N+ days) | `cargo install cargo-sweep` |
| 2.9 | **cargo-binstall** | Installs cargo tools from prebuilt binaries instead of compiling them | `cargo install cargo-binstall` |

## 2b. Local Services

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 2b.1 | **Mailpit** | Local SMTP catcher. `config/development.yaml` sends mail to `localhost:1025`; registration and password reset need something listening there. Inbox at http://localhost:8025 | `brew install mailpit && brew services start mailpit` |
| 2b.2 | **sqlite3** | Inspect the database files (`app_development.sqlite`, and on the server `app_<env>.sqlite`); `.backup` makes a consistent copy while the app runs | ships with macOS |

## 3. Downloaded by cargo-leptos

Nothing to install. Fetched into `~/.cache/cargo-leptos` on first build; the
version is derived from `Cargo.lock` where one applies.

| # | Tool | Purpose | Pin a version |
|-----|------|---------|---------------|
| 3.1 | **tailwindcss** | Compiles `style/tailwind.css` | `LEPTOS_TAILWIND_VERSION=v4.x.y` |
| 3.2 | **wasm-bindgen** | JS glue for the wasm bundle | matched to the crate in `Cargo.lock` |
| 3.3 | **wasm-opt** | Shrinks the release wasm | `LEPTOS_WASM_OPT_VERSION=version_NNN` |

## 4. Optional

| # | Tool | Purpose | Install |
|-----|------|---------|---------|
| 4.1 | **Docker** | The engine behind `cross` (2.4), and the container path (README.Docker.md); the reference deploy is a bare binary | `brew install --cask docker` |
| 4.2 | **Caddy** | Reverse proxy (HTTPS) in front of Loco. On the server it is the shared native Caddy (README.DPY-falkenstein-1.md); nothing to run locally | not needed on macOS |

## 5. Editor (VS Code)

| # | Extension | Purpose |
|-----|-----------|---------|
| 5.1 | **rust-analyzer** | Rust language support |
| 5.2 | **Tailwind CSS IntelliSense** (`bradlc.vscode-tailwindcss`) | Class completion and hover previews inside `view!` macros; `.vscode/settings.json` already maps Rust files for it |

## Verify

```bash
rustup target list --installed | grep wasm32 && cargo leptos --version && cargo loco --version
```

## Updates

### Rust and Included Tools
```bash
rustup update
```

### Cargo Tools
```bash
cargo install-update -a
```

### Brew Packages
```bash
brew update && brew upgrade
```
