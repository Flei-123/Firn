# SPDX-License-Identifier: MPL-2.0
FUI_ROOT=$(cd "$FUI_CLI/../../.." && pwd)
ROOT=$FUI_ROOT
export FIRNLIB="${FIRNLIB:-$FUI_ROOT/lib}"
FIRNC=${FIRNC:-}

fui_require_firnc() {
    local candidate
    for candidate in "$FIRNC" "$FUI_ROOT/compiler/target/release/firnc" "$(command -v firnc || true)"; do
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            FIRNC=$candidate
            return 0
        fi
    done
    fui_die 4 "firnc not found (set FIRNC or build compiler/target/release/firnc)"
}

fui_registered_targets() {
    local f
    for f in "$FUI_CLI"/targets/*.sh; do
        basename "$f" .sh
    done
}

fui_is_target() {
    [ -f "$FUI_CLI/targets/$1.sh" ]
}
