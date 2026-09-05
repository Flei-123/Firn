#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/checkidx/run.sh -- THE PROOF FOR THE CHECKED INDEX, THE CHECKED
# DIVISION AND THE REPLACEABLE PANIC HANDLER (round 89, SPEC 13 items
# L9), in BOTH compilers -- after the pattern of tools/checked/run.sh.
#
# THE ABORT IS THE RESULT. What is measured is not that the program runs
# but that it STOPS, at the right place, with the right sentence, and that
# both compilers say that sentence octet for octet the same way.
#
#   1. INDEX OUT OF RANGE. `a[9]` on an `[i32; 4]` aborts in `dev`,
#      `dev-fast` and `release-safe` -- exit code 101 -- and reads past the
#      end in `release-fast`, where the name says so.
#   2. THE MESSAGE. File, line, column, the array type, the index and the
#      length, in the words `index=` and `len=` and not `a=`/`b=`. Out of
#      both compilers, compared octet for octet.
#   3. AT COMPILE TIME where it can be. A constant index outside a known
#      length is an ERROR, at every build level, `release-fast` included --
#      that is not an optimisation of the run time check, it is a different
#      promise.
#   4. DIVISION BY ZERO and `MIN / -1` (round 72, re-measured here because
#      the honest list in the README said until this round that neither was
#      caught, and that was wrong).
#   5. THE PRICE IS PAID ONLY WHERE IT IS OWED. In `release-fast` there is
#      no comparison in the emitted code at all; and where the optimiser
#      can PROVE the index is inside (a loop over a constant bound), the
#      check disappears again -- measured on the FIR, not asserted.
#   6. THE PANIC HANDLER. A program with `#[panic_handler]` ends its panic
#      itself: the default text does not appear, the program's own does,
#      and the exit code is the one the handler chose. The counter-check:
#      the same program WITHOUT the attribute produces the default again.
#
# Usage:  bash tools/checkidx/run.sh
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
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc0 cannot build firnc1"; exit 1; }
fi

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }

LEVELS_CHECKED="--no-opt --opt-level=dev-fast --opt-level=release-safe"
LEVEL_FAST="--opt-level=release-fast"

# The assembly text of a program. firnc0 writes it with `--emit=asm -o`,
# firnc1 writes it to STANDARD OUTPUT when no `-o` is given.
asm_of() { # $1 = compiler index, $2 = level flag, $3 = source, $4 = output
    if [ "$1" = 0 ]; then
        "$FIRNC" "$2" --emit=asm "$3" -o "$4" > /dev/null 2>&1
    else
        "$FC1" "$2" "$3" > "$4" 2> /dev/null
    fi
}

build() {
    local cc="$1" flag="$2" src="$3" out="$4"
    rm -f "$out"
    if [ -z "$flag" ]; then
        "$cc" "$src" -o "$out" > "$TMPD/build.log" 2>&1
    else
        "$cc" "$flag" "$src" -o "$out" > "$TMPD/build.log" 2>&1
    fi
}

# ---------------------------------------------------------------------------
echo "== 1. an index outside the array aborts, and both compilers say the same =="
for src in tools/checkidx/idx_read.fi tools/checkidx/idx_write.fi \
           tools/checkidx/idx_u8.fi tools/checkidx/idx_nested.fi; do
    name=$(basename "$src" .fi)
    for lvl in $LEVELS_CHECKED; do
        msg0=""; msg1=""; rc0=0; rc1=0
        for cc in 0 1; do
            if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
            if ! build "$C" "$lvl" "$src" "$TMPD/$name.$cc"; then
                bad "$name [$lvl] compiler $cc did not build: $(head -2 "$TMPD/build.log")"
                continue 2
            fi
            "$TMPD/$name.$cc" > /dev/null 2> "$TMPD/err.$cc"
            rc=$?
            if [ "$cc" = 0 ]; then rc0=$rc; msg0=$(cat "$TMPD/err.$cc"); else rc1=$rc; msg1=$(cat "$TMPD/err.$cc"); fi
        done
        if [ "$rc0" != 101 ]; then
            bad "$name [$lvl] firnc0: exit code $rc0, expected 101"
        elif [ "$rc1" != 101 ]; then
            bad "$name [$lvl] firnc1: exit code $rc1, expected 101"
        elif [ "$msg0" != "$msg1" ]; then
            bad "$name [$lvl] the two compilers say different things:"
            echo "        firnc0: $msg0"
            echo "        firnc1: $msg1"
        else
            case "$msg0" in
                *"index out of bounds"*"index="*"len="*) ok ;;
                *) bad "$name [$lvl] message without index/len: $msg0" ;;
            esac
        fi
    done
    # release-fast: no check, no abort. The program reads past the end and
    # says so by NOT dying -- that is what the level's name promises.
    for cc in 0 1; do
        if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
        if ! build "$C" "$LEVEL_FAST" "$src" "$TMPD/$name.f.$cc"; then
            bad "$name [release-fast] compiler $cc did not build"
            continue
        fi
        "$TMPD/$name.f.$cc" > /dev/null 2>&1
        if [ $? = 101 ]; then
            bad "$name [release-fast] compiler $cc aborted anyway"
        else
            ok
        fi
    done
