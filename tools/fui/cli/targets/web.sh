# SPDX-License-Identifier: MPL-2.0
. "$FUI_ROOT/tools/wasm/weblib.sh"

web_available() {
    command -v python3 >/dev/null || { echo "python3 not found" >&2; return 1; }
}

web_build() {
    local site=$OUT/web
    web_site_prepare "$site" "$APP_DATA_DIR"
    web_compile "$site/$APP_PACKAGE.wasm" "$APP_OPT" --package "$APP_DIR" || return 1
    web_page_write "$site/index.html" "$APP_TITLE" "$APP_PACKAGE.wasm"
    fui_info "web: $site/index.html"
}
