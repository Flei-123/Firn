#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proof of the ATOMIC PRIMITIVE (round 47, compiler/src/atomic.rs,
# lib/firnc1/{fir,sema,lower,codegen}.fi).
#
# WHY THIS PROOF AND NOT A TWO-THREAD RUN: Firn has no threads in stage 0
# (SPEC 7). A race cannot be provoked, and a claim of "thread safe" would
# be uncovered. What CAN be proven is what matters:
# that `__atomic_add` becomes exactly ONE
# machine instruction with a `lock` prefix and that an ordinary `+= 1`
# does NOT. Exactly that is what this tool checks -- on the emitted assembly
# and on the finished binary, in BOTH compilers.
#
# What is checked:
#   1. `__atomic_add` produces `lock xadd qword ptr [..], ..` -- exactly
#      once per call site, in all three build stages.
#   2. An ordinary `*p = *p + 7` produces NO `lock` (otherwise the
#      proof would be worthless, because it would let everything pass).
#   3. The return value is the OLD value, and the counter is exactly right
#      after 100,000 increments and 100,000 decrements.
#   4. firnc1 (the compiler in Firn) produces the same instruction, and its
#      FIR is octet-identical with the one of firnc0.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
FC1=${FIRNC1:-./.firnc1}
FDUMP=${FIRDUMP:-./.firdump}
W=$(mktemp -d /tmp/firn-atomar.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "ERROR: $1"; ERRORS=1; }

export FIRNLIB="$(pwd)/lib"

cat > "$W/atom.fi" <<'EOF'
fn main() -> i32 {
    var z: u64 = 5
    let alt: u64 = __atomic_add(&z, 7)
    if alt != 5 {
        return 1
    }
    if z != 12 {
        return 2
    }
    var i: u64 = 0
    while i < 100000 {
        __atomic_add(&z, 1)
        i = i + 1
    }
    if z != 100012 {
        return 3
    }
    var j: u64 = 0
    while j < 100000 {
        __atomic_add(&z, 18446744073709551615)
        j = j + 1
    }
    if z != 12 {
        return 4
    }
    // (The return value is bound: an integer literal next to a
    // call only gets its type through the probe, and the probe does not know
    // this primitive -- the same limitation as with the ct primitives.)
    let alt2: u64 = __atomic_add(&z, 30)
    if alt2 != 12 {
        return 5
    }
    if z != 42 {
        return 6
    }
    return 0
}
EOF

cat > "$W/nonatomic.fi" <<'EOF'
fn main() -> i32 {
    var z: u64 = 5
    let p: *mut u64 = &z
    *p = *p + 7
    if z != 12 {
        return 1
    }
    return 0
}
EOF

# --- 1./3. firnc0: instruction and behaviour, in all three build stages -----
for stage in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! "$FIRNC" $opt --emit=asm -o "$W/atom_$name.s" "$W/atom.fi" 2>"$W/err"; then
        report "firnc0/$name: assembly output failed"
        head -5 "$W/err"
        continue
    fi
    n=$(grep -c 'lock xadd qword ptr' "$W/atom_$name.s" || true)
    if [ "$n" -ne 4 ]; then
        report "firnc0/$name: $n 'lock xadd' instead of 4 (four call sites in the program)"
    fi
    if ! "$FIRNC" $opt -o "$W/atom_$name" "$W/atom.fi" 2>"$W/err"; then
        report "firnc0/$name: build failed"
        continue
    fi
    set +e; "$W/atom_$name"; rc=$?; set -e
    [ "$rc" -eq 0 ] || report "firnc0/$name: the program yields $rc instead of 0"
    b=$(objdump -d "$W/atom_$name" | grep -c 'lock' || true)
    [ "$b" -ge 1 ] || report "firnc0/$name: there is no 'lock' in the binary"
done

# --- 2. counter-check: an ordinary += has NO lock --------------------------
"$FIRNC" --emit=asm -o "$W/plain.s" "$W/nonatomic.fi" 2>/dev/null
if grep -q 'lock' "$W/plain.s"; then
    report "counter-check: an ordinary '*p = *p + 7' produces a 'lock' -- the proof would be worthless"
fi
"$FIRNC" -o "$W/plain" "$W/nonatomic.fi" 2>/dev/null
set +e; "$W/plain"; rc=$?; set -e
[ "$rc" -eq 0 ] || report "counter-check: the program yields $rc instead of 0"

# --- 4. firnc1: the same instruction, octet-identical FIR -------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || report "firnc1 could not be built"
fi
if [ ! -x "$FDUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FDUMP" -print -quit)" ]; then
    rm -f "$FDUMP"
    "$FIRNC" bin/firdump.fi -o "$FDUMP" >/dev/null || report "firdump could not be built"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/atom.fi" -o "$W/atom1" >/dev/null 2>"$W/err1"; then
        set +e; "$W/atom1"; rc=$?; set -e
        [ "$rc" -eq 0 ] || report "firnc1: the program yields $rc instead of 0"
        b=$(objdump -d "$W/atom1" | grep -c 'lock' || true)
        [ "$b" -ge 4 ] || report "firnc1: only $b 'lock' in the binary (at least 4 expected)"
    else
        report "firnc1: build failed"
        head -5 "$W/err1"
    fi
fi
if [ -x "$FDUMP" ]; then
    "$FIRNC" --emit=fir-raw "$W/atom.fi" > "$W/f0.txt" 2>/dev/null
    "$FDUMP" "$W/atom.fi" > "$W/f1.txt" 2>/dev/null || report "firdump yielded no FIR"
    if ! cmp -s "$W/f0.txt" "$W/f1.txt"; then
        report "the FIR of firnc0 and firnc1 differ"
        diff "$W/f0.txt" "$W/f1.txt" | head -10
    fi
    grep -q 'atomadd.u64' "$W/f0.txt" || report "FIR text without 'atomadd.u64'"
fi

if [ "$ERRORS" -ne 0 ]; then
    echo "ATOMIC: FAILED"
    exit 1
fi
echo "ATOMIC: passed -- 'lock xadd' in 3 build stages and in both compilers, FIR octet-identical, counter-check without 'lock'"
exit 0
