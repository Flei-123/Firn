# SPDX-License-Identifier: MPL-2.0
fui_info() {
    echo "fui: $*"
}

fui_die() {
    local code=$1
    shift
    echo "fui: error: $*" >&2
    exit "$code"
}