done

# ---------------------------------------------------------------------------
echo "== 2. the message names file, line, column, type, index and length =="
"$FIRNC" --no-opt tools/checkidx/idx_read.fi -o "$TMPD/m0" > /dev/null 2>&1
"$TMPD/m0" > /dev/null 2> "$TMPD/m0.err"
want="panic: index out of bounds in '[i32; 4]' at tools/checkidx/idx_read.fi:11:14 (index=9 len=4)"
got=$(cat "$TMPD/m0.err")
if [ "$got" = "$want" ]; then ok; else
    bad "the message is not the expected one"
    echo "        expected: $want"
    echo "        got:      $got"
fi

# ---------------------------------------------------------------------------
echo "== 3. a constant index is refused at COMPILE time, at every level =="
for lvl in $LEVELS_CHECKED $LEVEL_FAST; do
    if "$FIRNC" "$lvl" tools/checkidx/neg/const_index.fi -o "$TMPD/ci" > "$TMPD/ci.log" 2>&1; then
        bad "const_index [$lvl] firnc0: was accepted"
    elif ! grep -q "is outside" "$TMPD/ci.log"; then
        bad "const_index [$lvl] firnc0: refused without saying what: $(head -1 "$TMPD/ci.log")"
    else
        ok
    fi
    if "$FC1" "$lvl" tools/checkidx/neg/const_index.fi -o "$TMPD/ci1" > /dev/null 2>&1; then
        bad "const_index [$lvl] firnc1: was accepted"
    else
        ok
    fi
done
# The same through a `const`, which is a number just as much.
if "$FIRNC" tools/checkidx/neg/const_name.fi -o "$TMPD/cn" > "$TMPD/cn.log" 2>&1; then
    bad "const_name firnc0: was accepted"
else
    ok
fi

# ---------------------------------------------------------------------------
echo "== 4. division by zero and MIN / -1 (round 72, re-measured) =="
for src in tools/checkidx/div_zero.fi tools/checkidx/div_min.fi tools/checkidx/rem_zero.fi; do
    name=$(basename "$src" .fi)
    for lvl in $LEVELS_CHECKED; do
        msg0=""; msg1=""; rc0=0; rc1=0
        for cc in 0 1; do
            if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
            if ! build "$C" "$lvl" "$src" "$TMPD/$name.$cc"; then
                bad "$name [$lvl] compiler $cc did not build"
                continue 2
            fi
            "$TMPD/$name.$cc" > /dev/null 2> "$TMPD/err.$cc"
            rc=$?
            if [ "$cc" = 0 ]; then rc0=$rc; msg0=$(cat "$TMPD/err.$cc"); else rc1=$rc; msg1=$(cat "$TMPD/err.$cc"); fi
        done
        if [ "$rc0" != 101 ] || [ "$rc1" != 101 ]; then
            bad "$name [$lvl]: exit codes $rc0/$rc1, expected 101/101"
        elif [ "$msg0" != "$msg1" ]; then
            bad "$name [$lvl] the two compilers say different things:"
            echo "        firnc0: $msg0"
            echo "        firnc1: $msg1"
        else
            ok
        fi
    done
