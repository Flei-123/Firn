#!/usr/bin/env bash
# tools/fir_vergleich.sh — das Lowering in FIRN gegen das in RUST.
#
# MASSSTAB ist `firnc0 --emit=fir-raw`: die Zwischendarstellung DIREKT nach dem
# Lowering, ohne jede Optimierung. Verglichen wird der Text Oktett fuer Oktett,
# und der enthaelt alles, worauf es ankommt: Wertnummern, Blocknummern,
# Reihenfolge der Instruktionen, Terminatoren.
#
# Das ist der schaerfste Vergleich der ganzen Reihe. Zwei Wertnummern in
# anderer Reihenfolge, ein Block zu viel oder zu wenig — und der Text stimmt
# nicht mehr.
#
# Rueckgabewerte von `.firdump`:
#   0 Ausgabe · 1 Fehler · 3 keine Kernsprache · 4 comptime noetig ·
#   5 `defer`/`errdefer` (im Lowering noch nicht portiert)
#
# GETESTET WIRD NUR, was `firnc0` auch EINZELN uebersetzen kann: `--emit=fir-raw`
# laeuft sonst ueber das zusammengefuehrte Modulprogramm, der Firn-Weg aber
# ueber eine einzige Datei — das waere kein Vergleich, sondern zwei
# verschiedene Eingaben.
set -uo pipefail
cd "$(dirname "$0")/.."

FIRNC=compiler/target/release/firnc
DUMP=${FIRDUMP:-./.firdump}

# Neu bauen, wenn das Dump-Binary fehlt ODER Quellen juenger sind
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/firdump.fi -o "$DUMP" || exit 1
fi

BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
bekannt=0
nichtkern=0
comptime=0
defer_zahl=0
uebersprungen=0
instruktionen=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=typen "$f" >/dev/null 2>&1; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    if ! "$FIRNC" --emit=fir-raw "$f" > /tmp/firv_a.txt 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > /tmp/firv_b.txt 2>/dev/null
    rc=$?
    case "$rc" in
        3) nichtkern=$((nichtkern+1)); continue;;
        4) comptime=$((comptime+1)); continue;;
        5) defer_zahl=$((defer_zahl+1)); continue;;
    esac
    if [ "$rc" -eq 0 ] && cmp -s /tmp/firv_a.txt /tmp/firv_b.txt; then
        gleich=$((gleich+1))
        n=$(grep -c '^  ' /tmp/firv_a.txt)
        instruktionen=$((instruktionen + n))
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
echo "INSTRUKTIONEN: $instruktionen  (Wertnummern und Bloecke eingeschlossen)"
echo "DEFER:         $defer_zahl  (im Lowering noch nicht portiert)"
echo "NICHT KERN:    $nichtkern"
echo "COMPTIME:      $comptime"
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 uebersetzt die Datei nicht einzeln)"
if [ -n "$erste" ]; then
    echo "erste unerwartete Abweichung: $erste"
    ff=${erste%% *}
    diff <("$FIRNC" --emit=fir-raw "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -12
    exit 1
fi
exit 0
