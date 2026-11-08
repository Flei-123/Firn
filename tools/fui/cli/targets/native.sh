# SPDX-License-Identifier: MPL-2.0
native_available() {
    return 0
}

native_build() {
    local dir=$OUT/native
    mkdir -p "$dir/data"
    "$FIRNC" --package "$APP_DIR" --opt-level="$APP_OPT" -o "$dir/$APP_PACKAGE" || return 1
    [ ! -d "$APP_DATA_DIR" ] || cp -r "$APP_DATA_DIR"/. "$dir/data/"
    fui_info "native: $dir/$APP_PACKAGE"
}
