#!/usr/bin/env bash
# Records what firnc builds do with the memory-safety probes in tools/safety/ub/ and
# compares with tools/safety/expected.txt (docs/SAFETY.md). A line is: <name> <mode> <result>,
# result = exit code of the program or CERR (compile error).
# Usage: bash tools/safety/run.sh [--update]
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
got="$W/got.txt"
: > "$got"
for f in "$ROOT"/tools/safety/ub/*.fi; do
    n=$(basename "$f" .fi)
    for m in dev-fast release-safe release-fast; do
        if ! timeout 300 "$FIRNC" --opt-level=$m -o "$W/x" "$f" >/dev/null 2>&1; then
            echo "$n $m CERR" >> "$got"
            continue
        fi
        timeout 30 "$W/x" >/dev/null 2>&1
        echo "$n $m $?" >> "$got"
        rm -f "$W/x"
    done
done
if [ "${1:-}" = "--update" ]; then
    cp "$got" "$ROOT/tools/safety/expected.txt"
    echo "updated: $(wc -l < "$got") lines"
    exit 0
fi
if diff -u "$ROOT/tools/safety/expected.txt" "$got"; then
    echo "safety probes: $(wc -l < "$got") results as recorded"
else
    echo "safety probes: DIFFERENCE (update docs/SAFETY.md, then run with --update)"
    exit 1
fi
