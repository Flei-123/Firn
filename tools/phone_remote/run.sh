#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/phone_remote/run.sh -- examples/phone_remote end to end.
#
#   1. the example itself builds (as it is, uinput backend)
#   2. a shadow copy with tools/input/record_backend.fi as input/backend.fi
#      and a free port instead of 8080 is driven by a real Chromium with a
#      phone's touch screen (tools/phone_remote/check.cjs)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$FIRNC" -o "$W/phone_remote" examples/phone_remote/main.fi 2> "$W/b1.log" || { cat "$W/b1.log"; exit 1; }
echo "   examples/phone_remote builds ($(wc -l < examples/phone_remote/main.fi) lines of Firn, $(wc -l < examples/phone_remote/index.html) of HTML)"
PORT=$((21000 + RANDOM % 900))
mkdir -p "$W/shadow/input"
cp examples/phone_remote/index.html "$W/shadow/"
sed "s/http.app(8080)/http.app($PORT)/" examples/phone_remote/main.fi > "$W/shadow/main.fi"
cp tools/input/record_backend.fi "$W/shadow/input/backend.fi"
"$FIRNC" -o "$W/pr" "$W/shadow/main.fi" 2> "$W/b2.log" || { cat "$W/b2.log"; exit 1; }
PW="${PLAYWRIGHT:-}"
if [ -z "$PW" ]; then
    for d in /root/jarvis/node_modules/playwright "$(npm root -g 2>/dev/null)/playwright"; do
        [ -d "$d" ] && PW="$d" && break
    done
fi
if [ -z "$PW" ]; then
    echo "SKIP: Playwright not found (PLAYWRIGHT=<path>)"
    exit 0
fi
PLAYWRIGHT="$PW" timeout 120 node tools/phone_remote/check.cjs "$W/pr" "$PORT"
