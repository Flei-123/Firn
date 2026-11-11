#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0

declare -A SEEN_SOURCE
declare -A SEEN_BY
declare -A SEEN_DIR
declare -A SEEN_VERSION
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
    local name=$1 desc=$2 by=$3 dir=$4 version=$5
    if [ -z "${SEEN_SOURCE[$name]:-}" ]; then
        SEEN_SOURCE[$name]=$desc
        SEEN_BY[$name]=$by
        SEEN_DIR[$name]=$dir
        SEEN_VERSION[$name]=$version
        return 0
    fi
    if [ "${SEEN_SOURCE[$name]}" != "$desc" ]; then
        [ "${SEEN_VERSION[$name]}" != "$version" ] && return 0
        die "conflict: dependency '$name' $version resolves to '${SEEN_SOURCE[$name]}' (via ${SEEN_BY[$name]:-the project}) and to '$desc' (via ${by:-the project}); give them different versions or use one source"
    fi
    [ "${SEEN_DIR[$name]}" = "$dir" ] && return 0
    case $desc in git*) return 0 ;; esac
    die "dependency '$name' is checked out twice (via ${SEEN_BY[$name]:-the project} and via ${by:-the project}); two path directories with the same version are two packages: point both needs at the same path"
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
        dep_mark "$name" "$want"
        say "have    $label ${rev:0:12}"
        return 0
    fi
    dep_install_or_die "$name" "$url" "$rev"
    got=$(pkg_tree_hash "$DEPS_DIR/$name")
    if [ "$got" != "$want" ]; then
        rm -rf "${DEPS_DIR:?}/$name"
        die "checksum mismatch for $label at ${rev:0:12}: expected $want, got $got"
    fi
    dep_mark "$name" "$want"
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
        register_source "$name" "$(source_descriptor "$name" "$path")" "${label%/}" "$(cd "$path" && pwd -P)" "$(dep_version "$path")"
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
    SEEN_VERSION=()
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
    if [ -n "$got" ] && [ "$got" = "$hash" ]; then
        if [ "$(cat "$DEPS_DIR/$name/.firn/pin" 2>/dev/null)" != "$hash" ]; then
            echo "FAILED  $label (pin marker missing or stale: run firn fetch)"
            return 1
        fi
        echo "ok      $label"
        return 0
    fi
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

TREE_FIELD_SEP=$'\x1f'
NO_PIN=-

tree_pin() {
    local pin
    pin=$(head -n 1 "$1/.firn/pin" 2>/dev/null)
    printf '%s' "${pin:-$NO_PIN}"
}

tree_row() {
    local IFS=$TREE_FIELD_SEP
    TREE_ROWS+=("$*")
}

walk_tree() {
    local indent=$1 name path ver here desc
    while read -r name path ver; do
        desc=$(describe_source "$name" "$path")
        if [ ! -d "$path" ]; then
            tree_row "$indent" "$name" "" "" "$NO_PIN" "$desc" missing
            continue
        fi
        here=$PWD
        cd "$path" || exit 125
        ver=$(dep_version .)
        case $VISITED in
        *"|$(pwd -P)|"*)
            tree_row "$indent" "$name" "$ver" "$(pwd -P)" "$(tree_pin .)" "$desc" seen
            ;;
        *)
            tree_row "$indent" "$name" "$ver" "$(pwd -P)" "$(tree_pin .)" "$desc" walk
            VISITED="$VISITED|$(pwd -P)|"
            walk_tree "$indent  "
            ;;
        esac
        cd "$here" || exit 125
    done < <(needs_lines)
}

tree_choose() {
    local name=$1 best="" n ver real pin entry
    for entry in "${TREE_COPIES[@]}"; do
        IFS=$TREE_FIELD_SEP read -r n ver real pin <<< "$entry"
        [ "$n" = "$name" ] || continue
        [ -z "$best" ] || [ "$(printf '%s\n%s\n' "$best" "$ver" | sort -V | tail -n 1)" = "$ver" ] && best=$ver
    done
    TREE_BEST[$name]=$best
    local first="" same=1
    for entry in "${TREE_COPIES[@]}"; do
        IFS=$TREE_FIELD_SEP read -r n ver real pin <<< "$entry"
        [ "$n" = "$name" ] && [ "$ver" = "$best" ] || continue
        if [ -z "$first" ]; then
            first=$pin
            TREE_WINNER[$name]=$real
        else
            [ "$pin" = "$first" ] && [ "$first" != "$NO_PIN" ] || same=0
            [ "$real" \< "${TREE_WINNER[$name]}" ] && TREE_WINNER[$name]=$real
        fi
    done
    [ $same = 1 ] || TREE_WINNER[$name]=
}

tree_mark() {
    local name=$1 ver=$2 real=$3
    [ "${TREE_DIRS[$name]:-0}" -gt 1 ] || return 0
    if [ "$ver" != "${TREE_BEST[$name]}" ]; then
        printf ' [not used: %s wins]' "${TREE_BEST[$name]}"
    elif [ -z "${TREE_WINNER[$name]}" ]; then
        printf ' [conflict: same version, different content]'
    elif [ "$real" = "${TREE_WINNER[$name]}" ]; then
        printf ' [used]'
    else
        printf ' [not used: same package as the used copy]'
    fi
}

tree_prepare() {
    local row indent name ver real pin desc flag seen=""
    TREE_COPIES=()
    declare -gA TREE_DIRS=() TREE_BEST=() TREE_WINNER=()
    for row in "${TREE_ROWS[@]}"; do
        IFS=$TREE_FIELD_SEP read -r indent name ver real pin desc flag <<< "$row"
        [ "$flag" != missing ] || continue
        case $seen in *"$TREE_FIELD_SEP$name$TREE_FIELD_SEP$real$TREE_FIELD_SEP"*) continue ;; esac
        seen="$seen$TREE_FIELD_SEP$name$TREE_FIELD_SEP$real$TREE_FIELD_SEP"
        TREE_DIRS[$name]=$(( ${TREE_DIRS[$name]:-0} + 1 ))
        TREE_COPIES+=("$name$TREE_FIELD_SEP$ver$TREE_FIELD_SEP$real$TREE_FIELD_SEP$pin")
    done
    for name in "${!TREE_DIRS[@]}"; do
        [ "${TREE_DIRS[$name]}" -gt 1 ] && tree_choose "$name"
    done
}

tree_print() {
    local row indent name ver real pin desc flag
    for row in "${TREE_ROWS[@]}"; do
        IFS=$TREE_FIELD_SEP read -r indent name ver real pin desc flag <<< "$row"
        if [ "$flag" = missing ]; then
            printf '%s%s (%s) [missing: run firn fetch]\n' "$indent" "$name" "$desc"
            continue
        fi
        printf '%s%s %s (%s)%s\n' "$indent" "$name" "$ver" "$desc" "$(tree_mark "$name" "$ver" "$real")"
        [ "$flag" != seen ] || printf '%s  (*)\n' "$indent"
    done
}

cmd_tree() {
    [ -f firn.package ] || die "no ./firn.package here"
    VISITED="|$(pwd -P)|"
    TREE_ROWS=()
    printf '%s %s\n' "$(pkg_name)" "$(dep_version .)"
    walk_tree "  "
    tree_prepare
    tree_print
}

pkg_ensure() {
    [ -f "$1/firn.package" ] || return 0
    (cd "$1" && cmd_fetch --quiet) || exit 125
}
