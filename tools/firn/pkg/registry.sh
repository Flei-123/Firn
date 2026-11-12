#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

REGISTRY_FILE_NAME=registry.index
SEMVER_PY=$HERE/pkg/semver.py

registry_location() { printf '%s' "${FIRN_REGISTRY:-}"; }

registry_local_copy() { printf '%s/%s' "$PKG_CACHE" "$REGISTRY_FILE_NAME"; }

registry_fetch() {
    local loc cache
    loc=$(registry_location)
    [ -n "$loc" ] || die "no registry configured (set FIRN_REGISTRY to an index file or URL)"
    cache=$(registry_local_copy)
    mkdir -p "$PKG_CACHE"
    case $loc in
    http://*|https://*)
        if pkg_offline; then [ -f "$cache" ] || die "registry not cached (offline)"
        else curl -fsSL "$loc" -o "$cache.tmp.$$" && mv "$cache.tmp.$$" "$cache" || { rm -f "$cache.tmp.$$"; die "cannot fetch registry $loc"; }; fi
        printf '%s' "$cache" ;;
    file://*) printf '%s' "${loc#file://}" ;;
    *) [ -f "$loc" ] || die "registry index not found: $loc"; printf '%s' "$loc" ;;
    esac
}

registry_rows() { awk 'NF >= 5 && $1 !~ /^#/' "$1"; }

registry_versions() {
    registry_rows "$1" | awk -v n="$2" '$1 == n && $6 != "yanked" { print $2 }'
}

registry_resolve() {
    local index=$1 name=$2 req=$3 versions version
    versions=$(registry_versions "$index" "$name")
    [ -n "$versions" ] || die "package '$name' is not in the registry (or all versions are yanked)"
    version=$(printf '%s\n' "$versions" | python3 -I "$SEMVER_PY" resolve "$req") || {
        local rc=$?
        [ $rc -eq 1 ] && die "no version of '$name' satisfies '$req' (available: $(printf '%s' "$versions" | sort -V | tr '\n' ' '))"
        die "bad version requirement '$req'"
    }
    registry_rows "$index" | awk -v n="$name" -v v="$version" '$1 == n && $2 == v && $6 != "yanked" { print; exit }'
}

registry_pin() {
    local name=$1 req=$2 index row version url rev hash
    index=$(registry_fetch)
    row=$(registry_resolve "$index" "$name" "$req")
    read -r _ version url rev hash _ <<< "$row"
    dep_pin "$name" "$url" "$rev"
    if [ "$PINNED_HASH" != "$hash" ]; then
        deps_drop "$name"
        rm -rf "${DEPS_DIR:?}/$name"
        die "checksum mismatch for $name $version: registry says ${hash:0:12}, checkout is ${PINNED_HASH:0:12}"
    fi
    deps_put "$name" "$url" "@$req" "$rev" "$hash"
    REGISTRY_VERSION=$version
}

cmd_search() {
    local index
    index=$(registry_fetch)
    registry_rows "$index" | awk -v q="${1:-}" '$6 != "yanked" && index($1, q) { last[$1] = $2; count[$1]++ } END { for (n in last) printf "%-20s %s (%d versions)\n", n, last[n], count[n] }' | LC_ALL=C sort
}

registry_index_line() {
    local dir=${1:-.} name version url rev hash
    [ -f "$dir/firn.package" ] || die "no firn.package in $dir"
    name=$(sed -n 's/^package[[:space:]]\{1,\}\([A-Za-z0-9_]*\).*/\1/p' "$dir/firn.package" | head -1)
    version=$(dep_version "$dir")
    [ -n "$name" ] && [ -n "$version" ] || die "firn.package needs 'package' and 'version'"
    git -C "$dir" rev-parse --git-dir > /dev/null 2>&1 || die "$dir is not a git checkout"
    [ -z "$(git -C "$dir" status --porcelain)" ] || die "working tree not clean: commit first"
    url=${FIRN_PUBLISH_URL:-$(git -C "$dir" remote get-url origin 2> /dev/null)}
    [ -n "$url" ] || die "no git remote 'origin' (or set FIRN_PUBLISH_URL)"
    rev=$(git -C "$dir" rev-parse HEAD)
    hash=$(pkg_tree_hash "$dir")
    printf '%s %s %s %s %s\n' "$name" "$version" "$url" "$rev" "$hash"
}

cmd_publish() {
    local line loc name version tmp
    line=$(registry_index_line "${1:-.}")
    loc=$(registry_location)
    [ -n "$loc" ] || { printf '%s\n' "$line"; return 0; }
    case $loc in http://*|https://*) printf '%s\n' "$line"; echo "remote registry: send this line to the index maintainer" >&2; return 0 ;; esac
    loc=${loc#file://}
    read -r name version _ <<< "$line"
    [ -f "$loc" ] && registry_rows "$loc" | awk -v n="$name" -v v="$version" '$1 == n && $2 == v { f = 1 } END { exit !f }' && die "$name $version is already published"
    tmp=$loc.tmp.$$
    { [ -f "$loc" ] && cat "$loc"; printf '%s\n' "$line"; } | LC_ALL=C sort -s -k1,1 > "$tmp" && mv "$tmp" "$loc"
    echo "published $name $version"
}

cmd_yank() {
    [ $# -eq 2 ] || die "usage: firn yank NAME VERSION"
    local loc tmp
    loc=$(registry_location)
    loc=${loc#file://}
    [ -f "$loc" ] || die "yank needs FIRN_REGISTRY to be a local index file"
    registry_rows "$loc" | awk -v n="$1" -v v="$2" '$1 == n && $2 == v { f = 1 } END { exit !f }' || die "$1 $2 is not in the registry"
    tmp=$loc.tmp.$$
    awk -v n="$1" -v v="$2" '$1 == n && $2 == v && $6 != "yanked" { print $1, $2, $3, $4, $5, "yanked"; next } { print }' "$loc" > "$tmp" && mv "$tmp" "$loc"
    echo "yanked $1 $2"
}
