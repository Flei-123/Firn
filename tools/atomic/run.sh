#!/usr/bin/env bash
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
FEHLER=0
melde() { echo "FEHLER: $1"; FEHLER=1; }

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

cat > "$W/nichtatom.fi" <<'EOF'
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
for stufe in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stufe%%:*}
    opt=${stufe#*:}
    if ! "$FIRNC" $opt --emit=asm -o "$W/atom_$name.s" "$W/atom.fi" 2>"$W/err"; then
        melde "firnc0/$name: Assembler-Ausgabe fehlgeschlagen"
        head -5 "$W/err"
        continue
    fi
    n=$(grep -c 'lock xadd qword ptr' "$W/atom_$name.s" || true)
    if [ "$n" -ne 4 ]; then
        melde "firnc0/$name: $n 'lock xadd' statt 4 (vier Aufrufstellen im Programm)"
    fi
    if ! "$FIRNC" $opt -o "$W/atom_$name" "$W/atom.fi" 2>"$W/err"; then
        melde "firnc0/$name: Bau fehlgeschlagen"
        continue
    fi
    set +e; "$W/atom_$name"; rc=$?; set -e
    [ "$rc" -eq 0 ] || melde "firnc0/$name: Programm liefert $rc statt 0"
    b=$(objdump -d "$W/atom_$name" | grep -c 'lock' || true)
    [ "$b" -ge 1 ] || melde "firnc0/$name: im Binary steht kein 'lock'"
done

# --- 2. counter-check: an ordinary += has NO lock --------------------------
"$FIRNC" --emit=asm -o "$W/nicht.s" "$W/nichtatom.fi" 2>/dev/null
if grep -q 'lock' "$W/nicht.s"; then
    melde "Gegenprobe: gewoehnliches '*p = *p + 7' erzeugt ein 'lock' — der Nachweis waere wertlos"
fi
"$FIRNC" -o "$W/nicht" "$W/nichtatom.fi" 2>/dev/null
set +e; "$W/nicht"; rc=$?; set -e
[ "$rc" -eq 0 ] || melde "Gegenprobe: Programm liefert $rc statt 0"

# --- 4. firnc1: the same instruction, octet-identical FIR -------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || melde "firnc1 liess sich nicht bauen"
fi
if [ ! -x "$FDUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FDUMP" -print -quit)" ]; then
    rm -f "$FDUMP"
    "$FIRNC" bin/firdump.fi -o "$FDUMP" >/dev/null || melde "firdump liess sich nicht bauen"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/atom.fi" -o "$W/atom1" >/dev/null 2>"$W/err1"; then
        set +e; "$W/atom1"; rc=$?; set -e
        [ "$rc" -eq 0 ] || melde "firnc1: Programm liefert $rc statt 0"
        b=$(objdump -d "$W/atom1" | grep -c 'lock' || true)
        [ "$b" -ge 4 ] || melde "firnc1: nur $b 'lock' im Binary (erwartet mindestens 4)"
    else
        melde "firnc1: Bau fehlgeschlagen"
        head -5 "$W/err1"
    fi
fi
if [ -x "$FDUMP" ]; then
    "$FIRNC" --emit=fir-raw "$W/atom.fi" > "$W/f0.txt" 2>/dev/null
    "$FDUMP" "$W/atom.fi" > "$W/f1.txt" 2>/dev/null || melde "firdump lieferte keine FIR"
    if ! cmp -s "$W/f0.txt" "$W/f1.txt"; then
        melde "FIR von firnc0 und firnc1 unterscheiden sich"
        diff "$W/f0.txt" "$W/f1.txt" | head -10
    fi
    grep -q 'atomadd.u64' "$W/f0.txt" || melde "FIR-Text ohne 'atomadd.u64'"
fi

if [ "$FEHLER" -ne 0 ]; then
    echo "ATOMAR: FEHLGESCHLAGEN"
    exit 1
fi
echo "ATOMAR: bestanden — 'lock xadd' in 3 Baustufen und in beiden Compilern, FIR oktettgleich, Gegenprobe ohne 'lock'"
exit 0
