#!/usr/bin/env bash
# tools/lex_vergleich.sh — der in FIRN geschriebene Lexer gegen den in RUST
# geschriebenen, ueber das gesamte Quellkorpus, Oktett fuer Oktett.
#
# WARUM SO: ein Lexer laesst sich nicht sinnvoll gegen sich selbst pruefen.
# `firnc0 --emit=tokens` ist eine unabhaengige Umsetzung in einer anderen
# Sprache — stimmen beide Ausgaben ueberein, ist das eine echte Gegenprobe.
#
# Verglichen werden BEIDE Stroeme:
#   * Standardausgabe = der Tokenstrom          (lib/firnc1/lexer.fi)
#   * Fehlerausgabe   = die Diagnosen mit Zeile, Spalte, Quelltextzeile und
#                       Markierung              (lib/firnc1/diag.fi)
# Deshalb bekommt `lexdump` den DATEINAMEN als Aufrufargument: er steht in
# jeder Diagnose.
set -uo pipefail
cd "$(dirname "$0")/.."

FIRNC=compiler/target/release/firnc
DUMP=${DUMP:-./.lexdump}

if [ ! -x "$DUMP" ]; then
    "$FIRNC" bin/lexdump.fi -o "$DUMP" || exit 1
fi

# BEKANNTE ABWEICHUNGEN — jede einzeln benannt, mit Grund. Diese Liste ist
# KEIN Freibrief: sie steht hier, damit die Zahl der Ausnahmen sichtbar bleibt
# und nicht schweigend waechst.
#
#   tests/590_f64.fi  ->  das Literal `1e308`. Der Lexer in Firn rechnet
#   Gleitkommaliterale ausserhalb des schnellen Pfades von Clinger
#   (|Exponent| > 22 und Mantisse passt nicht in 2^53) schrittweise und liegt
#   dort um bis zu ein ULP daneben. Korrekt waere Eisel-Lemire mit
#   128-Bit-Arithmetik; die fehlt noch.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
unerwartet=0
bekannt=0
mit_fehler=0
uebersprungen=0
tokens=0
langsam=0
erste=""

while IFS= read -r f; do
    "$FIRNC" --emit=tokens "$f" > /tmp/lexv_a.txt 2>/tmp/lexv_ae.txt
    # Modulbruchstuecke (`tests/modules/*.fi`) lassen sich nicht einzeln
    # uebersetzen: `firnc0` bricht schon in der Modulaufloesung ab, VOR dem
    # Lexer. Das ist keine Lexerfrage — solche Dateien werden gezaehlt und
    # uebersprungen.
    if grep -q "nicht lesen:" /tmp/lexv_ae.txt; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > /tmp/lexv_b.txt 2>/tmp/lexv_be.txt
    # Zaehlwerte stehen nur dann auf der Fehlerausgabe, wenn es KEINE
    # Diagnosen gab — sonst gehoert der ganze Strom den Meldungen.
    if grep -q '^; tokens ' /tmp/lexv_be.txt; then
        t=$(awk '{print $3}' /tmp/lexv_be.txt)
        g=$(awk '{print $5}' /tmp/lexv_be.txt)
        tokens=$((tokens + ${t:-0}))
        langsam=$((langsam + ${g:-0}))
        : > /tmp/lexv_be.txt
    else
        mit_fehler=$((mit_fehler+1))
    fi
    if cmp -s /tmp/lexv_a.txt /tmp/lexv_b.txt && cmp -s /tmp/lexv_ae.txt /tmp/lexv_be.txt; then
        gleich=$((gleich+1))
    else
        ungleich=$((ungleich+1))
        if echo "$BEKANNT" | tr ' ' '\n' | grep -qxF "$f"; then
            bekannt=$((bekannt+1))
        else
            unerwartet=$((unerwartet+1))
            [ -z "$erste" ] && erste="$f"
        fi
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "GLEICH:        $gleich"
echo "UNGLEICH:      $ungleich   (bekannt und benannt: $bekannt)"
echo "MIT DIAGNOSEN: $mit_fehler  (Fehlerausgabe ebenfalls verglichen)"
echo "UEBERSPRUNGEN: $uebersprungen  (Modulbruchstueck, nicht einzeln uebersetzbar)"
echo "TOKEN GESAMT:  $tokens"
echo "GLEITKOMMA ausserhalb des schnellen Pfades: $langsam"
if [ "$unerwartet" -gt 0 ]; then
    echo "erste unerwartete Abweichung: $erste"
    "$FIRNC" --emit=tokens "$erste" > /tmp/lexv_a.txt 2>/tmp/lexv_ae.txt
    "$DUMP" "$erste" > /tmp/lexv_b.txt 2>/tmp/lexv_be.txt
    diff /tmp/lexv_a.txt /tmp/lexv_b.txt | head -12
    diff /tmp/lexv_ae.txt /tmp/lexv_be.txt | head -20
    exit 1
fi
exit 0
