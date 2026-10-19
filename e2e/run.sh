#!/usr/bin/env bash
# The browser tests (docs/testing.md, "Browser tests"):
#
#   e2e/run.sh [cargo test's filter and flags]
#   e2e/run.sh low [cargo test's filter and flags]
#
# `low` runs the same tests in the low bar's Chrome, 114 (docs/testing.md,
# "The low bar"): Chrome for Testing and its driver from
# ~/Library/Caches/e2e-floor/, or E2E_BINARY and CHROMEDRIVER. Local only.
#
# Builds the site into target/e2e (its own target folder and site, so the
# watch loop's binary and target/site are never touched), starts it on port
# 5160 with a fresh database (LOCO_ENV=e2e, config/e2e.yaml) and
# chromedriver on 9515, runs e2e/tests in headless Chrome one at a time,
# and stops both whatever the tests did. The logs, and what a failed test's
# page showed, are in target/e2e/run/.
#
# CHROMEDRIVER names the driver, else the one in ~/Library/Caches/chromedriver
# (docs/testing.md), else `chromedriver` on the PATH; its version must match
# Chrome's. E2E_HEADED=1 shows the browser's window.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
export CARGO_TARGET_DIR="$root/target/e2e"
export LEPTOS_SITE_ROOT=target/e2e/site
export LOCO_ENV=e2e
site=5160
driver_port=9515
dir="$root/target/e2e/run"
cached="$HOME/Library/Caches/chromedriver/chromedriver-mac-arm64/chromedriver"
if [[ ${1:-} == low ]]; then
  shift
  floor="$HOME/Library/Caches/e2e-floor"
  export E2E_BINARY=${E2E_BINARY:-"$floor/chrome-114/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"}
  cached="$floor/chromedriver-114/chromedriver"
  if [[ ! -x $E2E_BINARY ]]; then
    echo "no low-bar Chrome at $E2E_BINARY (docs/testing.md, \"The low bar\")" >&2
    exit 1
  fi
else
  # A value left in the shell would pair the low bar's Chrome with the
  # current driver, which refuses it.
  unset E2E_BINARY
fi
if [[ -n ${CHROMEDRIVER:-} ]]; then
  chromedriver=$CHROMEDRIVER
elif [[ -x $cached ]]; then
  chromedriver=$cached
else
  chromedriver=chromedriver
fi

for port in "$site" "$driver_port"; do
  if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "port $port is taken: a run still going, or something else on it" >&2
    exit 1
  fi
done

# The patched cargo-leptos (CLAUDE.md, Islands), and `--split`: the real
# lazy path.
tools/cargo-leptos/check.sh
cargo leptos build --split
cargo test --manifest-path e2e/Cargo.toml --no-run

# A fresh folder: the database, the logs, the failures.
rm -rf "$dir"
mkdir -p "$dir"

pids=()
stop() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap stop EXIT

E2E_PORT=$site "$CARGO_TARGET_DIR/debug/app" start >"$dir/server.log" 2>&1 &
pids+=($!)
"$chromedriver" --port="$driver_port" >"$dir/driver.log" 2>&1 &
pids+=($!)

# The site and the driver up within a minute, or the run stops with their
# last words.
waited=0
until curl -sf "http://localhost:$site/_ping" >/dev/null &&
  curl -sf "http://localhost:$driver_port/status" >/dev/null; do
  if ((waited >= 60)); then
    echo "the site or the driver did not start; $dir/server.log and driver.log:" >&2
    tail -20 "$dir/server.log" "$dir/driver.log" >&2
    exit 1
  fi
  sleep 1
  waited=$((waited + 1))
done

export E2E_BASE="http://localhost:$site" E2E_DRIVER="http://localhost:$driver_port" E2E_DIR="$dir"
# One at a time: the memory tests measure with nothing else running.
cargo test --manifest-path e2e/Cargo.toml -- --test-threads=1 "$@"
