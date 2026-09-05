#!/usr/bin/env bash
# RUNDE KODIERER -- die Probe aufs Ganze.
#
# Die oktettweise Gegenprobe (vergleich.py) sagt: die Objektdatei ist
# dieselbe. Diese Probe sagt: das PROGRAMM verhaelt sich auch so. Jeder
# Testfall wird ZWEIMAL gebaut -- einmal ueber `as`, einmal ueber den
# eigenen Kodierer -- und beide Male ausgefuehrt. Verglichen wird gegen den
# ALTEN Weg, nicht gegen die Erwartung im Kopf der Datei: Faelle, die schon
# vorher rot waren, gehen so nicht zu Lasten des Kodierers.
#
# Aufruf: bash tools/kodierer/ende_zu_ende.sh [muster]
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
MUSTER="${1:-tests/*.fi}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT

gleich=0
anders=0
altfehler=0
neufehler=0

lauf() { # $1 = programm -> "rc|ausgabe"
    local out rc
    out=$(timeout 60 "$1" 2>&1)
    rc=$?
    printf '%s|%s' "$rc" "$out"
}

for t in $MUSTER; do
    [ -f "$t" ] || continue
    b=$(basename "$t" .fi)
    if ! "$FIRNC" -o "$W/alt" "$t" >"$W/alt.log" 2>&1; then
        altfehler=$((altfehler+1)); continue
    fi
    if ! "$FIRNC" --asm-intern -o "$W/neu" "$t" >"$W/neu.log" 2>&1; then
        neufehler=$((neufehler+1))
        echo "BAU  $t"
        head -3 "$W/neu.log" | sed 's/^/     /'
        continue
    fi
    a=$(lauf "$W/alt")
    n=$(lauf "$W/neu")
    if [ "$a" = "$n" ]; then
        gleich=$((gleich+1))
    else
        anders=$((anders+1))
        echo "LAUF $t"
        echo "     alt: ${a%%|*}  neu: ${n%%|*}"
    fi
done

echo "----"
echo "gleiches Verhalten : $gleich"
echo "abweichend         : $anders"
echo "alter Weg scheitert: $altfehler  (nicht dem Kodierer anzulasten)"
echo "neuer Weg scheitert: $neufehler"
[ "$anders" -eq 0 ] && [ "$neufehler" -eq 0 ]
