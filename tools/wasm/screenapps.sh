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
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
APPS=("$@")
[ ${#APPS[@]} -gt 0 ] || APPS=(flei settings notes)
OWN=0
if [ -z "${OUT:-}" ]; then
    OUT=$(mktemp -d "${TMPDIR:-/tmp}/screenapps.XXXXXX")
    OWN=1
fi
mkdir -p "$OUT/site"
SRV=""
cleanup() {
    [ -z "$SRV" ] || kill "$SRV" 2>/dev/null
    [ "$OWN" = 1 ] && [ -z "${KEEP:-}" ] && rm -rf "$OUT"
}
trap cleanup EXIT
LIM=${LIM:-0.5}
fail=0
cp demos/webdemo/firn.js "$OUT/site/"
cp lib/fui/fonts/Inter-*.ttf /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf "$OUT/site/"
cp examples/fui_apps/data/* "$OUT/site/"
ASSETS=$(cd examples/fui_apps/data && ls | tr '\n' ',' | sed 's/,$//')
FONTS="Inter-Regular.ttf,Inter-Medium.ttf,Inter-SemiBold.ttf,Inter-Bold.ttf,DejaVuSansMono.ttf"
for app in "${APPS[@]}"; do
    echo "== $app =="
    if ! "$FIRNC" --opt-level=release-safe --target=wasm32-browser -o "$OUT/site/$app.wasm" "examples/fui_apps/$app/main.fi"; then
        echo "RESULT $app BUILD-FAILED"; fail=1; continue
    fi
    cat > "$OUT/site/$app.html" <<HTML
<!doctype html>
<html><head><meta charset="utf-8"><title>$app</title>
<style>html,body{margin:0;background:#0e1014;overflow:hidden}canvas{display:block}</style></head>
<body><canvas id="firn" tabindex="0"></canvas>
<script src="firn.js" data-wasm="$app.wasm" data-font="DejaVuSans.ttf" data-fonts="$FONTS" data-assets="$ASSETS"></script>
</body></html>
HTML
    "$FIRNC" --opt-level=dev -o "$OUT/native-$app" "examples/fui_apps/$app/main.fi" || { echo "RESULT $app NATIVE-FAILED"; fail=1; continue; }
    for th in light dark; do
        "$OUT/native-$app" --theme $th --size 1100x700 --shot "$OUT/$app-native-$th.png" >/dev/null
    done
done
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
python3 -m http.server -d "$OUT/site" -b 127.0.0.1 "$PORT" >/dev/null 2>&1 &
SRV=$!
sleep 1
for app in "${APPS[@]}"; do
    [ -f "$OUT/site/$app.wasm" ] || continue
    chromium --headless=new --no-sandbox --disable-gpu --hide-scrollbars --force-color-profile=srgb \
        --window-size=1100,700 --virtual-time-budget=15000 --screenshot="$OUT/$app-web.png" \
        "http://127.0.0.1:$PORT/$app.html?w=1100&h=700" >"$OUT/$app-chrome.log" 2>&1
    python3 -I tools/wasm/screencmp.py "$LIM" "$app" "$OUT/$app-web.png" "$OUT/$app-native-light.png" "$OUT/$app-native-dark.png" || fail=1
done
exit $fail
