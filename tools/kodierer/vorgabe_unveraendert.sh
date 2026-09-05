#!/usr/bin/env bash
# RUNDE KODIERER -- Beweis, dass der VORGABEPFAD sich nicht geaendert hat.
#
# Die Runde hat Module HINZUGEFUEGT und eine Fahne eingebaut; ohne
# `--asm-intern` soll firnc genau das tun, was es vorher tat. Das laesst sich
# staerker und billiger pruefen als mit der ganzen Testsuite: derselbe
# Testfall, gebaut mit dem UNBERUEHRTEN Uebersetzer und mit dem dieser Runde,
# Ergebnis Oktett fuer Oktett vergleichen.
#
# Aufruf: bash tools/kodierer/vorgabe_unveraendert.sh [alter-firnc] [muster]
set -u
cd "$(dirname "$0")/../.."
ALT="${1:-/root/firn/compiler/target/release/firnc}"
NEU="compiler/target/release/firnc"
MUSTER="${2:-tests/*.fi}"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT

if [ ! -x "$ALT" ]; then
    echo "kein unberuehrter Uebersetzer unter $ALT"
    exit 2
fi

gleich=0; anders=0; sprung=0
for t in $MUSTER; do
    [ -f "$t" ] || continue
    "$ALT" -o "$W/a" "$t" >/dev/null 2>&1 || { sprung=$((sprung+1)); continue; }
    "$NEU" -o "$W/b" "$t" >/dev/null 2>&1 || { sprung=$((sprung+1)); continue; }
    if cmp -s "$W/a" "$W/b"; then
        gleich=$((gleich+1))
    else
        anders=$((anders+1))
        echo "ABWEICHUNG: $t"
    fi
    rm -f "$W/a" "$W/b"
done
echo "----"
echo "oktettgleich zum unberuehrten Uebersetzer : $gleich"
echo "abweichend                                : $anders"
echo "uebersprungen (baut in keinem der beiden) : $sprung"
[ "$anders" -eq 0 ]
