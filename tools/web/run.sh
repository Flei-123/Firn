#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/web/run.sh -- builds examples/dom for the browser and drives it in a
# headless Chromium (tools/web/dom_check.cjs). Needs chromium + node playwright.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=$(mktemp -d); SRV=
trap 'kill $SRV 2>/dev/null; rm -rf "$W"' EXIT
"$FIRNC" ${OPT:-} --target=wasm32-browser -o "$W/app.wasm" examples/dom/main.fi || exit 1
cp lib/web/dom.js examples/dom/index.html "$W/"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
python3 -m http.server -b 127.0.0.1 -d "$W" "$PORT" >/dev/null 2>&1 & SRV=$!
for i in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done
PW="${PLAYWRIGHT:-}"
if [ -z "$PW" ]; then
    for d in /root/jarvis/node_modules/playwright "$(npm root -g 2>/dev/null)/playwright"; do
        [ -d "$d" ] && PW="$d" && break
    done
fi
[ -z "$PW" ] && { echo "SKIP: Playwright not found (PLAYWRIGHT=<path>)"; exit 0; }
PLAYWRIGHT="$PW" timeout 120 node tools/web/dom_check.cjs "http://127.0.0.1:$PORT/index.html"
