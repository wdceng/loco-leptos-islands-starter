#!/usr/bin/env bash
# Builds and installs cargo-leptos with the two fixes that lazy islands with
# hashed file names need, until both are released (docs/development.md,
# "Lazy islands"):
#   1. cargo-leptos PR #691 (issue #690): the main .js/.wasm keep stale names
#      after the split loader's name is written into them. Fixed at REV.
#   2. wasm-split-prototype issue #79: the loader's imports come out in
#      random order, so every build gets new names. Fixed by the patch here.
# Takes about 3 minutes. Run it again after this script or the patch
# changes; tools/cargo-leptos/check.sh says whether you need to.
set -euo pipefail

REV=0fb3de8665b1cd345a99fda8fd9d669bce57821c   # PR #691, branch `690`
SPLIT_VERSION=0.2.3                            # wasm_split_cli_support
DIR="${CARGO_LEPTOS_SRC:-$HOME/src/cargo-leptos-split}"
HERE="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$DIR"

# cargo-leptos at the fixed commit.
if [ ! -d "$DIR/cargo-leptos/.git" ]; then
  git clone https://github.com/leptos-rs/cargo-leptos "$DIR/cargo-leptos"
fi
git -C "$DIR/cargo-leptos" fetch --quiet origin "$REV" || git -C "$DIR/cargo-leptos" fetch --quiet origin
git -C "$DIR/cargo-leptos" checkout --quiet --force "$REV"

# wasm_split_cli_support from crates.io, sorted.
rm -rf "$DIR/wasm_split_cli_support"
curl -sfL "https://crates.io/api/v1/crates/wasm_split_cli_support/$SPLIT_VERSION/download" | tar -xz -C "$DIR"
mv "$DIR/wasm_split_cli_support-$SPLIT_VERSION" "$DIR/wasm_split_cli_support"
patch --quiet -p1 -d "$DIR/wasm_split_cli_support" < "$HERE/wasm-split-sort.patch"

# cargo-leptos builds against the sorted copy.
cat >> "$DIR/cargo-leptos/Cargo.toml" <<'EOF'

[patch.crates-io]
wasm_split_cli_support = { path = "../wasm_split_cli_support" }
EOF

# The first build rewrites the patched crate's Cargo.lock entry (registry to
# path), which `--locked` would refuse; the install then runs locked.
cargo build --release --manifest-path "$DIR/cargo-leptos/Cargo.toml"
cargo install --locked --force --path "$DIR/cargo-leptos"

"$HERE/check.sh"
