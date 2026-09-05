#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proof of STATIC DISPATCH with interface bounds (round 50).
#
# The promise is: `fn f[T: I](x: *T)` calls `x.m()` DIRECTLY as soon as the
# instantiation is known -- no detour over a method table, no
# indirect jump. A promise of this kind cannot be proven with the wall clock
# (which scatters), only on the EMITTED CODE. Exactly that is what this
# tool does.
#
# Two programs, the same work:
#
#   static.fi     fn count[T: Order](a: *T, ...)    bound, instantiation
#   dynamic.fi    fn count_dyn(a: dyn OrderD, ...)  method table
#
# What is checked:
#   1. In `static` there is NO indirect call (`call <register>`) --
#      and without the optimiser a call by name `call ... Dot__less` instead.
#   2. In `dynamic` there is at least one. (Without this counter-check the
#      test would pass even if it measured nothing at all.)
#   3. The same in the FIR: `static` contains neither `calli` nor `vtab`,
#      `dynamic` contains both.
#   4. `static` does not load a method table address either (`lea ... .L__iface`).
#   5. Both compilers (firnc0 and firnc1) behave the same.
#   6. Instruction counts with callgrind -- deterministic, not the clock.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
FC1=${FIRNC1:-./.firnc1}
W=$(mktemp -d /tmp/firn-bounds.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "ERROR: $1"; ERRORS=1; }

export FIRNLIB="$(pwd)/lib"

# How many indirect calls are in this assembly file?
indirekte() {
    grep -cE '^[[:space:]]*call[[:space:]]+(\*|r[a-z0-9]+$)' "$1" || true
}

N=${BOUNDS_N:-2000000}

cat > "$W/static.fi" <<EOF
interface Order {
    fn less(*self, b: *Self) -> bool
}

struct Dot { x: i64 }

impl Order for Dot {
    fn less(*self, b: *Dot) -> bool { return (*self).x < (*b).x }
}

fn count[T: Order](a: *T, b: *T, n: i64) -> i64 {
    var i: i64 = 0
    var s: i64 = 0
    while i < n {
        if a.less(b) {
            s = s + 1
        }
        i = i + 1
    }
    return s
}

fn main() -> i32 {
    var p: Dot = Dot{ x: 1 }
    var q: Dot = Dot{ x: 2 }
    if count[Dot](&p, &q, $N) != $N {
        return 1
    }
    return 0
}
EOF

cat > "$W/dynamic.fi" <<EOF
interface OrderD {
    fn less(*self, b: *Dot) -> bool
}

struct Dot { x: i64 }

impl OrderD for Dot {
    fn less(*self, b: *Dot) -> bool { return (*self).x < (*b).x }
}

fn count_dyn(a: dyn OrderD, b: *Dot, n: i64) -> i64 {
    var i: i64 = 0
    var s: i64 = 0
    while i < n {
        if a.less(b) {
            s = s + 1
        }
        i = i + 1
    }
    return s
}

fn main() -> i32 {
    var p: Dot = Dot{ x: 1 }
    var q: Dot = Dot{ x: 2 }
    let d: dyn OrderD = (&p) as dyn OrderD
    if count_dyn(d, &q, $N) != $N {
        return 1
    }
    return 0
}
EOF

# --- 1./2./4. firnc0: assembly, in all three build stages ------------------
for stage in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}
    "$FIRNC" $opt --emit=asm -o "$W/s_$name.s" "$W/static.fi" 2>"$W/err" \
        || { report "firnc0/$name: the static version could not be compiled"; head -5 "$W/err"; continue; }
    "$FIRNC" $opt --emit=asm -o "$W/d_$name.s" "$W/dynamic.fi" 2>"$W/err" \
        || { report "firnc0/$name: the dynamic version could not be compiled"; head -5 "$W/err"; continue; }
    si=$(indirekte "$W/s_$name.s")
    di=$(indirekte "$W/d_$name.s")
    [ "$si" -eq 0 ] || report "firnc0/$name: the bound version has $si indirect calls (0 expected)"
    [ "$di" -ge 1 ] || report "firnc0/$name: the dyn version has no indirect call -- the counter-check measures nothing"
    if grep -qE 'lea.*\.L__iface' "$W/s_$name.s"; then
        report "firnc0/$name: the bound version loads a method table address"
    fi
    grep -qE 'lea.*\.L__iface' "$W/d_$name.s" \
        || report "firnc0/$name: the dyn version loads NO method table address"
