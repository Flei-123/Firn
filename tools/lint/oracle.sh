#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
LINT=${1:?lint binary}
COUNT=${2:-8}
export FIRNLIB="$ROOT/lib"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
find tests bench -name '*.fi' -not -type l -not -path 'tests/lexneg/*' -not -path 'tests/neg/*' | sort > "$T/all.txt"
xargs grep -l '^fn main' < "$T/all.txt" > "$T/mains.txt"
xargs "$LINT" --rules unused-import --lib lib < "$T/mains.txt" 2>/dev/null | awk 'NR%11==1' | head -"$COUNT" > "$T/picked.txt"
checked=0; wrong=0
while IFS=: read -r f l rest; do
    dir=$(dirname "$f")
    timeout 120 "$FIRNC" -o "$T/o.bin" "$f" > /dev/null 2>&1 || continue
    sed "${l}d" "$f" > "$dir/.lintoracle.fi"
    checked=$((checked + 1))
    if ! timeout 120 "$FIRNC" -o "$T/o.bin" "$dir/.lintoracle.fi" > /dev/null 2>&1; then
        wrong=$((wrong + 1))
        echo "   false positive: $f:$l"
    fi
    rm -f "$dir/.lintoracle.fi"
done < "$T/picked.txt"
echo "   removed flagged imports that still compile: $((checked - wrong)) / $checked"
[ "$wrong" -eq 0 ]
