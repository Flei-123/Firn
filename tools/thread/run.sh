#!/usr/bin/env bash
# Nachweis der FADEN-PRIMITIVE (Runde 49, compiler/src/thread.rs,
# lib/firnc1/{fir,sema,lower,codegen}.fi).
#
# WAS HIER BELEGT WIRD UND WARUM GERADE DAS:
#
#   1. `__faden_starten` erzeugt wirklich einen `clone(2)` mit den vereinbarten
#      Merkern und beendet das Kind mit `exit(2)` — NICHT mit `exit_group(2)`.
#      Der Unterschied ist der zwischen „ein Faden endet" und „der Prozess
#      endet"; im Assembler steht er als `mov eax, 60` gegen `mov eax, 231`.
#   2. `__atomar_tauschen` wird zu genau EINER Instruktion mit `lock`-Praefix.
#   3. `__faden_selbst` liest die Fadenbasis (`fs:0`) — ohne Systemaufruf.
#   4. Gegenprobe: gewoehnlicher Code erzeugt nichts davon. Ohne sie waere
#      der Nachweis wertlos, weil er alles bestehen liesse.
#   5. Alles in DREI Baustufen und in BEIDEN Compilern, und die FIR beider
#      Compiler ist oktettgleich.
#   6. Ein kurzer Lauf mit vier Faeden zeigt, dass die Instruktionen auch das
#      Richtige tun: der Mutex verliert keine Erhoehung, die Gegenprobe ohne
#      Sperre schon.
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

# Ein Programm, das alle drei Primitive benutzt.
cat > "$W/prim.fi" <<'EOF'
fn main() -> i32 {
    var z: u64 = 5
    var t: u64 = 0
    let alt: u64 = __atomar_tauschen(&z, 5, 9)
    let alt2: u64 = __atomar_tauschen(&z, 5, 11)
    let s: *mut u8 = __faden_selbst()
    let tp: *mut u8 = (&t) as *mut u8
    let r: i64 = __faden_starten(1, 2, tp)
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

# Gegenprobe: dieselbe Form, aber gewoehnlich.
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

# --- 1./2./3. firnc0 in allen drei Baustufen --------------------------------
for stufe in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stufe%%:*}
    opt=${stufe#*:}
    if ! "$FIRNC" $opt --emit=asm -o "$W/prim_$name.s" "$W/prim.fi" 2>"$W/err"; then
        melde "firnc0/$name: Assembler-Ausgabe fehlgeschlagen"
        head -5 "$W/err"
        continue
    fi
    # NUR in `main` zaehlen: die Laufzeit selbst benutzt dieselben
    # Instruktionen, und ihre Vorkommen sagen ueber DIESES Programm nichts.
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
    grep -q 'call _F0.__faden_einstieg' "$W/main_$name.s" || melde "firnc0/$name: das Kind ruft den Einstieg nicht"
    # Das Probeprogramm wird NICHT ausgefuehrt: es startet einen Faden ohne
    # angemeldeten Fadenblock. Dass die Instruktionen auch das Richtige tun,
    # zeigt der Kurzlauf in Abschnitt 6.
    if ! "$FIRNC" $opt -o "$W/prim_$name" "$W/prim.fi" 2>"$W/err"; then
        melde "firnc0/$name: Bau fehlgeschlagen"
        continue
    fi
    b=$(objdump -d "$W/prim_$name" | grep -c 'cmpxchg' || true)
    [ "$b" -ge 1 ] || melde "firnc0/$name: im Binary steht kein 'cmpxchg'"
done

# --- 4. Gegenprobe ----------------------------------------------------------
"$FIRNC" --emit=asm -o "$W/nicht.s" "$W/nicht.fi" 2>/dev/null
if grep -qE 'lock|fs:0|mov eax, 56' "$W/nicht.s"; then
    melde "Gegenprobe: gewoehnlicher Code erzeugt lock/fs:0/clone — der Nachweis waere wertlos"
fi
"$FIRNC" -o "$W/nicht" "$W/nicht.fi" 2>/dev/null
set +e; "$W/nicht"; rc=$?; set -e
[ "$rc" -eq 0 ] || melde "Gegenprobe: Programm liefert $rc statt 0"

# --- 5. firnc1: dieselben Instruktionen, oktettgleiche FIR ------------------
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
        # `grep -q` beendet sich beim ersten Treffer; zusammen mit
        # `pipefail` toetet das den Schreiber mit SIGPIPE und die Pipeline
        # meldet 141. Deshalb hier zaehlen statt abbrechen.
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
    # Verglichen wird `fn @main` — der Rest der Ausgabe ist die eingezogene
    # Laufzeit, und die steht in den beiden Compilern nicht in derselben
    # Reihenfolge in der Datei. Was diese Runde zusagt, ist die Uebersetzung
    # der drei Primitive, und die steht vollstaendig in `main`.
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
    grep -q 'fadenselbst.ptr' "$W/m0.txt" || melde "FIR-Text ohne 'fadenselbst.ptr'"
    grep -q 'spawn.i64' "$W/m0.txt" || melde "FIR-Text ohne 'spawn.i64'"
fi

# --- 6. Kurzlauf: tun die Instruktionen auch das Richtige? ------------------
cat > "$W/lauf.fi" <<'EOF'
const L_SYS_MMAP: i64 = 9
const L_MIT: u64 = 0
const L_OHNE: u64 = 8
const L_MUTEX: u64 = 16
const L_RUNDEN: u64 = 30000

fn seite() -> u64 {
    let r: i64 = syscall(L_SYS_MMAP, 0, 4096, 3, 34, -1, 0)
    if r < 0 {
        return 0
    }
    return r as u64
}

fn ld(a: u64, o: u64) -> u64 { return *((a + o) as *mut u64) }
fn st(a: u64, o: u64, v: u64) { *((a + o) as *mut u64) = v }

fn __faden_arbeit(art: u64, arg: u64) -> u64 {
    let _u: u64 = art
    var i: u64 = 0
    while i < L_RUNDEN {
        faden_sperren((arg + L_MUTEX) as *mut u64)
        st(arg, L_MIT, ld(arg, L_MIT) + 1)
        faden_entsperren((arg + L_MUTEX) as *mut u64)
        st(arg, L_OHNE, ld(arg, L_OHNE) + 1)
        i = i + 1
    }
    return L_RUNDEN
}

fn main() -> i32 {
    if !faden_init() {
        return 90
    }
    let z: u64 = seite()
    if z == 0 {
        return 91
    }
    var h: [u64; 4] = [0; 4]
    var i: u64 = 0
    while i < 4 {
        h[i as usize] = faden_starten(1, z)
        if h[i as usize] == 0 {
            return 1
        }
        i = i + 1
    }
    i = 0
    while i < 4 {
        if faden_warten(h[i as usize]) != L_RUNDEN {
            return 2
        }
        i = i + 1
    }
    if ld(z, L_MIT) != 4 * L_RUNDEN {
        return 3
    }
    if ld(z, L_OHNE) >= 4 * L_RUNDEN {
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
