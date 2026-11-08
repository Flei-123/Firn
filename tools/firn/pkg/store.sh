#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

PKG_CACHE=${FIRN_PKG_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/firn-pkg}

pkg_offline() { [ "${FIRN_OFFLINE:-0}" = 1 ]; }

store_dir() { printf '%s/git/%s.git' "$PKG_CACHE" "$(printf '%s' "$1" | sha256sum | cut -c1-16)"; }

store_sync() {
    local url=$1 mirror
    mirror=$(store_dir "$url")
    if [ ! -d "$mirror" ]; then
        pkg_offline && return 1
        mkdir -p "$PKG_CACHE/git"
        if ! git clone -q --bare "$url" "$mirror.tmp.$$" 2> /dev/null; then
            rm -rf "$mirror.tmp.$$"
            return 1
        fi
        mv "$mirror.tmp.$$" "$mirror"
        return 0
    fi
    pkg_offline && return 0
    git --git-dir="$mirror" fetch -q origin '+refs/heads/*:refs/heads/*' '+refs/tags/*:refs/tags/*' 2> /dev/null
}

store_rev() {
    git --git-dir="$(store_dir "$1")" rev-parse --verify -q "${2:-HEAD}^{commit}" 2> /dev/null
}

store_has() {
    [ -d "$(store_dir "$1")" ] && git --git-dir="$(store_dir "$1")" cat-file -e "$2^{commit}" 2> /dev/null
}

store_checkout() {
    local url=$1 rev=$2 dest=$3 index rc
    index=$(mktemp -u "${TMPDIR:-/tmp}/firn-index.XXXXXX")
    rm -rf "$dest"
    mkdir -p "$dest"
    GIT_INDEX_FILE=$index git -c core.autocrlf=false --git-dir="$(store_dir "$url")" --work-tree="$dest" read-tree "$rev" \
        && GIT_INDEX_FILE=$index git -c core.autocrlf=false --git-dir="$(store_dir "$url")" --work-tree="$dest" checkout-index -a -f
    rc=$?
    rm -f "$index"
    return $rc
}
