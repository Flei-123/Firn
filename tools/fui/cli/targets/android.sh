# SPDX-License-Identifier: MPL-2.0
android_available() {
    ANDROID_CHECK=1 bash "$FUI_ROOT/tools/android/build.sh" "$FUI_ROOT/examples/hello.fi"
}

android_build() {
    local dir=$OUT/android
    local abi_args=()
    [ -z "$ANDROID_ABI" ] || abi_args=(--abi "$ANDROID_ABI")
    mkdir -p "$dir"
    FIRNC="$FIRNC" APP_MAIN="$APP_START" ASSETS_DIR="$APP_DATA_DIR" APP_ID="$APP_ANDROID_ID" APP_TITLE="$APP_TITLE" OUTDIR="$dir" \
        bash "$FUI_ROOT/tools/android/screenapp.sh" "$APP_PACKAGE" ${abi_args[@]+"${abi_args[@]}"} || return 1
    fui_info "android: $dir"
}
