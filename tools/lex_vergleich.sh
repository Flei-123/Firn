#!/usr/bin/env bash
# tools/lex_vergleich.sh — der in FIRN geschriebene Lexer gegen den in RUST
# geschriebenen, ueber das gesamte Quellkorpus, Oktett fuer Oktett.
#
# WARUM SO: ein Lexer laesst sich nicht sinnvoll gegen sich selbst pruefen.
# `firnc0 --emit=tokens` ist eine unabhaengige Umsetzung in einer anderen
# Sprache — stimmen beide Ausgaben ueberein, ist das eine echte Gegenprobe.
#
# UEBERSPRUNGEN werden nur Dateien, die `firnc0` selbst nicht fehlerfrei lext
# (Negativtests) — dort ist die Fehlerausgabe der Massstab, nicht der
# Tokenstrom, und die Diagnosen gehoeren zum Modul `diag`, nicht zum Lexer.
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
#   (|Exponent| > 22, Mantisse passt nicht in 2^53) schrittweise und liegt
#   dort um bis zu ein ULP daneben. Korrekt waere Eisel-Lemire mit
#   128-Bit-Arithmetik; die fehlt noch. Betrifft im ganzen Korpus GENAU EIN
#   Literal von 211.126 Token.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
unerwartet=0
bekannt=0
uebersprungen=0
tokens=0
langsam=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=tokens "$f" > /tmp/lexv_a.txt 2>/tmp/lexv_e.txt; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    if grep -q '^error:' /tmp/lexv_e.txt; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" < "$f" > /tmp/lexv_b.txt 2>/tmp/lexv_s.txt
    t=$(awk '{print $3}' /tmp/lexv_s.txt)
    g=$(awk '{print $5}' /tmp/lexv_s.txt)
    tokens=$((tokens + ${t:-0}))
    langsam=$((langsam + ${g:-0}))
    if cmp -s /tmp/lexv_a.txt /tmp/lexv_b.txt; then
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
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 meldet dort selbst einen Fehler)"
echo "TOKEN GESAMT:  $tokens"
echo "GLEITKOMMA ausserhalb des schnellen Pfades: $langsam"
if [ "$unerwartet" -gt 0 ]; then
    echo "erste unerwartete Abweichung: $erste"
    "$FIRNC" --emit=tokens "$erste" > /tmp/lexv_a.txt 2>/dev/null
    "$DUMP" < "$erste" > /tmp/lexv_b.txt 2>/dev/null
    diff /tmp/lexv_a.txt /tmp/lexv_b.txt | head -20
    exit 1
fi
exit 0
