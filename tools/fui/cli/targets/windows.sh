# SPDX-License-Identifier: MPL-2.0
windows_available() {
    command -v x86_64-w64-mingw32-ld >/dev/null || { echo "x86_64-w64-mingw32-ld not found (binutils-mingw-w64-x86-64)" >&2; return 1; }
}

windows_build() {
    local dir=$OUT/windows
    mkdir -p "$dir/data"
    "$FIRNC" --package "$APP_DIR" --target=x86_64-windows --win-subsystem=windows --opt-level="$APP_OPT" \
        -o "$dir/$APP_PACKAGE.exe" || return 1
    [ ! -d "$APP_DATA_DIR" ] || cp -r "$APP_DATA_DIR"/. "$dir/data/"
    fui_info "windows: $dir/$APP_PACKAGE.exe"
}
