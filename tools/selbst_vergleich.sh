#!/usr/bin/env bash
# tools/selbst_vergleich.sh — der Compiler in FIRN uebersetzt, das Ergebnis
# LAEUFT, und es tut dasselbe wie das von `firnc0`.
#
# WARUM NICHT DER ASSEMBLERTEXT: `firnc0` hat eine Registerzuteilung
# (regalloc.rs), `lib/firnc1/codegen.fi` nicht — jeder Wert liegt dort im
# Rahmen. Die beiden Texte koennen gar nicht gleich sein, und sie MUESSEN es
# auch nicht. Was zaehlt, ist das Verhalten: derselbe Rueckgabewert, dieselbe
# Ausgabe.
#
# Ablauf je Datei:
#   1. `.firnc1 datei.fi -o ziel`  — und zwar ALLES davon in Firn: lexen,
#      parsen, pruefen, lowern, Code erzeugen, `as` und `ld` ueber
#      `fork`/`execve` aufrufen. Das Skript ruft KEIN Werkzeug selbst auf.
#   2. laufen lassen, Rueckgabewert und Standardausgabe vergleichen
#
# Rueckgabewerte von `.firnc1`: 3 = keine Kernsprache · 4 = comptime ·
# 5 = `defer` · 6 = der Codegenerator kann diese FIR nicht (Gleitkomma,
# mehr als sechs Argumente).
set -uo pipefail
cd "$(dirname "$0")/.."

# Modul-Suchpfad (Runde 39): std-Fassade fuer beide Seiten des Vergleichs.
export FIRNLIB="$(pwd)/lib"

FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
WORK=.selbst-work
mkdir -p "$WORK"

if [ ! -x "$FC1" ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1
fi

gleich=0
ungleich=0
nichtkern=0
comptime=0
defer_zahl=0
codegen=0
uebersprungen=0
fehlerhaft=0
erste=""

while IFS= read -r f; do
    # Nur Dateien, die `firnc0` uebersetzen kann — sonst waeren es zwei
    # verschiedene Eingaben. Seit Runde 29 zaehlen auch Dateien mit `import`
    # dazu: `firnc1` loest sie selbst auf.
    if ! "$FIRNC" "$f" -o "$WORK/ref" 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    rm -f "$WORK/a.bin" "$WORK/a.bin.s" "$WORK/a.bin.o"
    "$FC1" "$f" -o "$WORK/a.bin" >/dev/null 2>&1
    rc=$?
    case "$rc" in
        3) nichtkern=$((nichtkern+1)); continue;;
        4) comptime=$((comptime+1)); continue;;
        5) defer_zahl=$((defer_zahl+1)); continue;;
        6) codegen=$((codegen+1)); continue;;
    esac
    if [ "$rc" -ne 0 ]; then
        fehlerhaft=$((fehlerhaft+1))
        [ -z "$erste" ] && erste="$f (firnc1 rc=$rc)"
        continue
    fi
    if [ ! -x "$WORK/a.bin" ]; then
        fehlerhaft=$((fehlerhaft+1))
        [ -z "$erste" ] && erste="$f (keine ausfuehrbare datei)"
        continue
    fi
    timeout 20 "$WORK/ref" > "$WORK/ref.out" 2>/dev/null
    rref=$?
    timeout 20 "$WORK/a.bin" > "$WORK/a.out" 2>/dev/null
    ra=$?
    if [ "$rref" -eq "$ra" ] && cmp -s "$WORK/ref.out" "$WORK/a.out"; then
        gleich=$((gleich+1))
    else
        ungleich=$((ungleich+1))
        [ -z "$erste" ] && erste="$f (firnc0: $rref, firnc1: $ra)"
    fi
done < <(find tests bench -name '*.fi' -not -type l -not -path 'tests/neg/*' -not -path 'tests/lexneg/*' | sort)

echo "GLEICHES VERHALTEN: $gleich"
echo "ABWEICHEND:         $ungleich"
echo "FEHLERHAFT:         $fehlerhaft"
echo "NICHT KERN:         $nichtkern"
echo "DEFER:              $defer_zahl"
echo "COMPTIME:           $comptime"
echo "CODEGEN FEHLT:      $codegen  (Gleitkomma, mehr als sechs Argumente)"
echo "UEBERSPRUNGEN:      $uebersprungen  (firnc0 uebersetzt die Datei nicht einzeln)"
if [ -n "$erste" ]; then
    echo "erste Abweichung: $erste"
    exit 1
fi
exit 0
