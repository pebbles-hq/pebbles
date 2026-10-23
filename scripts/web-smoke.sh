#!/usr/bin/env bash
# Headless-browser smoke of the web build on the WebGL2 path (05 V50, informative).
#
# Builds the counter example for the web, serves it, loads it in headless Chrome with
# `?pebbles-force-webgl=1` (08 P03) on SwiftShader, and checks the browser console for the
# adapter line (08 §14.1, `backend=gl`) and the absence of a panic. Needs Trunk and Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."

chrome="${CHROME:-$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)}"
[ -n "$chrome" ] || { echo "web-smoke: no Chrome/Chromium found (set CHROME=...)" >&2; exit 2; }
port="${PORT:-8080}"
log="${LOG:-chrome.log}"

cargo run -q -p pebbles-cli -- build -d web -p counter
python3 -m http.server "$port" --directory examples/counter/dist > /dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
sleep 1

"$chrome" --headless=new --no-sandbox --enable-logging=stderr --v=0 --use-angle=swiftshader \
  --enable-unsafe-swiftshader --virtual-time-budget=15000 \
  --user-data-dir="$(mktemp -d)" --dump-dom "http://localhost:$port/?pebbles-force-webgl=1" \
  > /dev/null 2> "$log" || true

grep -E 'pebbles: GPU: .*backend=gl' "$log" || { echo "web-smoke: no 'GPU: … backend=gl' line in $log" >&2; exit 1; }
if grep -q 'panicked' "$log"; then
  echo "web-smoke: the app panicked (see $log)" >&2
  exit 1
fi
echo "web-smoke: ok"
