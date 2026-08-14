#!/usr/bin/env bash
# bench/ab.sh — A/B-Vergleich ZWEIER firnc-Staende auf denselben Programmen.
#
# WARUM DIESES SKRIPT: `bench/run.sh` misst Firn gegen Rust. Der Faktor dort
# schwankt aber mit der RUST-Zeit — auf einer geteilten Maschine um bis zu
# 20 %. Beim Optimieren des Compilers hat das dazu gefuehrt, dass eine
# Verbesserung als Verschlechterung erschien, obwohl die Firn-Zeiten praktisch
# gleich blieben. Ein A/B-Vergleich zweier Firn-Staende hat dieses Problem
# nicht: dieselben Programme, dieselbe Maschine, unmittelbar nacheinander.
#
# MINIMUM statt Median: Stoerungen machen einen Lauf nur langsamer, nie
# schneller. Der kleinste von N Laeufen ist damit der robusteste Schaetzer
# fuer die ungestoerte Rechenzeit.
#
# Aufruf:  bash bench/ab.sh <firnc-alt> <firnc-neu> [laeufe]
set -uo pipefail
cd "$(dirname "$0")/.."

ALT="${1:?erster firnc fehlt}"
NEU="${2:?zweiter firnc fehlt}"
LAEUFE="${3:-9}"
WORK=.bench-ab
rm -rf "$WORK"; mkdir -p "$WORK"

beste() {
    local best="" i a b t
    for ((i = 0; i < LAEUFE; i++)); do
        a=$(date +%s.%N)
        "$@" >/dev/null 2>&1
        b=$(date +%s.%N)
        t=$(awk -v a="$a" -v b="$b" 'BEGIN{printf "%.6f", b-a}')
        best=$(awk -v x="$t" -v y="$best" 'BEGIN{if(y==""||x+0<y+0)print x;else print y}')
    done
    echo "$best"
}

printf '%-14s %10s %10s %9s\n' PROGRAMM ALT NEU AENDERUNG
echo "---------------------------------------------------"
summe_alt=0
summe_neu=0
for src in bench/firn/*.fi; do
    name=$(basename "$src" .fi)
    if ! "$ALT" "$src" -o "$WORK/$name.alt" 2>"$WORK/$name.alt.err"; then
        printf '%-14s   BAU-FEHLER (alt)\n' "$name"; continue
    fi
    if ! "$NEU" "$src" -o "$WORK/$name.neu" 2>"$WORK/$name.neu.err"; then
        printf '%-14s   BAU-FEHLER (neu)\n' "$name"; continue
    fi
    # Gleiches Ergebnis? Sonst ist die Messung wertlos.
    "$WORK/$name.alt" > "$WORK/$name.alt.out" 2>&1
    "$WORK/$name.neu" > "$WORK/$name.neu.out" 2>&1
    if ! cmp -s "$WORK/$name.alt.out" "$WORK/$name.neu.out"; then
        printf '%-14s   ERGEBNIS WEICHT AB — Messung ungueltig\n' "$name"
        continue
    fi
    ta=$(beste "$WORK/$name.alt")
    tn=$(beste "$WORK/$name.neu")
    summe_alt=$(awk -v a="$summe_alt" -v b="$ta" 'BEGIN{print a+b}')
    summe_neu=$(awk -v a="$summe_neu" -v b="$tn" 'BEGIN{print a+b}')
    awk -v n="$name" -v a="$ta" -v b="$tn" \
        'BEGIN{printf "%-14s %9.4fs %9.4fs %+8.1f%%\n", n, a, b, (b-a)/a*100}'
done
echo "---------------------------------------------------"
awk -v a="$summe_alt" -v b="$summe_neu" \
    'BEGIN{printf "%-14s %9.4fs %9.4fs %+8.1f%%\n", "SUMME", a, b, (b-a)/a*100}'
