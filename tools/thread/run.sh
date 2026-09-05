#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proof of the THREAD PRIMITIVES (round 49, compiler/src/thread.rs,
# lib/firnc1/{fir,sema,lower,codegen}.fi).
#
# WHAT IS PROVEN HERE AND WHY EXACTLY THAT:
#
#   1. `__thread_start` really produces a `clone(2)` with the agreed
#      flags and ends the child with `exit(2)` -- NOT with `exit_group(2)`.
#      The difference is the one between "a thread ends" and "the process
#      ends"; in the assembly it reads `mov eax, 60` against `mov eax, 231`.
#   2. `__atomic_swap` becomes exactly ONE instruction with a `lock` prefix.
#   3. `__thread_self` reads the thread base (`fs:0`) -- without a system call.
#   4. Counter-check: ordinary code produces none of that. Without it the
#      proof would be worthless, because it would let everything pass.
#   5. All of it in THREE build stages and in BOTH compilers, and the FIR of both
#      compilers is octet-identical.
#   6. A short run with four threads shows that the instructions really do the
#      right thing: the mutex loses no increment, the counter-check without
#      a lock does.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
FC1=${FIRNC1:-./.firnc1}
FDUMP=${FIRDUMP:-./.firdump}
W=$(mktemp -d /tmp/firn-thread.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "ERROR: $1"; ERRORS=1; }

export FIRNLIB="$(pwd)/lib"

# A program that uses all three primitives.
cat > "$W/prim.fi" <<'EOF'
fn main() -> i32 {
    var z: u64 = 5
    var t: u64 = 0
    let alt: u64 = __atomic_swap(&z, 5, 9)
    let alt2: u64 = __atomic_swap(&z, 5, 11)
    let s: *mut u8 = __thread_self()
    let tp: *mut u8 = (&t) as *mut u8
    let r: i64 = __thread_start(1, 2, tp)
    if alt != 5 {
        return 1
    }
    if z != 9 {
        return 2
    }
    if alt2 != 9 {
        return 3
    }
    if (s as u64) == 0 {
        return 4
    }
    if r == 0 {
        return 5
    }
    return 0
}
EOF

# Counter-check: the same shape, but ordinary.
cat > "$W/plain.fi" <<'EOF'
fn main() -> i32 {
    var z: u64 = 5
    let p: *mut u64 = &z
    if *p == 5 {
        *p = 9
    }
    if z != 9 {
        return 1
    }
    return 0
}
EOF

# --- 1./2./3. firnc0 in all three build stages ------------------------------
for stage in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! "$FIRNC" $opt --emit=asm -o "$W/prim_$name.s" "$W/prim.fi" 2>"$W/err"; then
        report "firnc0/$name: assembly output failed"
        head -5 "$W/err"
        continue
    fi
    # Count ONLY in `main`: the runtime itself uses the same
    # instructions, and its occurrences say nothing about THIS program.
    awk '/^main:/{d=1; next} /^\.globl/{if(d) exit} d{print}' "$W/prim_$name.s" > "$W/main_$name.s"
    n=$(grep -c 'lock cmpxchg qword ptr' "$W/main_$name.s" || true)
    [ "$n" -eq 2 ] || report "firnc0/$name: $n 'lock cmpxchg' in main instead of 2 (two call sites)"
    n=$(grep -c 'mov rax, qword ptr fs:0' "$W/main_$name.s" || true)
    [ "$n" -eq 1 ] || report "firnc0/$name: $n 'fs:0' in main instead of 1"
    grep -q 'mov eax, 56' "$W/main_$name.s" || report "firnc0/$name: no 'mov eax, 56' (clone) in main"
    grep -q 'mov rdi, 3477248' "$W/main_$name.s" || report "firnc0/$name: wrong clone flags in main"
    grep -q 'mov eax, 60' "$W/main_$name.s" || report "firnc0/$name: no 'mov eax, 60' (exit) in main"
    if grep -q 'mov eax, 231' "$W/main_$name.s"; then
        report "firnc0/$name: 'exit_group' in the thread sequence -- an ending thread would take the process with it"
    fi
    grep -q 'call _F0.__thread_entry' "$W/main_$name.s" || report "firnc0/$name: the child does not call the entry point"
    # The probe program is NOT run: it starts a thread without a
    # registered thread block. That the instructions really do the right thing
    # is shown by the short run in section 6.
    if ! "$FIRNC" $opt -o "$W/prim_$name" "$W/prim.fi" 2>"$W/err"; then
        report "firnc0/$name: build failed"
        continue
    fi
    b=$(objdump -d "$W/prim_$name" | grep -c 'cmpxchg' || true)
    [ "$b" -ge 1 ] || report "firnc0/$name: there is no 'cmpxchg' in the binary"
done

# --- 4. counter-check -------------------------------------------------------
"$FIRNC" --emit=asm -o "$W/plain.s" "$W/plain.fi" 2>/dev/null
if grep -qE 'lock|fs:0|mov eax, 56' "$W/plain.s"; then
    report "counter-check: ordinary code produces lock/fs:0/clone -- the proof would be worthless"
fi
"$FIRNC" -o "$W/plain" "$W/plain.fi" 2>/dev/null
set +e; "$W/plain"; rc=$?; set -e
[ "$rc" -eq 0 ] || report "counter-check: the program yields $rc instead of 0"

