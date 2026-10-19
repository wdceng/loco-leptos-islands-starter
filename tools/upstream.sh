#!/usr/bin/env bash
# The upstream fixes this template works around, and whether each has been
# released, so its workaround can go (docs/decisions.md, each "Way back").
# Reads GitHub's and crates.io's public APIs; no login needed.
#
#   1. cargo-leptos PR #691 (issue #690): stale names after a hashed split
#      build. Workaround: the patched cargo-leptos (tools/cargo-leptos/).
#   2. wasm-split-prototype #79: the split loader's imports unsorted, new
#      names on every build. Workaround: the same patched cargo-leptos.
#   3. leptos PR #4953: an island's state disposed when it leaves the page.
#      Workaround: an island collector, until it is released.
#   4. htmx #4147: Chrome 150 to 156's parseHTMLUnsafe drops the comments
#      Leptos hydrates by. Workaround: always pass it options
#      (public/js/htmx-islands.js), until Chrome 157 is everywhere.
#   5. leptos #4959: the islands loader waits for idle time with no timeout,
#      so Chrome 114 to 123 can start no island. Workaround: Leptos from a
#      fork with the timeout ([patch.crates-io] in Cargo.toml).
#
# Exit 0: nothing to act on. Exit 2: something looks released or closed,
# see "act:" lines. Exit 1: an API could not be read.
set -euo pipefail

PATCHED_SPLIT=0.2.3   # wasm_split_cli_support version tools/cargo-leptos/ patches

get() { curl -sfL -A "loco-leptos-islands-starter upstream check" "$1" || { echo "cannot read $1" >&2; exit 1; }; }
field() { tr ',' '\n' | sed -n "s/.*\"$1\": *\"\{0,1\}\([^\",}]*\)\"\{0,1\}.*/\1/p" | head -1; }
nonull() { [ "$1" = "null" ] && echo "" || echo "$1"; }
newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]; }

act=0

# 1. cargo-leptos #691, and 2. what the latest cargo-leptos release locks.
pr="$(get https://api.github.com/repos/leptos-rs/cargo-leptos/pulls/691)"
cl_merged="$(printf '%s' "$pr" | field merged)"
cl_merged_at="$(nonull "$(printf '%s' "$pr" | field merged_at)")"
release="$(get https://api.github.com/repos/leptos-rs/cargo-leptos/releases/latest)"
cl_tag="$(printf '%s' "$release" | field tag_name)"
cl_published="$(printf '%s' "$release" | field published_at)"
lock="$(get "https://raw.githubusercontent.com/leptos-rs/cargo-leptos/$cl_tag/Cargo.lock")"
cl_locks="$(printf '%s\n' "$lock" | grep -A1 '^name = "wasm_split_cli_support"' | sed -n 's/^version = "\(.*\)"/\1/p')"
split_issue="$(nonull "$(get https://api.github.com/repos/WorldSEnder/wasm-split-prototype/issues/79 | field state)")"
split_crate="$(get https://crates.io/api/v1/crates/wasm_split_cli_support | field max_stable_version)"

fix1=no
if [ "$cl_merged" = "true" ] && [[ "$cl_published" > "$cl_merged_at" ]]; then fix1=yes; fi
fix2=no
if newer "$cl_locks" "$PATCHED_SPLIT"; then fix2=yes; fi

echo "1. cargo-leptos #691      merged=$cl_merged${cl_merged_at:+ ($cl_merged_at)}, latest release $cl_tag ($cl_published): released=$fix1"
echo "2. wasm-split #79         issue $split_issue, crates.io $split_crate, $cl_tag locks $cl_locks: released=$fix2"

# 3. leptos #4953, against the latest leptos release.
lpr="$(get https://api.github.com/repos/leptos-rs/leptos/pulls/4953)"
l_merged="$(printf '%s' "$lpr" | field merged)"
l_merged_at="$(nonull "$(printf '%s' "$lpr" | field merged_at)")"
l_state="$(printf '%s' "$lpr" | field state)"
lrel="$(get https://api.github.com/repos/leptos-rs/leptos/releases/latest)"
l_tag="$(printf '%s' "$lrel" | field tag_name)"
l_published="$(printf '%s' "$lrel" | field published_at)"
fix3=no
if [ "$l_merged" = "true" ] && [[ "$l_published" > "$l_merged_at" ]]; then fix3=yes; fi
echo "3. leptos #4953           $l_state, merged=$l_merged${l_merged_at:+ ($l_merged_at)}, latest release $l_tag: released=$fix3"

# 4. htmx #4147: an issue, so closed is the signal; its outcome is read by hand.
h_state="$(get https://api.github.com/repos/bigskysoftware/htmx/issues/4147 | field state)"
h_tag="$(get https://api.github.com/repos/bigskysoftware/htmx/releases/latest | field tag_name)"
echo "4. htmx #4147             issue $h_state, latest release $h_tag"

# 5. leptos #4959: an issue, closed by the fix; released once a leptos
# release came out after it closed.
idle="$(get https://api.github.com/repos/leptos-rs/leptos/issues/4959)"
i_state="$(printf '%s' "$idle" | field state)"
i_closed_at="$(nonull "$(printf '%s' "$idle" | field closed_at)")"
fix5=no
if [ "$i_state" = "closed" ] && [[ "$l_published" > "$i_closed_at" ]]; then fix5=yes; fi
echo "5. leptos #4959           issue $i_state${i_closed_at:+ ($i_closed_at)}, latest release $l_tag: released=$fix5"
echo

if [ "$fix1" = yes ] && [ "$fix2" = yes ]; then
  act=1
  cat <<EOF
act: cargo-leptos $cl_tag has both split fixes. Test the plain release, installed
     into a scratch folder, with the two-build check:
  cargo install --locked --root /tmp/cargo-leptos-plain cargo-leptos@${cl_tag#v} &&
  cp target/release/hash.txt /tmp/hash.txt.saved 2>/dev/null; for run in 1 2; do PATH=/tmp/cargo-leptos-plain/bin:\$PATH LEPTOS_HASH_FILES=true LEPTOS_SITE_ROOT=target/site-check cargo leptos build --release --split --frontend-only >/dev/null && ls target/site-check/pkg | sort > /tmp/names-\$run.txt; done &&
  diff /tmp/names-1.txt /tmp/names-2.txt && grep -oh '__wasm_split\.[A-Za-z0-9_-]*\.js' target/site-check/pkg/app.*.js | head -1 | xargs -I{} test -f target/site-check/pkg/{} && echo "PLAIN $cl_tag PASSES";
  cp /tmp/hash.txt.saved target/release/hash.txt 2>/dev/null; rm -rf target/site-check
     If it passes: "Way back" for the patched cargo-leptos in docs/decisions.md.
EOF
fi
if [ "$fix3" = yes ]; then
  act=1
  echo "act: leptos $l_tag disposes islands (#4953): the island collector can give way to \$\$dispose (docs/decisions.md)."
fi
if [ "$fix5" = yes ]; then
  act=1
  echo "act: leptos $l_tag came out after #4959 closed: read how it was fixed; if its islands loader passes a timeout, drop the fork pin in Cargo.toml (docs/decisions.md)."
fi
if [ "$h_state" = "closed" ]; then
  act=1
  echo "act: htmx #4147 is closed: read how, then decide whether the parseHTMLUnsafe workaround stays (docs/decisions.md)."
fi

if [ "$act" = 0 ]; then
  echo "Nothing released yet: every workaround stays."
  exit 0
fi
exit 2
