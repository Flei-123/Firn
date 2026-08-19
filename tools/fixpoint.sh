#!/usr/bin/env bash
# tools/fixpoint.sh — DER FIXPUNKT: Firn traegt sich selbst.
#
# Drei Stufen, dieselbe Quelle:
#
#   Stufe 1   firnc0 (Rust)  uebersetzt  bin/firnc1.fi  ->  .firnc1
#   Stufe 2   .firnc1        uebersetzt  bin/firnc1.fi  ->  .firnc2
#   Stufe 3   .firnc2        uebersetzt  bin/firnc1.fi  ->  .firnc3
#
# STUFE 2 UND STUFE 3 MUESSEN ZEICHENGLEICH SEIN. Das ist der eigentliche
# Nachweis: `.firnc2` ist von einem Compiler erzeugt, der aus Rust kam,
# `.firnc3` von einem, der aus Firn kam. Sind ihre Ausgaben gleich, haengt das
# Ergebnis nicht mehr am Rust-Compiler — der Uebersetzer ist ein Fixpunkt
# seiner selbst.
#
# WARUM STUFE 1 NICHT MITVERGLICHEN WIRD: `firnc0` hat eine Registerzuteilung,
# `lib/firnc1/codegen.fi` nicht. Die Assemblertexte von Stufe 1 und Stufe 2
# koennen gar nicht gleich sein und muessen es auch nicht — verglichen wird,
# was ab Stufe 2 stabil bleibt.
#
# Zum Schluss: der selbst uebersetzte Compiler (.firnc2) laeuft ueber das
# GANZE Testkorpus und muss sich dabei genauso verhalten wie Stufe 1. Ein
# Compiler, der nur sich selbst uebersetzen kann, waere kein Compiler.
set -uo pipefail
cd "$(dirname "$0")/.."
# Eigenes Temp-Verzeichnis je Lauf: zwei gleichzeitige Laeufe (z. B. Haupt-
# repo und ein Worktree) benutzten sonst DIESELBEN /tmp-Dateien und
# ueberschrieben sich gegenseitig die Vergleichsausgaben — das sah wie ein
# echter Unterschied aus (Runde 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
QUELLE=bin/firnc1.fi

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 fehlt: $FIRNC"
    exit 1
fi

# --- Stufe 1 ---------------------------------------------------------------
# Neu bauen, wenn .firnc1 fehlt ODER eine Quelldatei juenger ist — ein
# veraltetes .firnc1 misst sonst den Stand von gestern (Runde 35).
if [ ! -x ./.firnc1 ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer ./.firnc1 -print -quit)" ]; then
    rm -f ./.firnc1
    "$FIRNC" "$QUELLE" -o ./.firnc1 || { echo "Stufe 1 schlug fehl"; exit 1; }
fi

# --- Stufe 2 ---------------------------------------------------------------
rm -f .firnc2 .firnc2.s .firnc2.o
t0=$(date +%s%N)
./.firnc1 "$QUELLE" -o ./.firnc2
rc2=$?
t1=$(date +%s%N)
if [ "$rc2" -ne 0 ]; then
    echo "STUFE 2 SCHLUG FEHL (rc=$rc2)"
    echo "  3 = keine Kernsprache · 4 = comptime · 5 = defer · 6 = Codegenerator"
    exit 1
fi
if [ ! -x ./.firnc2 ]; then
    echo "STUFE 2: keine ausfuehrbare Datei"
    exit 1
fi

# --- Stufe 3 ---------------------------------------------------------------
rm -f .firnc3 .firnc3.s .firnc3.o
t2=$(date +%s%N)
./.firnc2 "$QUELLE" -o ./.firnc3
rc3=$?
t3=$(date +%s%N)
if [ "$rc3" -ne 0 ]; then
    echo "STUFE 3 SCHLUG FEHL (rc=$rc3)"
    exit 1
fi

echo "STUFE 2: $(( (t1 - t0) / 1000000 )) ms   $(wc -c < .firnc2) Oktette"
echo "STUFE 3: $(( (t3 - t2) / 1000000 )) ms   $(wc -c < .firnc3) Oktette"

# --- der Vergleich ---------------------------------------------------------
if ! cmp -s .firnc2.s .firnc3.s; then
    echo "KEIN FIXPUNKT: die Assemblertexte von Stufe 2 und 3 unterscheiden sich"
    diff <(head -400 .firnc2.s) <(head -400 .firnc3.s) | head -20
    exit 1
fi
if ! cmp -s .firnc2 .firnc3; then
    echo "KEIN FIXPUNKT: die Binaerdateien unterscheiden sich (bei gleichem .s)"
    exit 1
fi
zeilen=$(wc -l < .firnc2.s)
echo "FIXPUNKT:  Stufe 2 == Stufe 3, zeichengleich ($zeilen Zeilen Assembler)"

# --- der selbst uebersetzte Compiler am ganzen Korpus ----------------------
FIRNC1=./.firnc2 bash tools/self_compare.sh > "$TMPD"/fixpunkt_korpus.txt 2>&1
krc=$?
sed 's/^/  /' "$TMPD"/fixpunkt_korpus.txt
if [ "$krc" -ne 0 ]; then
    echo "STUFE 2 verhaelt sich am Korpus NICHT wie Stufe 1"
    exit 1
fi
echo "KORPUS:    .firnc2 verhaelt sich wie firnc0"
exit 0
