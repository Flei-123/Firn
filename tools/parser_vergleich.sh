#!/usr/bin/env bash
# tools/parser_vergleich.sh — der in FIRN geschriebene Parser gegen den in
# RUST geschriebenen, ueber das gesamte Quellkorpus.
#
# MASSSTAB ist `firnc0 --emit=ast-kanon`: eine sprachneutrale, geklammerte
# Form des Syntaxbaums (compiler/src/ast_kanon.rs). Zwei unabhaengige Parser
# erzeugen denselben Text genau dann, wenn sie denselben Baum gebaut haben.
#
# Rueckgabewerte von `.astdump`:
#   0  Ausgabe erzeugt
#   1  Syntaxfehler
#   3  die Datei benutzt eine Erweiterung, die der Kernparser nicht kennt
#      (`enum`/`match`, Fehlerunionen, Generics, `gc class`, Attribute,
#      `comptime`) — solche Dateien werden GEZAEHLT, nicht uebergangen.
set -uo pipefail
cd "$(dirname "$0")/.."

FIRNC=compiler/target/release/firnc
DUMP=${ASTDUMP:-./.astdump}

if [ ! -x "$DUMP" ]; then
    "$FIRNC" bin/astdump.fi -o "$DUMP" || exit 1
fi

# BEKANNTE ABWEICHUNG — einzeln benannt:
#   tests/590_f64.fi  ->  das Literal `1e308`. Das ist KEIN Parserfehler,
#   sondern der bekannte Gleitkomma-Rundungsfall aus Runde 20
#   (tools/lex_vergleich.sh); der Wert steht schon im Token falsch.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
bekannt=0
nichtkern=0
uebersprungen=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=ast-kanon "$f" > /tmp/parv_a.txt 2>/dev/null; then
        # firnc0 kommt selbst nicht durch (Modulbruchstueck, Negativtest).
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > /tmp/parv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        nichtkern=$((nichtkern+1))
        continue
    fi
    if [ "$rc" -eq 0 ] && cmp -s /tmp/parv_a.txt /tmp/parv_b.txt; then
        gleich=$((gleich+1))
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
echo "NICHT KERN:    $nichtkern  (enum/match, Fehlerunionen, Generics, gc, Attribute, comptime)"
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 kommt selbst nicht durch)"
if [ -n "$erste" ]; then
    echo "erste unerwartete Abweichung: $erste"
    ff=${erste%% *}
    "$FIRNC" --emit=ast-kanon "$ff" > /tmp/parv_a.txt 2>/dev/null
    "$DUMP" "$ff" > /tmp/parv_b.txt 2>/dev/null
    diff /tmp/parv_a.txt /tmp/parv_b.txt | head -10
    exit 1
fi
exit 0
