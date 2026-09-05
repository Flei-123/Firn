#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/checked/run.sh -- THE PROOF FOR CHECKED INTEGER ARITHMETIC
# (round 72, SPEC section 13 item L9), in BOTH compilers.
#
# What is proved, and nothing is skipped quietly:
#
#   1. A program that goes out of range ABORTS in `dev`, `dev-fast` and
#      `release-safe` -- exit code 101 -- and WRAPS in `release-fast`.
#   2. The message it prints is the same one out of BOTH compilers, octet
#      for octet: the same words, the same operator, the same file, line
#      and column.
#   3. The explicit operators `+% -% *%` (wrapping) and `+| -| *|`
#      (saturating) are NEVER checked -- the same result in all four
#      levels, out of both compilers.
#   4. Counter-checks: a program that stays IN range behaves exactly as it
#      always did, in every level and in both compilers; and a program with
#      no checked operation at all carries neither the message table nor
#      the trampoline (measured, not asserted).
#
# Usage:  bash tools/checked/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
# Never reuse an old `.firnc1`: after a change to lib/firnc1 it measures a
# compiler that no longer exists (the lesson of round 46).
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc0 cannot build firnc1"; exit 1; }
fi

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }

# $1 = compiler, $2 = level flag, $3 = source, $4 = output
build() {
    local cc="$1" flag="$2" src="$3" out="$4"
    rm -f "$out" "$out.s" "$out.o"
    if [ -z "$flag" ]; then
        "$cc" "$src" -o "$out" > "$TMPD/build.log" 2>&1
    else
        "$cc" "$flag" "$src" -o "$out" > "$TMPD/build.log" 2>&1
    fi
}

LEVELS_CHECKED="--no-opt --opt-level=dev-fast --opt-level=release-safe"
LEVEL_FAST="--opt-level=release-fast"

