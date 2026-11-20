#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB=$PWD/lib
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-book.XXXXXX")
trap 'rm -rf "$W"' EXIT
PASS=0
FAIL=0

report_fail() {
    FAIL=$((FAIL + 1))
    printf '  FAIL %s (%s)\n' "$1" "$2"
    head -5 "$W/err" | sed 's/^/       /'
}

while IFS=$'\t' read -r mode source origin; do
    base=${source%.fi}
    case $mode in
    run)
        if ! "$FIRNC" "$source" -o "$base.bin" > "$W/err" 2>&1; then report_fail "$origin" "does not compile"; continue; fi
        "$base.bin" > "$base.out" 2> "$W/err"
        if [ -f "$base.expected" ] && ! diff -q "$base.expected" "$base.out" > /dev/null; then
            diff "$base.expected" "$base.out" > "$W/err"
            report_fail "$origin" "output differs"
            continue
        fi ;;
    build)
        if ! "$FIRNC" "$source" -o "$base.bin" > "$W/err" 2>&1; then report_fail "$origin" "does not compile"; continue; fi ;;
    test)
        if ! "$FIRNC" --test --format=tap "$source" > "$W/err" 2>&1 || grep -q '^not ok' "$W/err"; then report_fail "$origin" "test fails"; continue; fi ;;
    esac
    PASS=$((PASS + 1))
    printf '  ok   %s (%s)\n' "$origin" "$mode"
done < <(python3 -I tools/book/extract.py docs/BOOK "$W")

echo "book: $PASS ok, $FAIL errors"
[ "$FAIL" -eq 0 ] && [ "$PASS" -gt 0 ]
