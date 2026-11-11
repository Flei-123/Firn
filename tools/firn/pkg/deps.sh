#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

DEPS_FILE=firn.deps
DEPS_DIR=.firn/deps
NO_VALUE=-

is_git_src() { case $1 in *://*|git@*|*.git) return 0 ;; *) return 1 ;; esac; }

dep_version() { sed -n 's/^version[[:space:]]\{1,\}\([0-9][0-9.]*\).*/\1/p' "$1/firn.package" 2>/dev/null | head -1; }

deps_put() {
    local tmp=$DEPS_FILE.tmp.$$
    {
        [ -f "$DEPS_FILE" ] && awk -v n="$1" '$1 != n' "$DEPS_FILE"
        printf '%s %s %s %s %s\n' "$1" "$2" "${3:-$NO_VALUE}" "${4:-$NO_VALUE}" "${5:-$NO_VALUE}"
    } | LC_ALL=C sort > "$tmp"
    mv "$tmp" "$DEPS_FILE"
}

deps_drop() {
    [ -f "$DEPS_FILE" ] || return 0
    local tmp=$DEPS_FILE.tmp.$$
    awk -v n="$1" '$1 != n' "$DEPS_FILE" > "$tmp"
    if [ -s "$tmp" ]; then mv "$tmp" "$DEPS_FILE"; else rm -f "$tmp" "$DEPS_FILE"; fi
}

deps_rows() {
    [ -f "$DEPS_FILE" ] || return 0
    awk 'NF >= 2 { printf "%s %s %s %s %s\n", $1, $2, ($3 == "" ? "-" : $3), ($4 == "" ? "-" : $4), ($5 == "" ? "-" : $5) }' "$DEPS_FILE"
}

deps_row() { deps_rows | awk -v n="$1" '$1 == n { print; exit }'; }

dep_mark() {
    mkdir -p "$DEPS_DIR/$1/.firn" && printf '%s\n' "$2" > "$DEPS_DIR/$1/.firn/pin"
}

dep_install() {
    local name=$1 url=$2 rev=$3
    if ! store_has "$url" "$rev"; then
        store_sync "$url" || return 2
        store_has "$url" "$rev" || return 3
    fi
    store_checkout "$url" "$rev" "$DEPS_DIR/$name"
}

dep_install_or_die() {
    dep_install "$1" "$2" "$3"
    case $? in
    0) return 0 ;;
    2) die "cannot fetch $1 from $2$(pkg_offline && echo ' (offline, not in cache)')" ;;
    3) die "revision $3 of $1 not found in $2" ;;
    *) die "checkout of $1 failed" ;;
    esac
}

dep_pin() {
    local name=$1 url=$2 ref=$3 rev hash
    [ "$ref" = "$NO_VALUE" ] && ref=
    store_sync "$url" || die "cannot fetch $url$(pkg_offline && echo ' (offline, not in cache)')"
    rev=$(store_rev "$url" "$ref") || die "unknown ref '${ref:-HEAD}' in $url"
    dep_install_or_die "$name" "$url" "$rev"
    [ -f "$DEPS_DIR/$name/firn.package" ] || { rm -rf "${DEPS_DIR:?}/$name"; die "'$name' ($url) has no firn.package (not a Firn package)"; }
    hash=$(pkg_tree_hash "$DEPS_DIR/$name")
    dep_mark "$name" "$hash"
    deps_put "$name" "$url" "${ref:-$NO_VALUE}" "$rev" "$hash"
    PINNED_REV=$rev
    PINNED_HASH=$hash
}

cmd_add() {
    [ $# -ge 2 ] || die "usage: firn add NAME PATH|GITURL [REF]"
    local name=$1 src=$2 ref=${3:-} path ver
    [ -f firn.package ] || die "no ./firn.package here (run inside a project; firn new NAME)"
    case $name in [A-Za-z_]*) ;; *) die "dependency name must start with a letter or _" ;; esac
    grep -q "^needs[[:space:]]\{1,\}$name[[:space:]]" firn.package && die "'$name' is already a dependency (edit firn.package to change it)"
    if is_git_src "$src"; then
        dep_pin "$name" "$src" "${ref:-$NO_VALUE}"
        path=$DEPS_DIR/$name
        grep -qx '/.firn/' .gitignore 2>/dev/null || printf '/.firn/\n' >> .gitignore
        echo "pinned $name ${PINNED_REV:0:12} sha256:${PINNED_HASH:0:12}"
    else
        [ -f "$src/firn.package" ] || die "'$src' has no firn.package"
        path=$src
    fi
    ver=$(dep_version "$path")
    if [ -n "$ver" ]; then printf 'needs    %s %s %s\n' "$name" "$path" "$ver" >> firn.package
    else printf 'needs    %s %s\n' "$name" "$path" >> firn.package; fi
    echo "added needs $name $path${ver:+ $ver}"
    [ -f "$path/$DEPS_FILE" ] && cmd_fetch --quiet
    return 0
}

cmd_remove() {
    [ $# -eq 1 ] || die "usage: firn remove NAME"
    local name=$1 tmp=firn.package.tmp.$$
    [ -f firn.package ] || die "no ./firn.package here"
    awk -v n="$name" '$1 == "needs" && $2 == n { found = 1 } END { exit !found }' firn.package || die "'$name' is not a dependency"
    awk -v n="$name" '!($1 == "needs" && $2 == n)' firn.package > "$tmp" && mv "$tmp" firn.package
    deps_drop "$name"
    rm -rf "${DEPS_DIR:?}/$name"
    echo "removed $name"
}

cmd_update() {
    local only=${1:-} name url ref rev hash found=0
    [ -f "$DEPS_FILE" ] || die "no $DEPS_FILE -- nothing to update"
    [ -z "$only" ] || [ -n "$(deps_row "$only")" ] || die "'$only' is not a git dependency"
    while read -r name url ref rev hash; do
        [ -n "$only" ] && [ "$only" != "$name" ] && continue
        dep_pin "$name" "$url" "$ref"
        if [ "$PINNED_REV" = "$rev" ]; then echo "unchanged $name ${rev:0:12}"
        else echo "updated   $name ${rev:0:12} -> ${PINNED_REV:0:12}"; fi
    done < <(deps_rows)
    cmd_fetch --quiet
}

cmd_deps() {
    [ -f firn.package ] || die "no ./firn.package here"
    awk '$1 == "needs" { printf "%-16s %-28s %s\n", $2, $3, $4 }' firn.package
    local name url ref rev hash
    while read -r name url ref rev hash; do
        printf 'git  %-12s %s ref=%s rev=%s sha256=%s\n' "$name" "$url" "$ref" "${rev:0:12}" "${hash:0:12}"
    done < <(deps_rows)
    return 0
}
