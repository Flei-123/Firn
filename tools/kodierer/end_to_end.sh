#!/usr/bin/env bash
# ROUND KODIERER -- the test of the whole.
#
# The octet-by-octet counter check (compare.py) says: the object file is the
# same. This test says: the PROGRAM behaves the same too. Every test case is
# built TWICE -- once through `as`, once through the own encoder -- and run
# both times. The comparison is against the OLD way, not against the
# expectation in the head of the file: cases that were red before do not count
# against the encoder that way.
#
# Usage: bash tools/kodierer/end_to_end.sh [pattern]
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
PATTERN="${1:-tests/*.fi}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT

same=0
differ=0
old_fail=0
new_fail=0

run() { # $1 = program -> "rc|output"
    local out rc
    out=$(timeout 60 "$1" 2>&1)
    rc=$?
    printf '%s|%s' "$rc" "$out"
}

for t in $PATTERN; do
    [ -f "$t" ] || continue
    if ! "$FIRNC" -o "$W/old" "$t" >"$W/old.log" 2>&1; then
        old_fail=$((old_fail+1)); continue
    fi
    if ! "$FIRNC" --asm-intern -o "$W/new" "$t" >"$W/new.log" 2>&1; then
        new_fail=$((new_fail+1))
        echo "BUILD $t"
        head -3 "$W/new.log" | sed 's/^/     /'
        continue
    fi
    a=$(run "$W/old")
    n=$(run "$W/new")
    if [ "$a" = "$n" ]; then
        same=$((same+1))
    else
        differ=$((differ+1))
        echo "RUN   $t"
        echo "     old: ${a%%|*}  new: ${n%%|*}"
    fi
done

echo "----"
echo "same behaviour     : $same"
echo "deviating          : $differ"
echo "old way fails      : $old_fail  (not the encoder's fault)"
echo "new way fails      : $new_fail"
[ "$differ" -eq 0 ] && [ "$new_fail" -eq 0 ]
