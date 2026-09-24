#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/promote/run.sh -- round OPT-GENERAL: the promotion pass NATIVELY.
#
# `promote` (compiler/src/promote.rs) is on by default for WebAssembly only
# (tools/wasm/run.sh covers it there); natively it waits for the register
# allocator of TEMPO 1-13. So that it does not rot natively in the meantime,
# this runs a set of programs with FIRN_PROMOTE_NATIVE=1 in both release
# levels, once normally and once in the stress mode that rotates EVERY loop
# (FIRN_PROMOTE_ROTATE_ALL=1 FIRN_PROMOTE_CLEANUP_FIRST=1), against the
# expectation in line 1 of each file. The programs: the pass's own test,
# the JavaScript lexer tests that found the rotation bug of this round, and
# a spread of loop-heavy ones.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$PWD/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=${W:-/tmp/firn-promote-run}
mkdir -p "$W"
PROGS="tests/1720_promote_regions.fi tests/1721_wasm_tree_phi.fi tests/1000_js_lex.fi tests/1002_js_interp.fi tests/1501_js_regexp.fi tests/1503_js_r74_gc.fi tests/065_deep_nesting.fi examples/bubblesort.fi examples/fib.fi"
pass=0; fail=0
for f in $PROGS; do
    [ -f "$f" ] || continue
    first=$(head -1 "$f")
    for lvl in release-fast release-safe; do
        for mode in normal stress; do
            b="$W/$(basename "$f" .fi).$lvl.$mode"
            if [ $mode = stress ]; then
                env FIRN_PROMOTE_NATIVE=1 FIRN_PROMOTE_ROTATE_ALL=1 FIRN_PROMOTE_CLEANUP_FIRST=1 \
                    "$FIRNC" --opt-level=$lvl -o "$b" "$f" > "$b.log" 2>&1
            else
                env FIRN_PROMOTE_NATIVE=1 "$FIRNC" --opt-level=$lvl -o "$b" "$f" > "$b.log" 2>&1
            fi || { echo "  FAIL $f [$lvl $mode]: compile"; fail=$((fail + 1)); continue; }
            out=$(timeout 120 "$b" < /dev/null 2>/dev/null); rc=$?
            good=1
            case "$first" in
                "// expect_exit: "*) [ "$rc" = "${first#// expect_exit: }" ] || good=0 ;;
                "// expect_out: "*) [ "$out" = "${first#// expect_out: }" ] || good=0 ;;
            esac
            if [ $good = 1 ]; then pass=$((pass + 1)); else echo "  FAIL $f [$lvl $mode]: exit $rc"; fail=$((fail + 1)); fi
        done
    done
done
echo "   promote natively: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