# ---------------------------------------------------------------------------
echo "== 1. a program that goes out of range aborts, and says the same thing =="
# Every one of these has to panic in the three checked levels and wrap in
# release-fast, in BOTH compilers, with byte-identical output.
for src in tools/checked/add_i32.fi tools/checked/sub_i8.fi \
           tools/checked/mul_u8.fi tools/checked/mul_i8.fi \
           tools/checked/cast_narrow.fi tools/checked/cast_sign.fi; do
    name=$(basename "$src" .fi)
    for lvl in $LEVELS_CHECKED; do
        msg0=""; msg1=""; rc0=0; rc1=0
        for cc in 0 1; do
            [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
            if ! build "$C" "$lvl" "$src" "$TMPD/$name.$cc"; then
                bad "$name [$lvl] compiler $cc did not build: $(head -2 "$TMPD/build.log")"
                continue 2
            fi
            "$TMPD/$name.$cc" > /dev/null 2> "$TMPD/err.$cc"
            rc=$?
            if [ "$cc" = 0 ]; then rc0=$rc; msg0=$(cat "$TMPD/err.$cc"); else rc1=$rc; msg1=$(cat "$TMPD/err.$cc"); fi
        done
        if [ "$rc0" -ne 101 ]; then bad "$name [$lvl] firnc0 exit $rc0, expected 101"; else ok; fi
        if [ "$rc1" -ne 101 ]; then bad "$name [$lvl] firnc1 exit $rc1, expected 101"; else ok; fi
        if [ "$msg0" != "$msg1" ]; then
            bad "$name [$lvl] different message: firnc0 '$msg0' vs firnc1 '$msg1'"
        else
            ok
        fi
        case "$msg0" in
            "panic: "*" at $src:"*) ok;;
            *) bad "$name [$lvl] message without 'panic:' and file:line:column: '$msg0'";;
        esac
    done
    # release-fast: the same program wraps and returns 7
    for cc in 0 1; do
        [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
        if ! build "$C" "$LEVEL_FAST" "$src" "$TMPD/$name.f$cc"; then
            bad "$name [release-fast] compiler $cc did not build"
            continue
        fi
        "$TMPD/$name.f$cc" > /dev/null 2>&1
        rc=$?
        if [ "$rc" -ne 7 ]; then bad "$name [release-fast] compiler $cc exit $rc, expected 7 (wrapped)"; else ok; fi
    done
done

# ---------------------------------------------------------------------------
echo "== 2. division: by zero and MIN / -1 =="
# Only the three CHECKED levels: release-fast has nothing to wrap here, the
# processor raises SIGFPE, and that is not this round's promise.
for src in tools/checked/div_zero.fi tools/checked/div_min.fi tools/checked/rem_zero.fi; do
    name=$(basename "$src" .fi)
    for lvl in $LEVELS_CHECKED; do
        msg0=""; msg1=""
        for cc in 0 1; do
            [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
            if ! build "$C" "$lvl" "$src" "$TMPD/$name.$cc"; then
                bad "$name [$lvl] compiler $cc did not build"
                continue 2
            fi
            "$TMPD/$name.$cc" > /dev/null 2> "$TMPD/err.$cc"
            rc=$?
            if [ "$rc" -ne 101 ]; then bad "$name [$lvl] compiler $cc exit $rc, expected 101"; else ok; fi
            [ "$cc" = 0 ] && msg0=$(cat "$TMPD/err.$cc") || msg1=$(cat "$TMPD/err.$cc")
        done
        if [ "$msg0" != "$msg1" ]; then
            bad "$name [$lvl] different message: '$msg0' vs '$msg1'"
        else
            ok
        fi
    done
done

# ---------------------------------------------------------------------------
echo "== 3. +% -% *% and +| -| *| are never checked =="
for src in tools/checked/wrapsat.fi tools/checked/two_sites.fi; do
    name=$(basename "$src" .fi)
    for lvl in $LEVELS_CHECKED $LEVEL_FAST; do
        for cc in 0 1; do
            [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
            if ! build "$C" "$lvl" "$src" "$TMPD/$name.$cc"; then
                bad "$name [$lvl] compiler $cc did not build: $(head -2 "$TMPD/build.log")"
                continue
            fi
            "$TMPD/$name.$cc" > /dev/null 2>&1
            rc=$?
            if [ "$rc" -ne 42 ]; then bad "$name [$lvl] compiler $cc exit $rc, expected 42"; else ok; fi
        done
    done
done

# ---------------------------------------------------------------------------
echo "== 4. counter-check: what stays in range behaves as it always did =="
for lvl in $LEVELS_CHECKED $LEVEL_FAST; do
    for cc in 0 1; do
        [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
        if ! build "$C" "$lvl" tools/checked/widths.fi "$TMPD/w.$cc"; then
            bad "widths [$lvl] compiler $cc did not build"
            continue
        fi
        "$TMPD/w.$cc" > /dev/null 2>&1
        rc=$?
        if [ "$rc" -ne 42 ]; then bad "widths [$lvl] compiler $cc exit $rc, expected 42"; else ok; fi
    done
done

# ---------------------------------------------------------------------------
echo "== 5. counter-check: no checked operation, no cost =="
# `release-fast` contains no checked site at all, so neither the message
# table nor the trampoline may appear in the assembly text -- in both
# compilers. The same program in `dev-fast` has to contain both, otherwise
# this measurement would prove nothing.
# `firnc0` deletes its assembly text after linking and writes it on demand
# (`--emit=asm`); `firnc1` leaves `<output>.s` lying next to the binary.
# Two ways to the same text, no third one.
asm_of() {   # $1 = compiler number, $2 = level, $3 = source, $4 = target .s
    if [ "$1" = 0 ]; then
        "$FIRNC" --emit=asm "$2" "$3" -o "$4" > "$TMPD/build.log" 2>&1
    else
        "$FC1" "$2" "$3" -o "$TMPD/asmout" > "$TMPD/build.log" 2>&1 && cp "$TMPD/asmout.s" "$4"
    fi
}
for cc in 0 1; do
    asm_of "$cc" "$LEVEL_FAST" tools/checked/add_i32.fi "$TMPD/cost.f$cc.s"
    asm_of "$cc" "--opt-level=dev-fast" tools/checked/add_i32.fi "$TMPD/cost.d$cc.s"
    nf=$(grep -c 'Lpanic_arith' "$TMPD/cost.f$cc.s" 2>/dev/null)
    nd=$(grep -c 'Lpanic_arith' "$TMPD/cost.d$cc.s" 2>/dev/null)
    if [ "$nf" -ne 0 ]; then bad "release-fast, compiler $cc: $nf mentions of the trampoline, expected 0"; else ok; fi
    if [ "$nd" -lt 2 ]; then bad "dev-fast, compiler $cc: only $nd mentions of the trampoline"; else ok; fi
    echo "  compiler $cc: release-fast $(wc -c < "$TMPD/cost.f$cc.s") octets of assembly, dev-fast $(wc -c < "$TMPD/cost.d$cc.s")"
done

# ---------------------------------------------------------------------------
echo "== 6. the optimiser sees the checked arithmetic =="
# ROUND 72, second pass. `release-safe` is the level that CHECKS and runs
# every optimisation pass. Between the two passes of this round it ran the
# passes and folded nothing at all, because the constant folder matched
# `Op::Bin` and the arithmetic had become `Op::CheckedBin`. Two numbers, not
# one: everything foldable has to be GONE after the optimiser, and a checked
# operation that really goes out of range has to still be THERE -- folding
# that one away would delete a panic the program promised.
before=$("$FIRNC" --opt-level=release-safe --emit=fir-raw tools/checked/fold.fi 2>/dev/null | grep -cE 'checked_|_wrap|_sat')
after=$("$FIRNC" --opt-level=release-safe --emit=fir-opt tools/checked/fold.fi 2>/dev/null | grep -cE 'checked_|_wrap|_sat')
if [ "$before" -lt 5 ]; then bad "fold: only $before checked/wrap/sat operations before the optimiser"; else ok; fi
if [ "$after" -ne 0 ]; then bad "fold: $after checked/wrap/sat operations survive the optimiser, expected 0"; else ok; fi
echo "  foldable arithmetic: $before operations before the optimiser, $after after"
kept=$("$FIRNC" --opt-level=release-safe --emit=fir-opt tools/checked/add_i32.fi 2>/dev/null | grep -c 'checked_add')
if [ "$kept" -lt 1 ]; then bad "fold: the out-of-range addition was folded away"; else ok; fi
for lvl in $LEVELS_CHECKED $LEVEL_FAST; do
    for cc in 0 1; do
        [ "$cc" = 0 ] && C="$FIRNC" || C="$FC1"
        if ! build "$C" "$lvl" tools/checked/fold.fi "$TMPD/fold.$cc"; then
            bad "fold [$lvl] compiler $cc did not build"
            continue
        fi
        "$TMPD/fold.$cc" > /dev/null 2>&1
        rc=$?
        if [ "$rc" -ne 42 ]; then bad "fold [$lvl] compiler $cc exit $rc, expected 42"; else ok; fi
    done
done

echo
echo "CHECKS PASSED: $PASS"
echo "CHECKS FAILED: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