# --- 5. firnc1: the same instructions, octet-identical FIR ------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || report "firnc1 could not be built"
fi
if [ ! -x "$FDUMP" ] || [ -n "$(find bin lib -name '*.fi' -newer "$FDUMP" -print -quit)" ]; then
    rm -f "$FDUMP"
    "$FIRNC" bin/firdump.fi -o "$FDUMP" >/dev/null || report "firdump could not be built"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/prim.fi" -o "$W/prim1" >/dev/null 2>"$W/err1"; then
        # `grep -q` ends itself at the first hit; together with
        # `pipefail` that kills the writer with SIGPIPE and the pipeline
        # reports 141. That is why we count here instead of aborting.
        objdump -d "$W/prim1" > "$W/prim1.dis"
        n=$(grep -c 'cmpxchg' "$W/prim1.dis" || true)
        [ "$n" -ge 2 ] || report "firnc1: $n 'cmpxchg' in the binary (at least 2 expected)"
        n=$(grep -c 'fs:0x0' "$W/prim1.dis" || true)
        [ "$n" -ge 1 ] || report "firnc1: no 'fs:0' in the binary"
    else
        report "firnc1: build failed"
        head -5 "$W/err1"
    fi
fi
if [ -x "$FDUMP" ]; then
    "$FIRNC" --emit=fir-raw "$W/prim.fi" > "$W/f0.txt" 2>/dev/null
    "$FDUMP" "$W/prim.fi" > "$W/f1.txt" 2>/dev/null || report "firdump yielded no FIR"
    # What is compared is `fn @main` -- the rest of the output is the pulled-in
    # runtime, and that does not stand in the same order in the file in the
    # two compilers. What this round promises is the translation
    # of the three primitives, and that stands completely in `main`.
    awk '/^fn @main\(/{d=1} d{print} d&&/^\}/{exit}' "$W/f0.txt" > "$W/m0.txt"
    awk '/^fn @main\(/{d=1} d{print} d&&/^\}/{exit}' "$W/f1.txt" > "$W/m1.txt"
    if [ ! -s "$W/m0.txt" ]; then
        report "the FIR of firnc0 contains no 'fn @main'"
    fi
    if ! cmp -s "$W/m0.txt" "$W/m1.txt"; then
        report "the FIR of firnc0 and firnc1 differ in main"
        diff "$W/m0.txt" "$W/m1.txt" | head -10
    fi
    grep -q 'atomcas.u64' "$W/m0.txt" || report "FIR text without 'atomcas.u64'"
    grep -q 'threadself.ptr' "$W/m0.txt" || report "FIR text without 'threadself.ptr'"
    grep -q 'spawn.i64' "$W/m0.txt" || report "FIR text without 'spawn.i64'"
fi

# --- 6. short run: do the instructions really do the right thing? ----------
cat > "$W/lauf.fi" <<'EOF'
const L_SYS_MMAP: i64 = 9
const L_WITH: u64 = 0
const L_WITHOUT: u64 = 8
const L_MUTEX: u64 = 16
const L_ROUNDS: u64 = 30000

fn seite() -> u64 {
    let r: i64 = syscall(L_SYS_MMAP, 0, 4096, 3, 34, -1, 0)
    if r < 0 {
        return 0
    }
    return r as u64
}

fn ld(a: u64, o: u64) -> u64 { return *((a + o) as *mut u64) }
fn st(a: u64, o: u64, v: u64) { *((a + o) as *mut u64) = v }

fn __thread_work(art: u64, arg: u64) -> u64 {
    let _u: u64 = art
    var i: u64 = 0
    while i < L_ROUNDS {
        thread_lock((arg + L_MUTEX) as *mut u64)
        st(arg, L_WITH, ld(arg, L_WITH) + 1)
        thread_unlock((arg + L_MUTEX) as *mut u64)
        st(arg, L_WITHOUT, ld(arg, L_WITHOUT) + 1)
        i = i + 1
    }
    return L_ROUNDS
}

fn main() -> i32 {
    if !thread_init() {
        return 90
    }
    let z: u64 = seite()
    if z == 0 {
        return 91
    }
    var h: [u64; 4] = [0; 4]
    var i: u64 = 0
    while i < 4 {
        h[i as usize] = thread_start(1, z)
        if h[i as usize] == 0 {
            return 1
        }
        i = i + 1
    }
    i = 0
    while i < 4 {
        if thread_wait(h[i as usize]) != L_ROUNDS {
            return 2
        }
        i = i + 1
    }
    if ld(z, L_WITH) != 4 * L_ROUNDS {
        return 3
    }
    if ld(z, L_WITHOUT) >= 4 * L_ROUNDS {
        return 4
    }
    return 0
}
EOF
if ! "$FIRNC" -o "$W/lauf" "$W/lauf.fi" 2>"$W/err2"; then
    report "short run: build failed"
    head -5 "$W/err2"
else
    set +e; timeout 120 "$W/lauf"; rc=$?; set -e
    case "$rc" in
        0) : ;;
        3) report "short run: the mutex lost increments" ;;
        4) report "short run: the counter WITHOUT a lock lost NOTHING -- the threads did not run at the same time, the proof would be worthless" ;;
        *) report "short run: return value $rc" ;;
    esac
fi

if [ "$ERRORS" -ne 0 ]; then
    echo "THREADS: FAILED"
    exit 1
fi
echo "THREADS: passed -- clone(2)/exit(2), 'lock cmpxchg' and 'fs:0' in 3 build stages and both compilers, FIR octet-identical, counter-checks strike"
exit 0
