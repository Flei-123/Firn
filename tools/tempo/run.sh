#!/bin/bash
# SPDX-License-Identifier: GPL-2.0-only
#
# tools/tempo/run.sh -- ROUND TEMPO: the proof that `-j` changes nothing.
#
# `-j` makes the optimizer run on several cores and cuts the assembly into
# parts for `as`.  Both are only allowed if the RESULT is the same.  This
# script checks exactly that, on the largest program in the repository:
#
#   1. the emitted assembly of `-j1` and `-j<n>` is compared OCTET FOR
#      OCTET.  The optimizer is per function and the functions cannot see
#      each other, so the order they are worked in must not show.
#   2. the linked binary of the split `as` path is compared against the
#      single-`as` path in its `.text` -- the split adds symbols to
#      `.symtab` (see asmsplit.rs, "What that costs"), the program text
#      itself has to be identical.
#
# Usage: bash tools/tempo/run.sh [firnc]
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${1:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
SRC=$ROOT/bin/firnc1.fi
N=$(nproc)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail=0

echo "== ROUND TEMPO: -j must not change the result =="
echo "   compiler: $FIRNC"
echo "   source:   $SRC   (cores: $N)"

echo "-- 1. assembly of -j1 vs -j$N --"
# `--emit=asm` writes to the OUTPUT file, so `-o` is what decides where it
# lands -- without it the assembly would overwrite `bin/firnc1`.
"$FIRNC" --emit=asm -j1 -o "$T/j1.s" "$SRC" 2> "$T/j1.err" || { cat "$T/j1.err"; exit 1; }
"$FIRNC" --emit=asm "-j$N" -o "$T/jn.s" "$SRC" 2> "$T/jn.err" || { cat "$T/jn.err"; exit 1; }
if cmp -s "$T/j1.s" "$T/jn.s"; then
    echo "   OK   $(wc -l < "$T/j1.s") lines, identical octet for octet"
else
    echo "   FAIL the optimizer produced different assembly under -j$N"
    cmp "$T/j1.s" "$T/jn.s" | head -3
    fail=1
fi

echo "-- 2. binary: one 'as' vs split 'as' --"
"$FIRNC" -j1 -o "$T/a.bin" "$SRC" > /dev/null 2>&1 || { echo "   FAIL -j1 build"; exit 1; }
"$FIRNC" "-j$N" -o "$T/b.bin" "$SRC" > /dev/null 2>&1 || { echo "   FAIL -j$N build"; exit 1; }
objcopy -O binary --only-section=.text "$T/a.bin" "$T/a.text" 2>/dev/null
objcopy -O binary --only-section=.text "$T/b.bin" "$T/b.text" 2>/dev/null
if cmp -s "$T/a.text" "$T/b.text"; then
    echo "   OK   .text identical ($(stat -c%s "$T/a.text") octets)"
else
    echo "   FAIL the split 'as' path produced different program text"
    fail=1
fi

echo "-- 3. and the split binary still runs --"
if file "$T/b.bin" | grep -q 'ELF 64-bit.*executable' && "$T/b.bin" tests/001_return_const.fi -o "$T/x.bin" > /dev/null 2>&1; then
    echo "   OK   it is an ELF executable and it compiles a program"
else
    echo "   FAIL the binary built with -j$N does not run"
    fail=1
fi

[ $fail -eq 0 ] && echo "== all good ==" || echo "== FAILURES =="
exit $fail
