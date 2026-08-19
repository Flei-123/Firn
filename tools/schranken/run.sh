#!/usr/bin/env bash
# Nachweis des STATISCHEN VERSANDS bei Schnittstellenschranken (Runde 50).
#
# Die Zusage lautet: `fn f[T: I](x: *T)` ruft `x.m()` DIREKT auf, sobald die
# Auspraegung bekannt ist — kein Umweg ueber eine Methodentafel, kein
# indirekter Sprung. Eine Zusage dieser Art laesst sich nicht mit der Wanduhr
# belegen (die streut), sondern nur am ERZEUGTEN CODE. Genau das tut dieses
# Werkzeug.
#
# Zwei Programme, dieselbe Arbeit:
#
#   statisch.fi   fn zaehle[T: Ordnung](a: *T, …)     Schranke, Auspraegung
#   dynamisch.fi  fn zaehle_dyn(a: dyn OrdnungD, …)   Methodentafel
#
# Geprueft wird:
#   1. In `statisch` gibt es KEINEN indirekten Aufruf (`call <register>`) —
#      und ohne Optimierer dafuer einen namentlichen `call … Punkt__kleiner`.
#   2. In `dynamisch` gibt es mindestens einen. (Ohne diese Gegenprobe wuerde
#      der Test auch dann bestehen, wenn er gar nichts misst.)
#   3. Dasselbe in der FIR: `statisch` enthaelt weder `calli` noch `vtab`,
#      `dynamisch` beides.
#   4. `statisch` laedt auch keine Methodentafel-Adresse (`lea … .L__iface`).
#   5. Beide Compiler (firnc0 und firnc1) verhalten sich gleich.
#   6. Instruktionszahlen mit callgrind — deterministisch, nicht die Uhr.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
FC1=${FIRNC1:-./.firnc1}
W=$(mktemp -d /tmp/firn-schranken.XXXXXX)
trap 'rm -rf "$W"' EXIT
FEHLER=0
melde() { echo "FEHLER: $1"; FEHLER=1; }

export FIRNLIB="$(pwd)/lib"

# Wie viele indirekte Aufrufe stehen in dieser Assemblerdatei?
indirekte() {
    grep -cE '^[[:space:]]*call[[:space:]]+(\*|r[a-z0-9]+$)' "$1" || true
}

N=${SCHRANKEN_N:-2000000}

cat > "$W/statisch.fi" <<EOF
interface Ordnung {
    fn kleiner(*self, b: *Self) -> bool
}

struct Punkt { x: i64 }

impl Ordnung for Punkt {
    fn kleiner(*self, b: *Punkt) -> bool { return (*self).x < (*b).x }
}

fn zaehle[T: Ordnung](a: *T, b: *T, n: i64) -> i64 {
    var i: i64 = 0
    var s: i64 = 0
    while i < n {
        if a.kleiner(b) {
            s = s + 1
        }
        i = i + 1
    }
    return s
}

fn main() -> i32 {
    var p: Punkt = Punkt{ x: 1 }
    var q: Punkt = Punkt{ x: 2 }
    if zaehle[Punkt](&p, &q, $N) != $N {
        return 1
    }
    return 0
}
EOF

cat > "$W/dynamisch.fi" <<EOF
interface OrdnungD {
    fn kleiner(*self, b: *Punkt) -> bool
}

struct Punkt { x: i64 }

impl OrdnungD for Punkt {
    fn kleiner(*self, b: *Punkt) -> bool { return (*self).x < (*b).x }
}

fn zaehle_dyn(a: dyn OrdnungD, b: *Punkt, n: i64) -> i64 {
    var i: i64 = 0
    var s: i64 = 0
    while i < n {
        if a.kleiner(b) {
            s = s + 1
        }
        i = i + 1
    }
    return s
}

fn main() -> i32 {
    var p: Punkt = Punkt{ x: 1 }
    var q: Punkt = Punkt{ x: 2 }
    let d: dyn OrdnungD = (&p) as dyn OrdnungD
    if zaehle_dyn(d, &q, $N) != $N {
        return 1
    }
    return 0
}
EOF

