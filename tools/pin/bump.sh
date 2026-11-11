#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
FIRN_HOME=${FIRN_HOME:-$HOME/.firn}
REGISTRY=$HERE/repos.txt
VERSION=
SMOKE=1
COMMIT_CHANGES=0
ONLY=
declare -A PATH_OF=()
usage() { echo "usage: bump.sh VERSION [--repo NAME=PATH] [--only NAME,...] [--no-smoke] [--commit]" >&2; exit 2; }
while [ $# -gt 0 ]; do
    case "$1" in
        --repo) PATH_OF[${2%%=*}]=${2#*=}; shift 2 ;;
        --only) ONLY=,$2,; shift 2 ;;
        --no-smoke) SMOKE=0; shift ;;
        --commit) COMMIT_CHANGES=1; shift ;;
        -*) usage ;;
        *) [ -z "$VERSION" ] || usage; VERSION=$1; shift ;;
    esac
done
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
TOOLCHAIN=$FIRN_HOME/versions/$VERSION
if [ ! -x "$TOOLCHAIN/bin/firnc" ]; then
    firn install "$VERSION" || { echo "bump: cannot install firn $VERSION" >&2; exit 1; }
fi
SOURCE_COMMIT=$(cat "$TOOLCHAIN/COMMIT")
[ -n "$SOURCE_COMMIT" ] || { echo "bump: $TOOLCHAIN/COMMIT is empty" >&2; exit 1; }

write_pin() {
    local repo=$1 pin_dir=$2
    echo "firn $VERSION" > "$repo/$pin_dir/firn.toolchain"
    CHANGED+=("$pin_dir/firn.toolchain")
    if [ -f "$repo/vendor/firn/COMMIT" ]; then
        echo "$SOURCE_COMMIT" > "$repo/vendor/firn/COMMIT"
        CHANGED+=(vendor/firn/COMMIT)
    fi
    update_pin_doc "$repo"
}

update_pin_doc() {
    local doc=$1/docs/firn-pin.md
    [ -f "$doc" ] || return 0
    sed -i -E "s/^(Firn toolchain pin )[0-9]+\.[0-9]+\.[0-9]+ \([0-9a-f]+\)/\1$VERSION ($SOURCE_COMMIT)/" "$doc"
    CHANGED+=(docs/firn-pin.md)
}

commit_pin() {
    local repo=$1
    git -C "$repo" add -- "${CHANGED[@]}"
    if git -C "$repo" diff --cached --quiet; then
        echo "   already pinned"
        return 0
    fi
    git -C "$repo" commit -q -m "firn: pin $VERSION (${SOURCE_COMMIT:0:9})" -- "${CHANGED[@]}"
}

FAILED=()
while read -r name default_path pin_dir <&3; do
    case "$name" in ''|'#'*) continue ;; esac
    [ -z "$ONLY" ] || [[ "$ONLY" == *",$name,"* ]] || continue
    repo=${PATH_OF[$name]:-$default_path}
    echo "== $name ($repo)"
    [ -d "$repo" ] || { echo "   missing"; FAILED+=("$name:missing"); continue; }
    CHANGED=()
    write_pin "$repo" "$pin_dir"
    if [ $SMOKE = 1 ]; then
        if bash "$HERE/smoke/$name.sh" "$repo" "$VERSION" < /dev/null; then
            echo "   smoke ok"
        else
            echo "   smoke FAILED"
            FAILED+=("$name:smoke")
            continue
        fi
    fi
    [ $COMMIT_CHANGES = 1 ] && { commit_pin "$repo" || FAILED+=("$name:commit"); }
done 3< "$REGISTRY"
if [ ${#FAILED[@]} -gt 0 ]; then
    echo "bump $VERSION: FAILED ${FAILED[*]}"
    exit 1
fi
echo "bump $VERSION: ok"
