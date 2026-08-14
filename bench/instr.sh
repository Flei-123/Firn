#!/usr/bin/env bash
# bench/instr.sh — A/B-Vergleich zweier firnc-Staende ueber die AUSGEFUEHRTEN
# INSTRUKTIONEN statt ueber die Uhr.
#
# WARUM: auf dieser Maschine schwankt die Wanduhrzeit derselben Binary um bis
# zu 40 % zwischen Laeufen — gemessen, nicht vermutet. Damit laesst sich eine
# Codegen-Aenderung von 5 % nicht bewerten; beim ersten Versuch erschien
# dieselbe Verbesserung einmal als -18 % und einmal als +6 %.
#
# `valgrind --tool=callgrind` zaehlt die tatsaechlich ausgefuehrten
# Instruktionen. Das Ergebnis ist auf die Instruktion genau reproduzierbar.
#
# EHRLICHE GRENZE: die Instruktionszahl ist NICHT die Laufzeit. Sie sagt nichts
# ueber Cache-Fehlgriffe, Sprungvorhersage oder Abhaengigkeitsketten; ein `lea`
# und ein `div` zaehlen beide als eine Instruktion. Sie ist die richtige Metrik
# fuer die Frage „erzeugt der Compiler weniger Arbeit?" — und nur dafuer wird
# sie hier benutzt. Fuer das Endergebnis bleibt die Uhr zustaendig
# (`bench/run.sh`, `bench/ab.sh`).
#
# Aufruf:  bash bench/instr.sh <firnc-alt> <firnc-neu> [programm ...]
set -uo pipefail
cd "$(dirname "$0")/.."

ALT="${1:?erster firnc fehlt}"
NEU="${2:?zweiter firnc fehlt}"
shift 2
WORK=.bench-instr
mkdir -p "$WORK"

QUELLEN=()
if [ "$#" -gt 0 ]; then
    for n in "$@"; do QUELLEN+=("bench/firn/$n.fi"); done
else
    for f in bench/firn/*.fi; do QUELLEN+=("$f"); done
fi

# Instruktionen eines Programms zaehlen. Leere Ausgabe = fehlgeschlagen.
zaehle() {
    local bin="$1" tag="$2"
    if ! timeout 900 valgrind --tool=callgrind \
        --callgrind-out-file="$WORK/$tag.cg" "$bin" >/dev/null 2>"$WORK/$tag.log"; then
        return 1
    fi
    grep -oP 'I\s+refs:\s+\K[0-9,]+' "$WORK/$tag.log" | tr -d ','
}

printf '%-14s %16s %16s %10s\n' PROGRAMM ALT NEU AENDERUNG
echo "-------------------------------------------------------------"
sa=0
sn=0
for src in "${QUELLEN[@]}"; do
    name=$(basename "$src" .fi)
    if ! "$ALT" "$src" -o "$WORK/$name.alt" 2>/dev/null; then
        printf '%-14s   BAU-FEHLER (alt)\n' "$name"
        continue
    fi
    if ! "$NEU" "$src" -o "$WORK/$name.neu" 2>/dev/null; then
        printf '%-14s   BAU-FEHLER (neu)\n' "$name"
        continue
    fi
    a=$(zaehle "$WORK/$name.alt" "$name.alt")
    n=$(zaehle "$WORK/$name.neu" "$name.neu")
    if [ -z "$a" ] || [ -z "$n" ]; then
        printf '%-14s   KEINE ZAEHLUNG (valgrind)\n' "$name"
        continue
    fi
    sa=$(awk -v x="$sa" -v y="$a" 'BEGIN{printf "%.0f", x+y}')
    sn=$(awk -v x="$sn" -v y="$n" 'BEGIN{printf "%.0f", x+y}')
    awk -v x="$name" -v a="$a" -v b="$n" \
        'BEGIN{printf "%-14s %16d %16d %+9.2f%%\n", x, a, b, (b-a)/a*100}'
done
echo "-------------------------------------------------------------"
awk -v a="$sa" -v b="$sn" \
    'BEGIN{if(a>0) printf "%-14s %16d %16d %+9.2f%%\n", "SUMME", a, b, (b-a)/a*100}'
