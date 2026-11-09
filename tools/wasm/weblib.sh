#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

WEB_PAGE_FONTS="Inter-Regular.ttf,Inter-Medium.ttf,Inter-SemiBold.ttf,Inter-Bold.ttf,DejaVuSansMono.ttf"
WEB_SYSTEM_FONT_DIR=/usr/share/fonts/truetype/dejavu
WEB_SERVER_PID=""
WEB_PORT=""
WEB_ASSETS=""

web_site_prepare() {
    local site=$1 data=$2
    mkdir -p "$site"
    cp "$ROOT/demos/webdemo/firn.js" "$site/"
    cp "$ROOT"/lib/fui/fonts/Inter-*.ttf "$WEB_SYSTEM_FONT_DIR/DejaVuSans.ttf" "$WEB_SYSTEM_FONT_DIR/DejaVuSansMono.ttf" "$site/"
    WEB_ASSETS=""
    [ -d "$data" ] || return 0
    local f
    for f in "$data"/*; do
        [ -f "$f" ] || continue
        cp "$f" "$site/"
        WEB_ASSETS="${WEB_ASSETS:+$WEB_ASSETS,}$(basename "$f")"
    done
}

web_compile() {
    local wasm=$1 opt=$2
    shift 2
    FIRN_PRUNE_FIR=1 "$FIRNC" --opt-level="$opt" --target=wasm32-browser -o "$wasm" "$@"
}

web_page_write() {
    local page=$1 title=$2 wasm_name=$3
    cat > "$page" <<HTML
<!doctype html>
<html><head><meta charset="utf-8"><title>$title</title>
<style>html,body{margin:0;background:#0e1014;overflow:hidden}canvas{display:block}</style></head>
<body><canvas id="firn" tabindex="0"></canvas>
<script src="firn.js" data-wasm="$wasm_name" data-font="DejaVuSans.ttf" data-fonts="$WEB_PAGE_FONTS" data-assets="$WEB_ASSETS"></script>
</body></html>
HTML
}

web_serve_start() {
    local site=$1
    WEB_PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
    python3 -m http.server -d "$site" -b 127.0.0.1 "$WEB_PORT" >/dev/null 2>&1 &
    WEB_SERVER_PID=$!
    sleep 1
}

web_serve_stop() {
    [ -z "$WEB_SERVER_PID" ] || kill "$WEB_SERVER_PID" 2>/dev/null
    WEB_SERVER_PID=""
}

web_shot() {
    local page=$1 out=$2 w=$3 h=$4 log=$5
    chromium --headless=new --no-sandbox --disable-gpu --hide-scrollbars --force-color-profile=srgb \
        --window-size="$w,$h" --virtual-time-budget=15000 --screenshot="$out" \
        "http://127.0.0.1:$WEB_PORT/$page?w=$w&h=$h" >"$log" 2>&1
}
