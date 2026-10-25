#!/usr/bin/env bash
# tools/hygiene/check.sh -- cheap repository guards (r574).
#   1. lib/firnc1/gctext.fi must equal what tools/gen_gctext.sh produces
#      from lib/gc/*.fi (a stale file once broke fixpoint.sh).
#      Same for lib/firnc1/gcnames.fi from lib/gc/runtime_names.txt.
#   2. No NEW duplicate test numbers in tests/ (numbers listed in
#      tools/hygiene/known_dup_testnums.txt are grandfathered).
#   3. docs/LIBS.md (the lib/ index) must match lib/ -- stale means a module,
#      purpose line, export or platform twin changed without regenerating:
#      run bash tools/gen_libs_index.sh (quick, ~1 s, no heavy needed).
#   4. Generated sources must equal their generators' output
#      (tools/strlib/expand.py --check, lib/rc/gen_tests.sh --check).
# Exit 0 = clean, 1 = a guard struck. Usage: bash tools/hygiene/check.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
rc=0
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

bash tools/gen_gctext.sh "$T/gctext.fi" >/dev/null
if ! cmp -s "$T/gctext.fi" lib/firnc1/gctext.fi; then
    echo "HYGIENE: lib/firnc1/gctext.fi is stale -- run: bash tools/gen_gctext.sh (and cargo build)"
    rc=1
fi

bash tools/gen_gcnames.sh "$T/gcnames.fi" >/dev/null
if ! cmp -s "$T/gcnames.fi" lib/firnc1/gcnames.fi; then
    echo "HYGIENE: lib/firnc1/gcnames.fi is stale -- run: bash tools/gen_gcnames.sh"
    rc=1
fi

ls tests | grep -E '^[0-9]+_' | sed -E 's/_.*//' | sort | uniq -d > "$T/dups"
sort tools/hygiene/known_dup_testnums.txt > "$T/known"
NEW=$(comm -23 "$T/dups" "$T/known")
if [ -n "$NEW" ]; then
    for n in $NEW; do
        echo "HYGIENE: duplicate test number $n: $(ls tests | grep -E "^${n}_" | tr '\n' ' ')"
    done
    echo "HYGIENE: rename to a free number (ls tests | sed -E 's/_.*//' | sort -u | tail)"
    rc=1
fi
# Grandfathered entries that are no longer duplicates should be dropped.
GONE=$(comm -13 "$T/dups" "$T/known")
[ -n "$GONE" ] && echo "HYGIENE: note: resolved, remove from known_dup_testnums.txt: $(echo $GONE)"

LIBS_OUT=$(bash tools/gen_libs_index.sh --check 2>&1) || { echo "HYGIENE: $LIBS_OUT"; rc=1; }
case "$LIBS_OUT" in *note:*) echo "HYGIENE: $LIBS_OUT" ;; esac

EXPAND_OUT=$(python3 tools/strlib/expand.py --check 2>&1) || { echo "HYGIENE: $EXPAND_OUT"; echo "HYGIENE: run: python3 tools/strlib/expand.py --all"; rc=1; }
GEN_OUT=$(bash lib/rc/gen_tests.sh --check 2>&1) || { echo "HYGIENE: $GEN_OUT"; echo "HYGIENE: run: bash lib/rc/gen_tests.sh"; rc=1; }

[ $rc = 0 ] && echo "HYGIENE: ok"
exit $rc
