#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/billig/arm_icount.sh -- DIE EXAKT AUSGEFUEHRTEN BEFEHLE AUF ARM64.
#
# Das Gegenstueck zu `tools/bench90/icount.py`, das auf x86 mit callgrind
# zaehlt. Auf ARM64 gibt es hier kein callgrind (und kein Silizium), also
# zaehlt qemu selbst: `-singlestep -d cpu` schreibt je AUSGEFUEHRTEM Befehl
# einen Registerblock ins Protokoll.
#
# ZWEI FALLEN, beide teuer bezahlt, beide hier vermieden:
#
#   1. `-D /dev/stdout` schreibt NICHT in die Roehre. qemu oeffnet die Datei
#      selbst; was ankommt, ist nichts. Deshalb eine BENANNTE ROEHRE
#      (`mkfifo`): der Zaehler liest, qemu schreibt, und nichts landet auf
#      der Platte -- was auch noetig ist, denn ein Protokoll dieser Art
#      kostet rund 1060 Oktette JE BEFEHL.
#
#   2. Die Zeilen fangen NICHT mit `PC=` an -- `grep -c '^PC='` liefert
#      stumpf 0, ohne zu klagen. Genau EIN Feld steht einmal je Befehl im
#      Block: `X00=`. Das ist der Zaehler.
#
# Aufruf:  bash tools/billig/arm_icount.sh <zeitlimit-s> <binaer> [<binaer> ...]
set -u
LIMIT=${1:?zeitlimit fehlt}
shift
FIFO=$(mktemp -u /tmp/icfifo.XXXXXX)
RC=$(mktemp /tmp/icrc.XXXXXX)
for b in "$@"; do
    rm -f "$FIFO"
    mkfifo "$FIFO"
    ( timeout "$LIMIT" qemu-aarch64 -singlestep -d cpu -D "$FIFO" "$b" \
        >/dev/null 2>&1; echo $? > "$RC" ) &
    n=$(grep -c 'X00=' "$FIFO")
    wait
    if [ "$(cat "$RC")" = "124" ]; then
        echo "$(basename "$b") ZEITLIMIT (>$LIMIT s, mindestens $n Befehle)"
    else
        echo "$(basename "$b") $n"
    fi
done
rm -f "$FIFO" "$RC"
