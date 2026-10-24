#!/usr/bin/env bash
# Proof for firnlint: fixtures, counter-checks, self-lint, oracle on the tree.
# Usage: bash tools/lint/run.sh [--fast]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
FAIL=0
note() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }

echo "== 1. build firnlint =="
LINT="$T/firnlint"
FIRNLIB="$ROOT/lib" "$FIRNC" -o "$LINT" tools/lint/firnlint.fi || { echo "cannot build firnlint"; exit 1; }
echo "   built: $(wc -c < "$LINT") octets"

echo "== 2. fixtures =="
cases=0; bad=0
for src in tools/lint/cases/*.fi; do
    base=${src%.fi}
    [ -e "$base.expected" ] || continue
    cases=$((cases + 1))
    ( cd tools/lint/cases && "$LINT" --lib ../../../lib "$(basename "$src")" > "$T/out.txt" 2>/dev/null )
    if ! cmp -s "$T/out.txt" "$base.expected"; then
        bad=$((bad + 1))
        echo "   $src"
        diff "$base.expected" "$T/out.txt" | head -8 | sed 's/^/     /'
    fi
done
echo "   cases: $((cases - bad)) / $cases"
[ "$bad" -eq 0 ] || note "fixture mismatch"

echo "== 3. counter-checks =="
( cd tools/lint/cases && "$LINT" clean.fi > /dev/null 2>&1 ); [ $? -eq 0 ] || note "clean file must exit 0"
( cd tools/lint/cases && "$LINT" unused_variable.fi > /dev/null 2>&1 ); [ $? -eq 1 ] || note "findings must exit 1"
( cd tools/lint/cases && "$LINT" broken.fi > /dev/null 2>&1 ); [ $? -eq 3 ] || note "unscannable source must exit 3"
"$LINT" tools/lint/nonexistent.fi > /dev/null 2>&1; [ $? -eq 2 ] || note "missing file must exit 2"
( cd tools/lint/cases && "$LINT" --rules unreachable-code unused_variable.fi > /dev/null 2>&1 ); [ $? -eq 0 ] || note "--rules must restrict the rules"
"$LINT" --list | grep -c . | grep -qx 11 || note "--list must name 11 rules"
echo "   done"

echo "== 4. self-lint =="
"$LINT" --lib lib tools/lint/*.fi tools/lint/rules/*.fi > "$T/self.txt" 2>&1
selfrc=$?
if [ "$selfrc" -ne 0 ]; then
    head -10 "$T/self.txt" | sed 's/^/     /'
    note "firnlint sources are not lint-clean"
else
    echo "   own sources clean"
fi

echo "== 5. tree =="
find lib bin tests demos examples bench tools -name '*.fi' -not -type l -not -path 'tests/lexneg/*' | sort > "$T/files.txt"
xargs "$LINT" --lib lib < "$T/files.txt" > "$T/tree.txt" 2> "$T/tree.err"
echo "   files: $(wc -l < "$T/files.txt")  findings: $(wc -l < "$T/tree.txt")  refused: $(wc -l < "$T/tree.err")"
awk '{print $2}' "$T/tree.txt" | sort | uniq -c | sed 's/^/   /'
while IFS= read -r rf; do
    p=${rf##*: }
    if FIRNLIB="$ROOT/lib" "$FIRNC" -o /dev/null "$p" > /dev/null 2>&1; then
        note "firnlint refuses a source the compiler accepts: $p"
    fi
done < "$T/tree.err"

if [ "$FAST" -eq 0 ]; then
    echo "== 6. oracle: edits suggested by the rules still compile =="
    bash tools/lint/oracle.sh "$LINT" 8 || note "oracle found false positives"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "LINT: everything passed"
    exit 0
fi
echo "LINT: $FAIL checks failed"
exit 1
