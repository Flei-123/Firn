#!/usr/bin/env bash
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
#   6. A short run with four threads shows that the instructions also do the
#      right thing: the mutex loses no increment, the counter-check without
#      a lock does.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
FC1=${FIRNC1:-./.firnc1}
FDUMP=${FIRDUMP:-./.firdump}
W=$(mktemp -d /tmp/firn-faden.XXXXXX)
trap 'rm -rf "$W"' EXIT
FEHLER=0
melde() { echo "FEHLER: $1"; FEHLER=1; }

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
cat > "$W/nicht.fi" <<'EOF'
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
for stufe in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stufe%%:*}
    opt=${stufe#*:}
    if ! "$FIRNC" $opt --emit=asm -o "$W/prim_$name.s" "$W/prim.fi" 2>"$W/err"; then
        melde "firnc0/$name: Assembler-Ausgabe fehlgeschlagen"
        head -5 "$W/err"
        continue
    fi
    # Count ONLY in `main`: the runtime itself uses the same
    # instructions, and its occurrences say nothing about THIS program.
    awk '/^main:/{d=1; next} /^\.globl/{if(d) exit} d{print}' "$W/prim_$name.s" > "$W/main_$name.s"
    n=$(grep -c 'lock cmpxchg qword ptr' "$W/main_$name.s" || true)
    [ "$n" -eq 2 ] || melde "firnc0/$name: $n 'lock cmpxchg' in main statt 2 (zwei Aufrufstellen)"
    n=$(grep -c 'mov rax, qword ptr fs:0' "$W/main_$name.s" || true)
    [ "$n" -eq 1 ] || melde "firnc0/$name: $n 'fs:0' in main statt 1"
    grep -q 'mov eax, 56' "$W/main_$name.s" || melde "firnc0/$name: kein 'mov eax, 56' (clone) in main"
    grep -q 'mov rdi, 3477248' "$W/main_$name.s" || melde "firnc0/$name: falsche clone-Merker in main"
    grep -q 'mov eax, 60' "$W/main_$name.s" || melde "firnc0/$name: kein 'mov eax, 60' (exit) in main"
    if grep -q 'mov eax, 231' "$W/main_$name.s"; then
        melde "firnc0/$name: 'exit_group' in der Faden-Folge — ein endender Faden naehme den Prozess mit"
    fi
    grep -q 'call _F0.__thread_entry' "$W/main_$name.s" || melde "firnc0/$name: das Kind ruft den Einstieg nicht"
    # The probe program is NOT run: it starts a thread without a
    # registered thread block. That the instructions also do the right thing
    # is shown by the short run in section 6.
    if ! "$FIRNC" $opt -o "$W/prim_$name" "$W/prim.fi" 2>"$W/err"; then
        melde "firnc0/$name: Bau fehlgeschlagen"
        continue
    fi
    b=$(objdump -d "$W/prim_$name" | grep -c 'cmpxchg' || true)
    [ "$b" -ge 1 ] || melde "firnc0/$name: im Binary steht kein 'cmpxchg'"
done

# --- 4. counter-check -------------------------------------------------------
"$FIRNC" --emit=asm -o "$W/nicht.s" "$W/nicht.fi" 2>/dev/null
if grep -qE 'lock|fs:0|mov eax, 56' "$W/nicht.s"; then
    melde "Gegenprobe: gewoehnlicher Code erzeugt lock/fs:0/clone — der Nachweis waere wertlos"
fi
"$FIRNC" -o "$W/nicht" "$W/nicht.fi" 2>/dev/null
set +e; "$W/nicht"; rc=$?; set -e
[ "$rc" -eq 0 ] || melde "Gegenprobe: Programm liefert $rc statt 0"

# --- 5. firnc1: the same instructions, octet-identical FIR ------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || melde "firnc1 liess sich nicht bauen"
fi
if [ ! -x "$FDUMP" ] || [ -n "$(find bin lib -name '*.fi' -newer "$FDUMP" -print -quit)" ]; then
    rm -f "$FDUMP"
    "$FIRNC" bin/firdump.fi -o "$FDUMP" >/dev/null || melde "firdump liess sich nicht bauen"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/prim.fi" -o "$W/prim1" >/dev/null 2>"$W/err1"; then
        # `grep -q` ends itself at the first hit; together with
        # `pipefail` that kills the writer with SIGPIPE and the pipeline
        # reports 141. That is why we count here instead of aborting.
        objdump -d "$W/prim1" > "$W/prim1.dis"
        n=$(grep -c 'cmpxchg' "$W/prim1.dis" || true)
        [ "$n" -ge 2 ] || melde "firnc1: $n 'cmpxchg' im Binary (erwartet mindestens 2)"
        n=$(grep -c 'fs:0x0' "$W/prim1.dis" || true)
        [ "$n" -ge 1 ] || melde "firnc1: kein 'fs:0' im Binary"
    else
        melde "firnc1: Bau fehlgeschlagen"
        head -5 "$W/err1"
    fi
fi
if [ -x "$FDUMP" ]; then
    "$FIRNC" --emit=fir-raw "$W/prim.fi" > "$W/f0.txt" 2>/dev/null
    "$FDUMP" "$W/prim.fi" > "$W/f1.txt" 2>/dev/null || melde "firdump lieferte keine FIR"
    # What is compared is `fn @main` -- the rest of the output is the pulled-in
    # runtime, and that does not stand in the same order in the file in the
    # two compilers. What this round promises is the translation
    # of the three primitives, and that stands completely in `main`.
    awk '/^fn @main\(/{d=1} d{print} d&&/^\}/{exit}' "$W/f0.txt" > "$W/m0.txt"
    awk '/^fn @main\(/{d=1} d{print} d&&/^\}/{exit}' "$W/f1.txt" > "$W/m1.txt"
    if [ ! -s "$W/m0.txt" ]; then
        melde "FIR von firnc0 enthaelt kein 'fn @main'"
    fi
    if ! cmp -s "$W/m0.txt" "$W/m1.txt"; then
        melde "FIR von firnc0 und firnc1 unterscheiden sich in main"
        diff "$W/m0.txt" "$W/m1.txt" | head -10
    fi
    grep -q 'atomcas.u64' "$W/m0.txt" || melde "FIR-Text ohne 'atomcas.u64'"
    grep -q 'threadself.ptr' "$W/m0.txt" || melde "FIR-Text ohne 'threadself.ptr'"
    grep -q 'spawn.i64' "$W/m0.txt" || melde "FIR-Text ohne 'spawn.i64'"
fi

# --- 6. short run: do the instructions also do the right thing? -------------
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
    melde "Kurzlauf: Bau fehlgeschlagen"
    head -5 "$W/err2"
else
    set +e; timeout 120 "$W/lauf"; rc=$?; set -e
    case "$rc" in
        0) : ;;
        3) melde "Kurzlauf: der Mutex hat Erhoehungen verloren" ;;
        4) melde "Kurzlauf: der Zaehler OHNE Sperre hat NICHTS verloren — die Faeden liefen nicht gleichzeitig, der Nachweis waere wertlos" ;;
        *) melde "Kurzlauf: Rueckgabe $rc" ;;
    esac
fi

if [ "$FEHLER" -ne 0 ]; then
    echo "FAEDEN: FEHLGESCHLAGEN"
    exit 1
fi
echo "FAEDEN: bestanden — clone(2)/exit(2), 'lock cmpxchg' und 'fs:0' in 3 Baustufen und beiden Compilern, FIR oktettgleich, Gegenproben schlagen an"
exit 0
