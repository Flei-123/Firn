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
check_rule() {
    rule=$1; edit=$2; label=$3
    xargs "$LINT" --rules "$rule" --lib lib < "$T/mains.txt" 2>/dev/null | awk 'NR%11==1' | head -"$COUNT" > "$T/picked.txt"
    checked=0; wrong=0
    while IFS=: read -r f l rest; do
        dir=$(dirname "$f")
        timeout 120 "$FIRNC" -o "$T/o.bin" "$f" > /dev/null 2>&1 || continue
        sed "$l$edit" "$f" > "$dir/.lintoracle.fi"
        checked=$((checked + 1))
        if ! timeout 120 "$FIRNC" -o "$T/o.bin" "$dir/.lintoracle.fi" > /dev/null 2>&1; then
            wrong=$((wrong + 1))
            echo "   false positive ($rule): $f:$l"
        fi
        rm -f "$dir/.lintoracle.fi"
    done < "$T/picked.txt"
    echo "   $label: $((checked - wrong)) / $checked"
    [ "$wrong" -eq 0 ]
}
rc=0
check_rule unused-import d "removed flagged imports that still compile" || rc=1
check_rule var-never-mutated 's/\bvar\b/let/' "flagged var turned into let still compiles" || rc=1
exit $rc
