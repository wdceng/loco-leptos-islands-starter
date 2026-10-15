#!/usr/bin/env bash
# Says whether the installed cargo-leptos has both fixes that lazy islands
# with hashed file names need (tools/cargo-leptos/install.sh). Exits 1 when
# it hasn't, so a deploy chain that starts with it stops before building.
set -euo pipefail

REV=0fb3de8665b1cd345a99fda8fd9d669bce57821c   # keep equal to install.sh

line="$(cargo install --list | grep '^cargo-leptos ' || true)"
if [ -z "$line" ]; then
  echo "cargo-leptos is not installed: run tools/cargo-leptos/install.sh" >&2
  exit 1
fi

# `cargo-leptos v0.3.11 (/path/to/source):` for a build from a folder.
src="$(printf '%s' "$line" | sed -n 's/^cargo-leptos v[^ ]* (\(\/[^)]*\)):$/\1/p')"
if [ -z "$src" ]; then
  echo "cargo-leptos is a plain install ($line): split builds with hashed names" >&2
  echo "get stale or changing names. Run tools/cargo-leptos/install.sh" >&2
  exit 1
fi

problems=()
[ "$(git -C "$src" rev-parse HEAD 2>/dev/null || true)" = "$REV" ] ||
  problems+=("$src is not at $REV (cargo-leptos PR #691)")
grep -q 'exported.sort();' "$src/../wasm_split_cli_support/src/js.rs" 2>/dev/null ||
  problems+=("its wasm_split_cli_support is not sorted (wasm-split-prototype #79)")
grep -q 'wasm_split_cli_support = { path = "../wasm_split_cli_support" }' "$src/Cargo.toml" 2>/dev/null ||
  problems+=("$src/Cargo.toml does not patch wasm_split_cli_support")

if [ ${#problems[@]} -gt 0 ]; then
  printf 'cargo-leptos from %s lacks a fix:\n' "$src" >&2
  printf '  - %s\n' "${problems[@]}" >&2
  echo "Run tools/cargo-leptos/install.sh" >&2
  exit 1
fi
echo "cargo-leptos from $src: both fixes in"
