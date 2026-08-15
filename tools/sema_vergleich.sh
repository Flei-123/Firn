#!/usr/bin/env bash
# tools/sema_vergleich.sh — der Typpruefer in FIRN gegen den in RUST.
#
# MASSSTAB ist `firnc0 --emit=typen`: der kanonische Syntaxbaum mit dem TYP an
# jedem Ausdruck. `firnc0` sichert zu, dass nach der Pruefung jeder Ausdruck
# einen konkreten Typ hat — genau diese Zusicherung wird hier verglichen.
#
# Rueckgabewerte von `.semadump`:
#   0  Ausgabe erzeugt
#   1  Fehler
#   3  keine Kernsprache (enum/match, Fehlerunionen, Generics, gc, Attribute,
#      comptime, die Intrinsics fuer konstante Laufzeit)
#   4  eine Konstante braucht Auswertung zur Uebersetzungszeit (comptime.rs)
#
# UEBERSPRUNGEN werden Dateien, die `firnc0` SELBST nicht einzeln pruefen kann
# — fast alle davon binden ein Modul ein, dessen Namen einzeln unbekannt sind.
set -uo pipefail
cd "$(dirname "$0")/.."

FIRNC=compiler/target/release/firnc
DUMP=${SEMADUMP:-./.semadump}

if [ ! -x "$DUMP" ]; then
    "$FIRNC" bin/semadump.fi -o "$DUMP" || exit 1
fi

# BEKANNTE ABWEICHUNG: tests/590_f64.fi, das Literal `1e308`. Kein Typfehler,
# sondern der Gleitkomma-Rundungsfall aus Runde 20 — der Wert steht schon im
# Token falsch.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
bekannt=0
nichtkern=0
comptime=0
uebersprungen=0
ausdruecke=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=typen "$f" > /tmp/semv_a.txt 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > /tmp/semv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        nichtkern=$((nichtkern+1))
        continue
    fi
    if [ "$rc" -eq 4 ]; then
        comptime=$((comptime+1))
        continue
    fi
    if [ "$rc" -eq 0 ] && cmp -s /tmp/semv_a.txt /tmp/semv_b.txt; then
        gleich=$((gleich+1))
        # Jede " :" ist ein typisierter Ausdruck.
        n=$(grep -o ' :' /tmp/semv_a.txt | wc -l)
        ausdruecke=$((ausdruecke + n))
        continue
    fi
    ungleich=$((ungleich+1))
    if echo "$BEKANNT" | tr ' ' '\n' | grep -qxF "$f"; then
        bekannt=$((bekannt+1))
    else
        [ -z "$erste" ] && erste="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "GLEICH:        $gleich"
echo "UNGLEICH:      $ungleich   (bekannt und benannt: $bekannt)"
echo "AUSDRUECKE:    $ausdruecke  (jeder mit demselben Typ wie in firnc0)"
echo "NICHT KERN:    $nichtkern"
echo "COMPTIME:      $comptime  (konstante Auswertung zur Uebersetzungszeit, nicht portiert)"
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 prueft die Datei nicht einzeln)"
if [ -n "$erste" ]; then
    echo "erste unerwartete Abweichung: $erste"
    ff=${erste%% *}
    diff <("$FIRNC" --emit=typen "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -6
    exit 1
fi
exit 0