# --- 1./2./4. firnc0: Assembler, in allen drei Baustufen --------------------
for stufe in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stufe%%:*}
    opt=${stufe#*:}
    "$FIRNC" $opt --emit=asm -o "$W/s_$name.s" "$W/statisch.fi" 2>"$W/err" \
        || { melde "firnc0/$name: statisch liess sich nicht uebersetzen"; head -5 "$W/err"; continue; }
    "$FIRNC" $opt --emit=asm -o "$W/d_$name.s" "$W/dynamisch.fi" 2>"$W/err" \
        || { melde "firnc0/$name: dynamisch liess sich nicht uebersetzen"; head -5 "$W/err"; continue; }
    si=$(indirekte "$W/s_$name.s")
    di=$(indirekte "$W/d_$name.s")
    [ "$si" -eq 0 ] || melde "firnc0/$name: die Schrankenfassung hat $si indirekte Aufrufe (erwartet 0)"
    [ "$di" -ge 1 ] || melde "firnc0/$name: die dyn-Fassung hat keinen indirekten Aufruf — die Gegenprobe misst nichts"
    if grep -qE 'lea.*\.L__iface' "$W/s_$name.s"; then
        melde "firnc0/$name: die Schrankenfassung laedt eine Methodentafel-Adresse"
    fi
    grep -qE 'lea.*\.L__iface' "$W/d_$name.s" \
        || melde "firnc0/$name: die dyn-Fassung laedt KEINE Methodentafel-Adresse"
done
# Der namentliche Aufruf ist ohne Optimierer sichtbar — MIT Optimierer
# verschwindet er ganz, und das ist der eigentliche Gewinn (siehe Messung).
grep -qE '^[[:space:]]*call[[:space:]]+\S*Punkt__kleiner' "$W/s_no-opt.s" \
    || melde "firnc0/no-opt: kein namentlicher Aufruf 'Punkt__kleiner' in der Schrankenfassung"

# --- 3. Dieselbe Aussage in der FIR ----------------------------------------
"$FIRNC" --emit=fir-raw "$W/statisch.fi"  > "$W/s.fir" 2>/dev/null
"$FIRNC" --emit=fir-raw "$W/dynamisch.fi" > "$W/d.fir" 2>/dev/null
for wort in calli vtab; do
    n=$(grep -c "$wort" "$W/s.fir" || true)
    [ "$n" -eq 0 ] || melde "FIR der Schrankenfassung enthaelt $n mal '$wort'"
    n=$(grep -c "$wort" "$W/d.fir" || true)
    [ "$n" -ge 1 ] || melde "FIR der dyn-Fassung enthaelt kein '$wort'"
done

# --- 5. firnc1 sagt dasselbe -----------------------------------------------
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || melde "firnc1 liess sich nicht bauen"
fi
if [ -x "$FC1" ]; then
    if "$FC1" "$W/statisch.fi" -o "$W/s1.bin" >/dev/null 2>"$W/e1"; then
        si=$(indirekte "$W/s1.bin.s")
        [ "$si" -eq 0 ] || melde "firnc1: die Schrankenfassung hat $si indirekte Aufrufe"
        grep -qE '^[[:space:]]*call[[:space:]]+\S*Punkt__kleiner' "$W/s1.bin.s" \
            || melde "firnc1: kein namentlicher Aufruf 'Punkt__kleiner'"
        set +e; "$W/s1.bin"; rc=$?; set -e
        [ "$rc" -eq 0 ] || melde "firnc1: die Schrankenfassung liefert $rc statt 0"
    else
        melde "firnc1: statisch liess sich nicht uebersetzen"
        head -5 "$W/e1"
    fi
    if "$FC1" "$W/dynamisch.fi" -o "$W/d1.bin" >/dev/null 2>"$W/e2"; then
        di=$(indirekte "$W/d1.bin.s")
        [ "$di" -ge 1 ] || melde "firnc1: die dyn-Fassung hat keinen indirekten Aufruf"
        set +e; "$W/d1.bin"; rc=$?; set -e
        [ "$rc" -eq 0 ] || melde "firnc1: die dyn-Fassung liefert $rc statt 0"
    else
        melde "firnc1: dynamisch liess sich nicht uebersetzen"
        head -5 "$W/e2"
    fi
fi

# --- 6. Instruktionen (callgrind) ------------------------------------------
messen() {   # $1 = Binary -> Instruktionen gesamt
    valgrind --tool=callgrind --callgrind-out-file=/dev/null "$1" 2>&1 \
        | sed -n 's/.*I *refs: *//p' | tr -d ', '
}
if command -v valgrind >/dev/null 2>&1 && [ "${SCHRANKEN_MESSEN:-1}" = 1 ]; then
    for stufe in "release-fast:" "no-opt:--no-opt"; do
        name=${stufe%%:*}
        opt=${stufe#*:}
        "$FIRNC" $opt -o "$W/s_$name" "$W/statisch.fi"  2>/dev/null
        "$FIRNC" $opt -o "$W/d_$name" "$W/dynamisch.fi" 2>/dev/null
        a=$(messen "$W/s_$name")
        b=$(messen "$W/d_$name")
        if [ -n "$a" ] && [ -n "$b" ]; then
            echo "MESSUNG[$name]: schranke $a  dyn $b  je Durchlauf: $((a / N)) gegen $((b / N))"
        fi
    done
else
    echo "MESSUNG: uebersprungen (kein valgrind oder SCHRANKEN_MESSEN=0)"
fi

if [ "$FEHLER" -ne 0 ]; then
    echo "SCHRANKEN: FEHLGESCHLAGEN"
    exit 1
fi
echo "SCHRANKEN: bestanden — 0 indirekte Aufrufe unter der Schranke, >=1 unter 'dyn', in 3 Baustufen und in beiden Compilern"
exit 0
