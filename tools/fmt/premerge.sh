#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd -P)
REPO=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "premerge: not inside a git repository" >&2; exit 2; }
cd "$REPO" || exit 2

FIX=0
if [ "${1:-}" = "--fix" ]; then FIX=1; shift; fi
BASE=${1:-origin/main}
EXCLUDED_PREFIX=tests/lexneg/

pick_compiler() {
    local candidate
    for candidate in "${FIRNC:-}" "$ROOT/compiler/target/release/firnc" "$(command -v firnc || true)"; do
        [ -n "$candidate" ] && [ -x "$candidate" ] && { echo "$candidate"; return 0; }
    done
    return 1
}

build_formatter() {
    local out=$1 compiler
    compiler=$(pick_compiler) || { echo "premerge: no firnc found (set FIRNC)" >&2; return 1; }
    FIRNLIB="$ROOT/lib" "$compiler" -o "$out" "$ROOT/tools/fmt/firnfmt.fi" || {
        echo "premerge: cannot build tools/fmt/firnfmt.fi with $compiler" >&2
        return 1
    }
}

changed_sources() {
    local fork
    fork=$(git merge-base "$BASE" HEAD 2>/dev/null) || {
        echo "premerge: unknown base '$BASE'" >&2
        return 1
    }
    {
        git diff --name-only --diff-filter=ACMR "$fork"
        git ls-files --others --exclude-standard
    } | sort -u | while IFS= read -r path; do
        case $path in
        *.fi) ;;
        *) continue ;;
        esac
        case $path in "$EXCLUDED_PREFIX"*) continue ;; esac
        [ -f "$path" ] && [ ! -L "$path" ] && echo "$path"
    done
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

FORMATTER=${FIRNFMT:-}
if [ -z "$FORMATTER" ]; then
    FORMATTER=$WORK/firnfmt
    build_formatter "$FORMATTER" || exit 2
fi

changed_sources > "$WORK/files" || exit 2
count=$(wc -l < "$WORK/files")
if [ "$count" -eq 0 ]; then
    echo "premerge fmt: no changed .fi files vs $BASE"
    exit 0
fi

mapfile -t FILES < "$WORK/files"
if [ "$FIX" -eq 1 ]; then
    "$FORMATTER" -w "${FILES[@]}" || exit 1
    echo "premerge fmt: formatted $count files"
    exit 0
fi
"$FORMATTER" -c "${FILES[@]}" > "$WORK/out" 2> "$WORK/err"
status=$?
if [ "$status" -eq 0 ]; then
    echo "premerge fmt: $count changed .fi files are formatted"
    exit 0
fi

echo "premerge fmt: FAILED ($count files checked, base $BASE)"
cat "$WORK/out" "$WORK/err"
echo "fix: bash tools/fmt/premerge.sh --fix"
exit 1
