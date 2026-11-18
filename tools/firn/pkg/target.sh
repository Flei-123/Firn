# SPDX-License-Identifier: MPL-2.0
TARGET_NAMES="web android windows linux"
FUI_CLI_BIN="$FIRN_ROOT/tools/fui/cli/fui"

target_expand() {
    case $1 in
        all) echo "${TARGET_NAMES// /,}" ;;
        *) echo "$1" ;;
    esac
}

target_check() {
    local t
    for t in ${1//,/ }; do
        case " $TARGET_NAMES native " in
            *" $t "*) ;;
            *) die "unknown target '$t' (known: $TARGET_NAMES all)" ;;
        esac
    done
}

cmd_build_target() {
    local args=() targets=""
    while [ $# -gt 0 ]; do
        case $1 in
            --target) targets=${2:?--target needs a value}; shift 2 ;;
            *) args+=("$1"); shift ;;
        esac
    done
    targets=$(target_expand "$targets")
    target_check "$targets"
    exec bash "$FUI_CLI_BIN" build ${args[@]+"${args[@]}"} --targets "$targets"
}

cmd_new_app() {
    bash "$FUI_CLI_BIN" new "$@" || return $?
    echo "build: firn build --target web|android|windows|linux|all   (output in dist/, see: firn targets)"
}

cmd_targets() {
    exec bash "$FUI_CLI_BIN" targets
}
