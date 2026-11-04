#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0
for t in tests/gc_certus/*.fi; do
    for o in release-safe release-fast dev-fast; do
        if ! FIRNLIB="$(pwd)/lib" FIRNC_GC_RUNTIME=certus timeout 300 "$FIRNC" --opt-level=$o -o "$T/b" "$t" > "$T/c.log" 2>&1; then
            echo "   COMPILE-FAIL $t $o"
            head -5 "$T/c.log" | sed 's/^/     /'
            fail=$((fail + 1))
            continue
        fi
        rc=0
        (cd "$T" && timeout 120 ./b > /dev/null 2>&1) || rc=$?
        if [ "$rc" -eq 0 ]; then
            pass=$((pass + 1))
        else
            echo "   FAIL $t $o rc=$rc"
            fail=$((fail + 1))
        fi
        rm -f "$T/b"
    done
done
echo "   gc_certus: pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
