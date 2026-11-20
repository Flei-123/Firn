#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
total=0
for f in "$ROOT"/tools/safety/borrow/*.fi; do
    n=$(basename "$f" .fi)
    hdr=$(head -n 1 "$f")
    total=$((total + 1))
    level=on
    [ "$n" = bad_local_to_extern ] && level=strict
    case "$hdr" in
    *expect_error:*)
        want=${hdr#*expect_error: }
        pos=${want%% *}
        text=${want#* }
        out=$("$FIRNC" --borrow=$level -o "$W/x" "$f" 2>&1)
        if echo "$out" | grep -qF -- "$text" && echo "$out" | grep -q -- "--> .*:${pos}$"; then :; else
            echo "FAIL $n: expected '$text' at $pos"; echo "$out" | head -4; fail=$((fail + 1))
        fi
        if ! "$FIRNC" --borrow=off --emit=asm -o "$W/x.s" "$f" >/dev/null 2>&1; then
            echo "FAIL $n: must compile with --borrow=off"; fail=$((fail + 1))
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
if [ -n "${FIRNC1:-}" ]; then
    export FIRNLIB=$ROOT/lib
    for f in "$ROOT"/tools/safety/borrow/*.fi; do
        n=$(basename "$f" .fi)
        hdr=$(head -n 1 "$f")
        total=$((total + 1))
        case "$hdr" in
        *expect_error:*)
            want=${hdr#*expect_error: }
            pos=${want%% *}
            text=${want#* }
            out=$("$FIRNC1" --borrow=strict -o "$W/x1" "$f" 2>&1)
            if echo "$out" | grep -qF -- "$text" && echo "$out" | grep -q -- "--> .*:${pos}$"; then :; else
                echo "FAIL firnc1 $n: expected '$text' at $pos"; echo "$out" | head -4; fail=$((fail + 1))
            fi
            ;;
        *expect_exit:*)
            want=${hdr#*expect_exit: }
            if ! "$FIRNC1" --borrow=strict -o "$W/x1" "$f" >"$W/log1" 2>&1; then
                echo "FAIL firnc1 $n: does not compile"; head -4 "$W/log1"; fail=$((fail + 1)); continue
            fi
            timeout 30 "$W/x1" >/dev/null 2>&1
            rc=$?
            if [ "$rc" != "$want" ]; then echo "FAIL firnc1 $n: exit $rc, want $want"; fail=$((fail + 1)); fi
            rm -f "$W/x1"
            ;;
        esac
    done
fi
echo "borrow: $((total - fail))/$total"
[ "$fail" = 0 ]
