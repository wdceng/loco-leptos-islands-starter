#!/usr/bin/env bash
# Says whether the two split fixes the patched cargo-leptos carries
# (install.sh) have been released, so the template can go back to a plain
# `cargo install` (docs/decisions.md, "Way back"). Reads GitHub's and
# crates.io's public APIs; no login needed.
#
# Exit 0: not yet, keep the patched build. Exit 2: both fixes look released,
# run the test it prints. Exit 1: an API could not be read.
set -euo pipefail

PR=691                   # cargo-leptos: stale names after a hashed split build
ISSUE=79                 # wasm-split-prototype: loader imports unsorted
PATCHED_SPLIT=0.2.3      # wasm_split_cli_support version install.sh patches

get() { curl -sfL -A "loco-leptos-islands-starter upstream check" "$1"; }
field() { sed -n "s/.*\"$1\": *\"\{0,1\}\([^\",}]*\)\"\{0,1\}.*/\1/p" | head -1; }

pr="$(get "https://api.github.com/repos/leptos-rs/cargo-leptos/pulls/$PR")" || { echo "cannot read cargo-leptos PR #$PR" >&2; exit 1; }
release="$(get https://api.github.com/repos/leptos-rs/cargo-leptos/releases/latest)" || { echo "cannot read cargo-leptos releases" >&2; exit 1; }
issue="$(get "https://api.github.com/repos/WorldSEnder/wasm-split-prototype/issues/$ISSUE")" || { echo "cannot read wasm-split issue #$ISSUE" >&2; exit 1; }
crate="$(get https://crates.io/api/v1/crates/wasm_split_cli_support)" || { echo "cannot read crates.io" >&2; exit 1; }

merged="$(printf '%s' "$pr" | tr ',' '\n' | field merged)"
merged_at="$(printf '%s' "$pr" | tr ',' '\n' | field merged_at)"
tag="$(printf '%s' "$release" | tr ',' '\n' | field tag_name)"
published="$(printf '%s' "$release" | tr ',' '\n' | field published_at)"
issue_state="$(printf '%s' "$issue" | tr ',' '\n' | field state)"
split_version="$(printf '%s' "$crate" | tr ',' '\n' | field max_stable_version)"
if [ "$merged_at" = "null" ]; then merged_at=""; fi

echo "cargo-leptos PR #$PR:      merged=$merged ${merged_at:+(at $merged_at)}"
echo "cargo-leptos release:      $tag ($published)"
echo "wasm-split issue #$ISSUE:     $issue_state"
echo "wasm_split_cli_support:    $split_version (patched here: $PATCHED_SPLIT)"
echo

fix1=no
if [ "$merged" = "true" ] && [[ "$published" > "$merged_at" ]]; then fix1=yes; fi
fix2=no
if [ "$issue_state" = "closed" ] && [ "$split_version" != "$PATCHED_SPLIT" ]; then fix2=yes; fi

if [ "$fix1" = no ] || [ "$fix2" = no ]; then
  [ "$fix1" = no ] && echo "not yet: PR #$PR is not in a release (merged=$merged, latest $tag)"
  [ "$fix2" = no ] && echo "not yet: wasm-split #$ISSUE is $issue_state, wasm_split_cli_support still $split_version"
  echo "Keep the patched build (tools/cargo-leptos/install.sh)."
  exit 0
fi

cat <<EOF
maybe: both fixes look released ($tag, wasm_split_cli_support $split_version).
A merge proves nothing about the release, so test the plain $tag, installed
into a scratch folder, with the template's two-build check:

  cargo install --locked --root /tmp/cargo-leptos-plain cargo-leptos@${tag#v} &&
  cp target/release/hash.txt /tmp/hash.txt.saved 2>/dev/null; for run in 1 2; do PATH=/tmp/cargo-leptos-plain/bin:\$PATH LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-check cargo leptos build --release --split --frontend-only >/dev/null && ls target/site-check/pkg | sort > /tmp/names-\$run.txt; done &&
  diff /tmp/names-1.txt /tmp/names-2.txt && grep -oh '__wasm_split\.[A-Za-z0-9_-]*\.js' target/site-check/pkg/app.*.js | head -1 | xargs -I{} test -f target/site-check/pkg/{} && echo "PLAIN $tag PASSES";
  cp /tmp/hash.txt.saved target/release/hash.txt 2>/dev/null; rm -rf target/site-check

If it passes, follow "Way back" in docs/decisions.md.
EOF
exit 2
