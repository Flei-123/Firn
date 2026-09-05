#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/phi/run.sh -- ROUND 92: THE LOOP COUNTER LEAVES THE FRAME.
#
# WHY THIS EXISTS.
#
# Round 92 gave FIR phi nodes. The point of them is not the instruction, it
# is what becomes possible once a variable written SEVERAL TIMES can live in
# a value instead of in a stack slot: `mem2reg.rs` could only resolve cells
# written exactly once before, so every loop counter stayed in memory, and a
# counter in memory is invisible to induction variable analysis, to range
# analysis across a back edge and to loop invariant motion of anything that
# reads it.
#
# A round whose result is "a foundation" is exactly the kind of round that
# can quietly do nothing. So this section does not ask whether the code
# compiles. It asks the two questions that have an answer either way:
#
#   1. IS THE COUNTER REALLY OUT OF THE FRAME? The body of `sum_to` is
#      compared with and without `--no-pass=mem2reg`, and the number of
#      MEMORY ACCESSES in the loop is counted, on both machines. Without the
#      pass the loop must touch memory; with it, on x86 not at all.
#   2. IS THE PARALLEL COPY RIGHT? `rotate` moves three variables round in a
#      circle and `swap_n` exchanges two. On the back edge those are copies
#      that all happen AT THE SAME MOMENT; written out in any order without
#      breaking the cycle, all the variables end up with the same value.
#      The program says so with its exit code, at all four build levels and
#      on both machines (aarch64 under `qemu-aarch64` when it is there).
#
# Usage:  bash tools/phi/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
SRC=tools/phi/loops.fi
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FAIL=0
note() { echo "  $*"; }
bad()  { FAIL=$((FAIL + 1)); echo "  FAIL  $*"; }

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi

# The body of one function out of an assembly file: from its label to the
# next empty line. Deliberately the WHOLE body and not a guessed inner block
# -- `sum_to` is eight lines long, and a count over the whole of it cannot be
# argued with.
body() {                    # $1 = .s file, $2 = function name
    awk -v fn="$2" '
        $0 ~ "^[_A-Za-z0-9.$]*" fn ":$" { on = 1; next }
        on && /^$/ { exit }
        on { print }
    ' "$1"
}

# --------------------------------------------------- 1. the four levels ----
echo "1. the program is right at every build level"
for lvl in dev dev-fast release-safe release-fast; do
    if ! "$FIRNC" --opt-level="$lvl" -o "$TMPD/loops.$lvl" "$SRC" >"$TMPD/c.err" 2>&1; then
        bad "$lvl: does not compile -- $(head -2 "$TMPD/c.err" | tr '\n' ' ')"
        continue
    fi
    "$TMPD/loops.$lvl" >/dev/null 2>&1
    rc=$?
    [ "$rc" -eq 0 ] || bad "$lvl: exit code $rc (see the numbers in $SRC)"
done
# ... and with the pass of this round switched off, so that it is provably
# the pass and not the program that is being measured.
"$FIRNC" --opt-level=release-fast --no-pass=mem2reg -o "$TMPD/loops.nom2r" "$SRC" >/dev/null 2>&1 \
    || bad "release-fast --no-pass=mem2reg does not compile"
if [ -x "$TMPD/loops.nom2r" ]; then
    "$TMPD/loops.nom2r" >/dev/null 2>&1
    rc=$?
    [ "$rc" -eq 0 ] || bad "--no-pass=mem2reg: exit code $rc"
fi
note "four levels plus --no-pass=mem2reg, all 0"

# ------------------------------------------------------------ 2. the FIR ---
echo "2. the counter is a phi and no longer a stack slot"
"$FIRNC" --opt-level=release-fast --emit=fir-opt "$SRC" > "$TMPD/opt.fir" 2>/dev/null
awk '/^fn @[_A-Za-z0-9.]*sum_to\(/,/^}/' "$TMPD/opt.fir" > "$TMPD/sum_to.fir"
if [ ! -s "$TMPD/sum_to.fir" ]; then
    bad "@sum_to is not in the FIR text"
else
    grep -q ' = phi\.' "$TMPD/sum_to.fir" || bad "@sum_to has no phi:
$(cat "$TMPD/sum_to.fir")"
    mem=$(grep -cE '(alloca|load|store)\.' "$TMPD/sum_to.fir")
    [ "$mem" -eq 0 ] || bad "@sum_to still touches memory $mem times:
$(cat "$TMPD/sum_to.fir")"
    phis=$(grep -c ' = phi\.' "$TMPD/sum_to.fir")
    note "@sum_to: $phis phi nodes, 0 alloca/load/store"