done
# The call by name is visible without the optimiser -- WITH the optimiser
# it disappears completely, and that is the real gain (see the measurement).
grep -qE '^[[:space:]]*call[[:space:]]+\S*Dot__less' "$W/s_no-opt.s" \
    || report "firnc0/no-opt: no call by name 'Dot__less' in the bound version"

# --- 3. the same statement in the FIR --------------------------------------
"$FIRNC" --emit=fir-raw "$W/static.fi"  > "$W/s.fir" 2>/dev/null
"$FIRNC" --emit=fir-raw "$W/dynamic.fi" > "$W/d.fir" 2>/dev/null
for word in calli vtab; do
    n=$(grep -c "$word" "$W/s.fir" || true)
    [ "$n" -eq 0 ] || report "the FIR of the bound version contains '$word' $n times"
    n=$(grep -c "$word" "$W/d.fir" || true)
    [ "$n" -ge 1 ] || report "the FIR of the dyn version contains no '$word'"
done

# --- 5. firnc1 says the same -----------------------------------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || report "firnc1 could not be built"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/static.fi" -o "$W/s1.bin" >/dev/null 2>"$W/e1"; then
        si=$(indirekte "$W/s1.bin.s")
        [ "$si" -eq 0 ] || report "firnc1: the bound version has $si indirect calls"
        grep -qE '^[[:space:]]*call[[:space:]]+\S*Dot__less' "$W/s1.bin.s" \
            || report "firnc1: no call by name 'Dot__less'"
        set +e; "$W/s1.bin"; rc=$?; set -e
        [ "$rc" -eq 0 ] || report "firnc1: the bound version yields $rc instead of 0"
    else
        report "firnc1: the static version could not be compiled"
        head -5 "$W/e1"
    fi
    if "$FC1" "$W/dynamic.fi" -o "$W/d1.bin" >/dev/null 2>"$W/e2"; then
        di=$(indirekte "$W/d1.bin.s")
        [ "$di" -ge 1 ] || report "firnc1: the dyn version has no indirect call"
        set +e; "$W/d1.bin"; rc=$?; set -e
        [ "$rc" -eq 0 ] || report "firnc1: the dyn version yields $rc instead of 0"
    else
        report "firnc1: the dynamic version could not be compiled"
        head -5 "$W/e2"
    fi
fi

# --- 6. instructions (callgrind) -------------------------------------------
measure() {   # $1 = binary -> instructions in total
    valgrind --tool=callgrind --callgrind-out-file=/dev/null "$1" 2>&1 \
        | sed -n 's/.*I *refs: *//p' | tr -d ', '
}
if command -v valgrind >/dev/null 2>&1 && [ "${BOUNDS_MEASURE:-1}" = 1 ]; then
    for stage in "release-fast:" "no-opt:--no-opt"; do
        name=${stage%%:*}
        opt=${stage#*:}
        "$FIRNC" $opt -o "$W/s_$name" "$W/static.fi"  2>/dev/null
        "$FIRNC" $opt -o "$W/d_$name" "$W/dynamic.fi" 2>/dev/null
        a=$(measure "$W/s_$name")
        b=$(measure "$W/d_$name")
        if [ -n "$a" ] && [ -n "$b" ]; then
            echo "MEASUREMENT[$name]: bound $a  dyn $b  per run: $((a / N)) against $((b / N))"
        fi
    done
else
    echo "MEASUREMENT: skipped (no valgrind or BOUNDS_MEASURE=0)"
fi

if [ "$ERRORS" -ne 0 ]; then
    echo "BOUNDS: FAILED"
    exit 1
fi
echo "BOUNDS: passed -- 0 indirect calls under the bound, >=1 under 'dyn', in 3 build stages and in both compilers"
exit 0
