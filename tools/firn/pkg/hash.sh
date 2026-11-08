#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

pkg_file_hash() {
    local f=$1
    if [ -L "$f" ]; then
        printf 'link:%s' "$(readlink "$f")" | sha256sum | cut -d' ' -f1
    else
        sha256sum < "$f" | cut -d' ' -f1
    fi
}

pkg_tree_hash() {
    local dir=$1 f
    (
        cd "$dir" || exit 1
        find . \( -type f -o -type l \) ! -path './.git/*' ! -path './.firn/*' -print | LC_ALL=C sort | while IFS= read -r f; do
            printf '%s  %s\n' "$(pkg_file_hash "$f")" "$f"
        done | sha256sum | cut -d' ' -f1
    )
}
