#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

declare -A SEEN_SOURCE
declare -A SEEN_BY
declare -A SEEN_DIR
VISITED=
LOCKED=0
QUIET=0

say() {
    if [ "$QUIET" = 1 ]; then case $1 in have*) return 0 ;; esac; fi
    echo "$1"
}

needs_lines() { awk '$1 == "needs" { print $2, $3, $4 }' firn.package; }

pkg_name() { sed -n 's/^package[[:space:]]\{1,\}\([A-Za-z0-9_]*\).*/\1/p' firn.package | head -1; }

source_descriptor() {
    local name=$1 path=$2 row
    row=$(deps_row "$name")
    if [ -n "$row" ] && [ "$path" = "$DEPS_DIR/$name" ]; then
        set -- $row
        printf 'git %s %s' "$2" "$4"
    else
        printf 'path'
    fi
}

register_source() {
    local name=$1 desc=$2 by=$3 dir=$4
    if [ -z "${SEEN_SOURCE[$name]:-}" ]; then
        SEEN_SOURCE[$name]=$desc
        SEEN_BY[$name]=$by
        SEEN_DIR[$name]=$dir
        return 0
    fi
    if [ "${SEEN_SOURCE[$name]}" != "$desc" ]; then
        die "conflict: dependency '$name' resolves to '${SEEN_SOURCE[$name]}' (via ${SEEN_BY[$name]:-the project}) and to '$desc' (via ${by:-the project})"
    fi
    [ "${SEEN_DIR[$name]}" = "$dir" ] && return 0
    die "dependency '$name' is checked out twice (via ${SEEN_BY[$name]:-the project} and via ${by:-the project}); the compiler accepts one directory per package: point both needs at the same path"
}

dep_fetch_one() {
    local label=$1 name=$2 url=$3 ref=$4 rev=$5 want=$6 got
    if [ "$rev" = "$NO_VALUE" ] || [ "$want" = "$NO_VALUE" ]; then
        [ "$LOCKED" = 1 ] && die "$label is not pinned in $DEPS_FILE (run firn fetch without --locked)"
        dep_pin "$name" "$url" "$ref"
        say "pinned  $label ${PINNED_REV:0:12}"
        return 0
    fi
    if [ -d "$DEPS_DIR/$name" ] && [ "$(pkg_tree_hash "$DEPS_DIR/$name")" = "$want" ]; then
        say "have    $label ${rev:0:12}"
        return 0
    fi
    dep_install_or_die "$name" "$url" "$rev"
    got=$(pkg_tree_hash "$DEPS_DIR/$name")
    if [ "$got" != "$want" ]; then
        rm -rf "${DEPS_DIR:?}/$name"
        die "checksum mismatch for $label at ${rev:0:12}: expected $want, got $got"
    fi
    say "fetched $label ${rev:0:12}"
}

walk_fetch() {
    local label=$1 key name url ref rev hash path ver
    key=$(pwd -P)
    case $VISITED in *"|$key|"*) return 0 ;; esac
    VISITED="$VISITED|$key|"
    [ -f firn.package ] || die "$label has no firn.package"
    while read -r name url ref rev hash; do
        dep_fetch_one "$label$name" "$name" "$url" "$ref" "$rev" "$hash"
    done < <(deps_rows)
    while read -r name path ver; do
        [ -d "$path" ] || die "dependency '$name' of ${label:-the project} points to '$path', which does not exist"
        register_source "$name" "$(source_descriptor "$name" "$path")" "${label%/}" "$(cd "$path" && pwd -P)"
        local here=$PWD
        cd "$path" || die "cannot enter $path"
        walk_fetch "$label$name/"
        cd "$here" || exit 125
    done < <(needs_lines)
}

cmd_fetch() {
    LOCKED=0
    QUIET=0
    VISITED=
    SEEN_SOURCE=()
    SEEN_BY=()
    SEEN_DIR=()
    local a
    for a in "$@"; do
        case $a in
        --locked) LOCKED=1 ;;
        --offline) export FIRN_OFFLINE=1 ;;
        --quiet) QUIET=1 ;;
        *) die "usage: firn fetch [--locked] [--offline]" ;;
        esac
    done
    [ -f firn.package ] || die "no ./firn.package here"
    walk_fetch ""
}

verify_one() {
    local label=$1 name=$2 hash=$3 got=
    [ -d "$DEPS_DIR/$name" ] && got=$(pkg_tree_hash "$DEPS_DIR/$name")
    if [ -n "$got" ] && [ "$got" = "$hash" ]; then echo "ok      $label"; return 0; fi
    echo "FAILED  $label (expected $hash, got ${got:-missing})"
    return 1
}

walk_verify() {
    local label=$1 key name url ref rev hash path ver here
    key=$(pwd -P)
    case $VISITED in *"|$key|"*) return 0 ;; esac
    VISITED="$VISITED|$key|"
    while read -r name url ref rev hash; do
        verify_one "$label$name" "$name" "$hash" || VERIFY_BAD=1
    done < <(deps_rows)
    while read -r name path ver; do
        [ -d "$path" ] || continue
        here=$PWD
        cd "$path" || exit 125
        walk_verify "$label$name/"
        cd "$here" || exit 125
    done < <(needs_lines)
}

cmd_verify() {
    VISITED=
    VERIFY_BAD=0
    [ -f firn.package ] || die "no ./firn.package here"
    walk_verify ""
    [ "$VERIFY_BAD" = 0 ]
}

describe_source() {
    local name=$1 path=$2 row
    row=$(deps_row "$name")
    if [ -n "$row" ] && [ "$path" = "$DEPS_DIR/$name" ]; then
        set -- $row
        printf 'git %s@%s' "$2" "${4:0:12}"
    else
        printf 'path %s' "$path"
    fi
}

walk_tree() {
    local indent=$1 name path ver here desc
    while read -r name path ver; do
        desc=$(describe_source "$name" "$path")
        if [ ! -d "$path" ]; then
            printf '%s%s (%s) [missing: run firn fetch]\n' "$indent" "$name" "$desc"
            continue
        fi
        here=$PWD
        cd "$path" || exit 125
        printf '%s%s %s (%s)\n' "$indent" "$name" "$(dep_version .)" "$desc"
        case $VISITED in
        *"|$(pwd -P)|"*) printf '%s  (*)\n' "$indent" ;;
        *) VISITED="$VISITED|$(pwd -P)|"; walk_tree "$indent  " ;;
        esac
        cd "$here" || exit 125
    done < <(needs_lines)
}

cmd_tree() {
    [ -f firn.package ] || die "no ./firn.package here"
    VISITED="|$(pwd -P)|"
    printf '%s %s\n' "$(pkg_name)" "$(dep_version .)"
    walk_tree "  "
}

pkg_ensure() {
    [ -f "$1/firn.package" ] || return 0
    (cd "$1" && cmd_fetch --quiet) || exit 125
}
