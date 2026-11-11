# SPDX-License-Identifier: MPL-2.0
. "$FUI_CLI/lib/manifest.sh"
. "$FUI_CLI/lib/verify.sh"

build_parse_args() {
    TARGETS=""
    OUT=""
    VERIFY=0
    LIMIT=
    OPT_OVERRIDE=""
    ANDROID_ABI=""
    DIR=.
    while [ $# -gt 0 ]; do
        case "$1" in
            --targets) TARGETS=${2:?--targets needs a value}; shift 2 ;;
            --out) OUT=${2:?--out needs a value}; shift 2 ;;
            --verify) VERIFY=1; shift ;;
            --limit) LIMIT=${2:?--limit needs a value}; shift 2 ;;
            --opt) OPT_OVERRIDE=${2:?--opt needs a value}; shift 2 ;;
            --abi) ANDROID_ABI=${2:?--abi needs a value}; shift 2 ;;
            -*) fui_die 2 "unknown option '$1'" ;;
            *) DIR=$1; shift ;;
        esac
    done
}

build_select_targets() {
    local t
    if [ -n "$TARGETS" ]; then
        APP_TARGETS=${TARGETS//,/ }
        for t in $APP_TARGETS; do
            fui_is_target "$t" || fui_die 2 "unknown target '$t' (known: $(fui_registered_targets | tr '\n' ' '))"
        done
    fi
    [ -z "$OPT_OVERRIDE" ] || APP_OPT=$OPT_OVERRIDE
    LIMIT=${LIMIT:-$APP_LIMIT}
    manifest_validate
    if [ "$VERIFY" = 1 ]; then
        [[ " $APP_TARGETS " == *" native "* && " $APP_TARGETS " == *" web "* ]] || fui_die 2 "--verify needs the targets native and web"
    fi
}

build_check_available() {
    local t
    [ "$VERIFY" = 0 ] || verify_check_tools
    for t in $APP_TARGETS; do
        . "$FUI_CLI/targets/$t.sh"
        "${t}_available" || fui_die 4 "target '$t' is unavailable on this machine"
    done
}

build_run_targets() {
    local t
    for t in $APP_TARGETS; do
        fui_info "building $APP_PACKAGE for $t"
        "${t}_build" || fui_die 1 "build failed for target '$t'"
    done
}

verb_build() {
    build_parse_args "$@"
    fui_require_firnc
    manifest_load "$DIR"
    build_select_targets
    OUT=$(realpath -m "${OUT:-$APP_DIR/dist}")
    build_check_available
    build_run_targets
    [ "$VERIFY" = 0 ] || verify_run
    fui_info "done: $OUT"
}