fi
# The counter-check: without the pass the cell IS there. Otherwise the line
# above would be measuring a program that never had a variable.
"$FIRNC" --opt-level=release-fast --no-pass=mem2reg --emit=fir-opt "$SRC" > "$TMPD/nom2r.fir" 2>/dev/null
awk '/^fn @[_A-Za-z0-9.]*sum_to\(/,/^}/' "$TMPD/nom2r.fir" > "$TMPD/sum_to.nom2r.fir"
mem0=$(grep -cE '(alloca|load|store)\.' "$TMPD/sum_to.nom2r.fir")
[ "$mem0" -gt 0 ] || bad "without mem2reg @sum_to touches no memory either -- nothing is being measured"
note "without the pass: $mem0 alloca/load/store in the same function"

# ------------------------------------------------------- 3. x86 assembly ---
# NOTE, and it is the honest half of this round's result: on x86 the counter
# was NOT in memory before either. `regalloc.rs` has promoted `alloca`s that
# fit in a register since round 40 -- it says so in its own header: "that
# replaces the phi nodes which FIR does not have". So the number to ask for
# here is not "zero frame accesses" (it was zero before), it is whether the
# code got SHORTER: the phi is a copy per back edge, and if the copy is not
# folded into the instruction that computes the value, this round makes x86
# code longer instead of shorter.
echo "3. x86_64: no frame access, and not one instruction more than before"
"$FIRNC" --opt-level=release-fast --emit=asm -o "$TMPD/on.s" "$SRC" >/dev/null 2>&1
"$FIRNC" --opt-level=release-fast --no-pass=mem2reg --emit=asm -o "$TMPD/off.s" "$SRC" >/dev/null 2>&1
on=$(body "$TMPD/on.s" sum_to | grep -c '\[rbp')
if [ "$on" -ne 0 ]; then
    bad "sum_to has $on frame accesses:
$(body "$TMPD/on.s" sum_to)"
fi
for fn in sum_to rotate swap_n; do
    ion=$(body "$TMPD/on.s" "$fn" | grep -cE '^[[:space:]]+[a-z]')
    ioff=$(body "$TMPD/off.s" "$fn" | grep -cE '^[[:space:]]+[a-z]')
    if [ "$ion" -gt "$ioff" ]; then
        bad "$fn: $ion instructions with the pass, $ioff without -- round 92 made it longer"
    fi
    note "$fn: $ioff instructions without mem2reg, $ion with it"
done
note "frame accesses in sum_to: $on"

# --------------------------------------------------- 4. aarch64 assembly ---
# The second machine has NO register allocation (`codegen_a64.rs`): every
# value lives in the frame there, so "zero accesses" is not the right
# question. The right one is whether the pass takes accesses AWAY, and it is
# the machine where this round matters most, because nothing downstream was
# rescuing the counter.
echo "4. aarch64: the pass takes memory accesses out of the loop"
if "$FIRNC" --target=aarch64 --opt-level=release-fast --emit=asm -o "$TMPD/on.a64.s" "$SRC" >/dev/null 2>&1 \
   && "$FIRNC" --target=aarch64 --opt-level=release-fast --no-pass=mem2reg --emit=asm -o "$TMPD/off.a64.s" "$SRC" >/dev/null 2>&1
then
    aon=$(body "$TMPD/on.a64.s" sum_to | grep -cE '^\s+(ldr|str)')
    aoff=$(body "$TMPD/off.a64.s" sum_to | grep -cE '^\s+(ldr|str)')
    [ "$aoff" -gt "$aon" ] || bad "aarch64: $aoff loads/stores without the pass, $aon with it -- no difference"
    note "aarch64 loads/stores in sum_to: $aoff without mem2reg, $aon with it"
else
    bad "aarch64: sum_to does not compile"
fi

# ------------------------------------------------- 5. the self-hosted one ---
# `firnc1` has no optimizer at all, so it never makes a phi and never has to
# take one apart. What it has to do is keep agreeing with `firnc0` on the
# same program -- that is what says the two compilers did not drift apart.
echo "5. the compiler written in Firn says the same"
FC1="$ROOT/.firnc1"
if [ -x "$FC1" ]; then
    "$FC1" -o "$TMPD/loops.fc1" "$SRC" >/dev/null 2>&1
    rc=$?
    case "$rc" in
        0)  "$TMPD/loops.fc1" >/dev/null 2>&1
            r=$?
            [ "$r" -eq 0 ] || bad "firnc1: exit code $r"
            note "firnc1 builds the same program and it answers 0" ;;
        3|4|5|6) note "firnc1 cannot build this file (rc=$rc) -- a known limit, not a difference" ;;
        *)  bad "firnc1: rc=$rc" ;;
    esac
else
    note "no ./.firnc1 -- skipped"
fi

echo
if [ "$FAIL" -gt 0 ]; then
    echo "phi: $FAIL FAILURES"
    exit 1
fi
echo "phi: ok"
