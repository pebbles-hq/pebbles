#!/usr/bin/env bash
# The counter example's wasm size (10 §12.2): the raw cargo output, no trunk or wasm-bindgen.
#
#   scripts/wasm-size.sh [--features "<cargo feature args>"] [--report-only | --check]
#
# Prints `raw=<n> gzip=<n> brotli=<n|n/a>`. `--check` fails when gzip exceeds the one integer in
# scripts/wasm-size-budget.txt. The default feature args build the shipped web configuration
# (WebGPU with the WebGL2 fallback, the umbrella's default `webgl` feature).
set -euo pipefail
cd "$(dirname "$0")/.."

features=""
mode="report"
while [ $# -gt 0 ]; do
  case "$1" in
    --features) features="$2"; shift 2 ;;
    --report-only) mode="report"; shift ;;
    --check) mode="check"; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# shellcheck disable=SC2086 # $features is a list of cargo arguments
cargo build --release -p counter --target wasm32-unknown-unknown $features
wasm="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/counter.wasm"
raw=$(wc -c < "$wasm")
gz=$(gzip -9 -c "$wasm" | wc -c)
if command -v brotli > /dev/null; then br=$(brotli -q 11 -c "$wasm" | wc -c); else br="n/a"; fi
echo "raw=$raw gzip=$gz brotli=$br"

if [ "$mode" = "check" ]; then
  budget=$(tr -d '[:space:]' < scripts/wasm-size-budget.txt)
  if [ "$gz" -gt "$budget" ]; then
    echo "counter.wasm gzip $gz bytes exceeds the budget of $budget bytes (scripts/wasm-size-budget.txt)" >&2
    exit 1
  fi
  echo "within budget: gzip $gz <= $budget"
fi
