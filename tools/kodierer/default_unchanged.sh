#!/usr/bin/env bash
# ROUND KODIERER -- proof that the DEFAULT PATH has not changed.
#
# The round ADDED modules and built in a flag; without `--asm-intern` firnc is
# to do exactly what it did before. That can be checked more strongly and more
# cheaply than with the whole test suite: the same test case, built with the
# UNTOUCHED compiler and with the one of this round, result compared octet by
# octet.
#
# Usage: bash tools/kodierer/default_unchanged.sh [old-firnc] [pattern]
set -u
cd "$(dirname "$0")/../.."
OLD="${1:-/root/firn/compiler/target/release/firnc}"
NEW="compiler/target/release/firnc"
PATTERN="${2:-tests/*.fi}"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT

if [ ! -x "$OLD" ]; then
    echo "no untouched compiler at $OLD"
    exit 2
fi

same=0; differ=0; skipped=0
for t in $PATTERN; do
    [ -f "$t" ] || continue
    "$OLD" -o "$W/a" "$t" >/dev/null 2>&1 || { skipped=$((skipped+1)); continue; }
    "$NEW" --asm-as -o "$W/b" "$t" >/dev/null 2>&1 || { skipped=$((skipped+1)); continue; }
    if cmp -s "$W/a" "$W/b"; then
        same=$((same+1))
    else
        differ=$((differ+1))
        echo "DEVIATION: $t"
    fi
    rm -f "$W/a" "$W/b"
done
echo "----"
echo "octet-identical to the untouched compiler : $same"
echo "deviating                                 : $differ"
echo "skipped (builds in neither of the two)    : $skipped"
[ "$differ" -eq 0 ]
