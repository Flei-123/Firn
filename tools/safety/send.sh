#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
total=0
for f in "$ROOT"/tools/safety/send/*.fi; do
    n=$(basename "$f" .fi)
    hdr=$(head -n 1 "$f")
    total=$((total + 1))
    case "$hdr" in
    *expect_error:*)
        want=${hdr#*expect_error: }
        pos=${want%% *}
        text=${want#* }
        out=$("$FIRNC" -o "$W/x" "$f" 2>&1)
        if echo "$out" | grep -qF -- "$text" && echo "$out" | grep -q -- "--> .*:${pos}$"; then :; else
            echo "FAIL $n: expected '$text' at $pos"; echo "$out" | head -4; fail=$((fail + 1))
        fi
        if ! "$FIRNC" --send-check=off -o "$W/x" "$f" >/dev/null 2>&1; then
            echo "FAIL $n: must compile with --send-check=off"; fail=$((fail + 1))
        fi
        if [ -n "${FIRNC1:-}" ]; then
            out=$("$FIRNC1" -o "$W/x" "$f" 2>&1)
            if echo "$out" | grep -qF -- "$text" && echo "$out" | grep -q -- "--> .*:${pos}$"; then :; else
                echo "FAIL $n: firnc1 differs from firnc0"; echo "$out" | head -4; fail=$((fail + 1))
            fi
        fi
        ;;
    *expect_exit:*)
        want=${hdr#*expect_exit: }
        if ! "$FIRNC" -o "$W/x" "$f" >"$W/log" 2>&1; then
            echo "FAIL $n: does not compile"; head -4 "$W/log"; fail=$((fail + 1)); continue
        fi
        timeout 30 "$W/x" >/dev/null 2>&1
        rc=$?
        if [ "$rc" != "$want" ]; then echo "FAIL $n: exit $rc, want $want"; fail=$((fail + 1)); fi
        rm -f "$W/x"
        ;;
    esac
done
echo "send: $((total - fail))/$total"
[ "$fail" = 0 ]
