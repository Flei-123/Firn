# SPDX-License-Identifier: MPL-2.0
. "$FUI_CLI/verbs/build.sh"

verb_verify() {
    build_parse_args "$@"
    fui_require_firnc
    manifest_load "$DIR"
    LIMIT=${LIMIT:-$APP_LIMIT}
    OUT=$(realpath -m "${OUT:-$APP_DIR/dist}")
    [ -x "$OUT/native/$APP_PACKAGE" ] && [ -f "$OUT/web/index.html" ] || fui_die 1 "no native+web build in $OUT (run: fui build --targets native,web)"
    verify_run
    fui_info "verified: $OUT"
}