done
# `release-fast` hands division by zero to the processor: SIGFPE, exit code
# 136, and no sentence at all. Named here so that nobody has to guess what
# the level costs.
"$FIRNC" $LEVEL_FAST tools/checkidx/div_zero.fi -o "$TMPD/dz.f" > /dev/null 2>&1
# The shell announces a SIGFPE on the terminal; the exit code is what is
# being measured, so the announcement is not part of the result.
dzrc=0
"$TMPD/dz.f" > /dev/null 2>&1 || dzrc=$?
if [ "$dzrc" = 136 ]; then ok; else bad "div_zero [release-fast]: exit $dzrc, expected SIGFPE (136)"; fi

# ---------------------------------------------------------------------------
echo "== 5. the price is paid only where it is owed =="
# release-fast: not one bounds check in the emitted code.
for cc in 0 1; do
    if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
    asm_of "$cc" "$LEVEL_FAST" tools/checkidx/loop_const.fi "$TMPD/lf.$cc.s"
    if grep -q "Lchkidx\|Lpanic_index" "$TMPD/lf.$cc.s"; then
        bad "release-fast [compiler $cc]: a bounds check in the emitted code"
    else
        ok
    fi
done
# dev-fast: the loop over a constant bound is PROVED inside, so the check
# that lowering put there is gone again. Both numbers are read off the FIR:
# `--emit=fir-raw` is before the optimiser, `--emit=fir` after it.
raw=$("$FIRNC" --emit=fir-raw tools/checkidx/loop_const.fi 2>/dev/null | grep -c "checked_idx")
opt=$("$FIRNC" --emit=fir tools/checkidx/loop_const.fi 2>/dev/null | grep -c "checked_idx")
if [ "$raw" -lt 1 ]; then
    bad "loop_const: lowering did not put a single check in (nothing to prove)"
elif [ "$opt" != 0 ]; then
    bad "loop_const: $raw checks before the optimiser, $opt left after it (expected 0)"
else
    ok
fi
# And the counter-check that the pass is not simply deleting everything:
# a loop whose bound is NOT known keeps its check.
uraw=$("$FIRNC" --emit=fir-raw tools/checkidx/loop_var.fi 2>/dev/null | grep -c "checked_idx")
uopt=$("$FIRNC" --emit=fir tools/checkidx/loop_var.fi 2>/dev/null | grep -c "checked_idx")
if [ "$uopt" -ge 1 ]; then ok; else
    bad "loop_var: $uraw checks before, $uopt after -- an unprovable one was removed"
fi

# ---------------------------------------------------------------------------
echo "== 6. #[panic_handler]: the program ends its own panic =="
for cc in 0 1; do
    if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
    if ! build "$C" "--no-opt" tools/checkidx/handler.fi "$TMPD/ph.$cc"; then
        bad "handler [compiler $cc]: did not build: $(head -2 "$TMPD/build.log")"
        continue
    fi
    "$TMPD/ph.$cc" > /dev/null 2> "$TMPD/ph.$cc.err"
    rc=$?
    txt=$(cat "$TMPD/ph.$cc.err")
    if [ "$rc" != 42 ]; then
        bad "handler [compiler $cc]: exit code $rc, expected 42 (the handler's own)"
    elif ! grep -q "caught by the program" "$TMPD/ph.$cc.err"; then
        bad "handler [compiler $cc]: the handler did not run: $txt"
    else
        ok
    fi
done
# The counter-check: the SAME program without the attribute falls back to
# the default -- exit code 101 and the message the compiler writes.
for cc in 0 1; do
    if [ "$cc" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
    build "$C" "--no-opt" tools/checkidx/handler_off.fi "$TMPD/pho.$cc"
    "$TMPD/pho.$cc" > /dev/null 2> "$TMPD/pho.$cc.err"
    rc=$?
    if [ "$rc" = 101 ] && grep -q "index out of bounds" "$TMPD/pho.$cc.err"; then ok; else
        bad "handler_off [compiler $cc]: exit $rc, message '$(cat "$TMPD/pho.$cc.err")'"
    fi
done
# A wrong signature is refused, and the message says what the right one is.
if "$FIRNC" tools/checkidx/neg/handler_sig.fi -o "$TMPD/hs" > "$TMPD/hs.log" 2>&1; then
    bad "handler_sig: a wrong signature was accepted"
elif ! grep -q "fixed signature" "$TMPD/hs.log"; then
    bad "handler_sig: refused without saying why: $(head -1 "$TMPD/hs.log")"
else
    ok
fi

echo
echo "checkidx: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
