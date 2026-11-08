#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/screenapps.sh -- THE fUi SCREEN APPS (examples/fui_apps/*) AS WEBASSEMBLY, BUILT AND PROVEN.
#
#   bash tools/wasm/screenapps.sh [app ...]      (default: flei settings notes)
#
# Per app: wasm32-browser build of the SAME main.fi the desktop runs, a page with Inter (4 weights) and a
# monospace face as extra fonts and the app's assets (examples/fui_apps/data), a headless Chromium
# screenshot at 1100x700 and a comparison with the native --shot of the same app (light and dark; the better one counts).
# Output: $OUT/<app>.wasm, <app>.html, <app>-web.png, <app>-native.png; the line "RESULT app ..." per app.
# The page, server and shot steps live in tools/wasm/weblib.sh (shared with `fui build`).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
export FIRN_PRUNE_FIR=1
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
. "$ROOT/tools/wasm/weblib.sh"
APPS=("$@")
[ ${#APPS[@]} -gt 0 ] || APPS=(flei settings notes)
OWN=0
if [ -z "${OUT:-}" ]; then
    OUT=$(mktemp -d "${TMPDIR:-/tmp}/screenapps.XXXXXX")
    OWN=1
fi
mkdir -p "$OUT/site"
cleanup() {
    web_serve_stop
    [ "$OWN" = 1 ] && [ -z "${KEEP:-}" ] && rm -rf "$OUT"
}
trap cleanup EXIT
LIM=${LIM:-0.5}
fail=0
web_site_prepare "$OUT/site" examples/fui_apps/data
for app in "${APPS[@]}"; do
    echo "== $app =="
    if ! web_compile "$OUT/site/$app.wasm" release-safe "examples/fui_apps/$app/main.fi"; then
        echo "RESULT $app BUILD-FAILED"; fail=1; continue
    fi
    web_page_write "$OUT/site/$app.html" "$app" "$app.wasm"
    "$FIRNC" --opt-level=dev -o "$OUT/native-$app" "examples/fui_apps/$app/main.fi" || { echo "RESULT $app NATIVE-FAILED"; fail=1; continue; }
    for th in light dark; do
        "$OUT/native-$app" --theme $th --size 1100x700 --shot "$OUT/$app-native-$th.png" >/dev/null
    done
done
web_serve_start "$OUT/site"
for app in "${APPS[@]}"; do
    [ -f "$OUT/site/$app.wasm" ] || continue
    web_shot "$app.html" "$OUT/$app-web.png" 1100 700 "$OUT/$app-chrome.log"
    python3 -I tools/wasm/screencmp.py "$LIM" "$app" "$OUT/$app-web.png" "$OUT/$app-native-light.png" "$OUT/$app-native-dark.png" || fail=1
done
exit $fail
