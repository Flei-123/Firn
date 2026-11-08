# SPDX-License-Identifier: MPL-2.0
verb_targets() {
    local t
    for t in $(fui_registered_targets); do
        . "$FUI_CLI/targets/$t.sh"
        if "${t}_available" >/dev/null 2>&1; then
            echo "$t"
        else
            echo "$t (unavailable)"
        fi
    done
}
