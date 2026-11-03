#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
TREE=${1:?usage: portable_lib.sh EXTRACT-DIR REPO-ROOT}
REPO=${2:?usage: portable_lib.sh EXTRACT-DIR REPO-ROOT}
TREE=$(cd "$TREE" && pwd)
LIB=$TREE/lib
fixed=0
while IFS= read -r -d '' link; do
    target=$(readlink "$link")
    parent=$(dirname "$link")
    case "$target" in
        /*) abs=$target ;;
        *) abs=$(realpath -m "$parent/$target") ;;
    esac
    case "$abs" in "$LIB"/*) continue ;; esac
    rel_parent=${parent#"$TREE"/}
    case "$abs" in
        "$TREE"/*) src=$REPO/${abs#"$TREE"/} ;;
        */lib/*) src=$abs ;;
        *) src= ;;
    esac
    rm -f "$link"
    if [ -n "$src" ] && [ "lib/${src##*/lib/}" = "$rel_parent" ]; then
        :
    elif [ -n "$src" ] && [ -f "$src" ]; then
        cp "$src" "$link"
    else
        echo "portable_lib: dropped link ${link#"$TREE"/} -> $target" >&2
    fi
    fixed=$((fixed + 1))
done < <(find "$LIB" -type l -print0)
echo "portable_lib: $fixed links made portable" >&2
