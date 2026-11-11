# SPDX-License-Identifier: MPL-2.0
. "$FUI_ROOT/tools/wasm/weblib.sh"

verify_native_shots() {
    local theme
    for theme in light dark; do
        FUI_DATA_DIR="$OUT/native/data" "$OUT/native/$APP_PACKAGE" --theme "$theme" --size "$APP_SIZE" \
            --shot "$OUT/verify/native-$theme.png" >/dev/null || fui_die 1 "native shot failed ($theme)"
    done
}

verify_check_tools() {
    command -v chromium >/dev/null || fui_die 4 "chromium not found (needed for --verify)"
    command -v python3 >/dev/null || fui_die 4 "python3 not found (needed for --verify)"
}

verify_run() {
    verify_check_tools
    mkdir -p "$OUT/verify"
    verify_native_shots
    web_serve_start "$OUT/web"
    web_shot index.html "$OUT/verify/web.png" "$APP_W" "$APP_H" "$OUT/verify/chrome.log"
    web_serve_stop
    python3 -I "$FUI_ROOT/tools/wasm/screencmp.py" "$LIMIT" "$APP_PACKAGE" "$OUT/verify/web.png" \
        "$OUT/verify/native-light.png" "$OUT/verify/native-dark.png" || fui_die 3 "pixel gate failed for $APP_PACKAGE"
}
