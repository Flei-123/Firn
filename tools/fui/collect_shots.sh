#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/collect_shots.sh -- DIE BELEGE IN DEN BAUM UEBERNEHMEN.
#
# `tools/fui/run.sh --images` malt sechsundzwanzig Bilder nach
# $BELEGE (Vorgabe: $W/belege). Ausgeliefert wird dieselbe Menge unter
# .gauntlet-shots/, in LESEREIHENFOLGE nummeriert -- ein Pruefer, der
# den Lauf nicht startet, soll die Bilder in der Reihenfolge sehen, in
# der RUN.md sie bespricht.
#
# Warum dieses Skript und keine Liste in RUN.md: die Zuordnung "welches
# gemalte Bild wird welche ausgelieferte Nummer" ist eine Tabelle, und
# eine Tabelle gehoert an EINEN Ort. Stand sie in der Anleitung, wurde
# beim naechsten neuen Beleg die Anleitung nachgezogen und die Kopie
# vergessen (oder umgekehrt), und am Ende lieferte der Baum ein Bild
# aus, das kein Programm mehr malt.
#
#     sh tools/fui/collect_shots.sh
#
# W und BELEGE wirken wie in run.sh.
set -e
cd "$(dirname "$0")/../.."
W="${W:-/tmp/fui-acceptance}"
Z="${BELEGE:-$W/belege}"
ZIEL=".gauntlet-shots"

if [ ! -d "$Z" ]; then
    echo "collect_shots: \"$Z\" gibt es nicht."
    echo "Erst malen: sh tools/fui/run.sh --images"
    exit 1
fi
mkdir -p "$ZIEL"

# Die Tabelle: <ausgelieferter Name ohne .png>:<gemalter Name ohne .png>
TAFEL="
01-wave1-grundelemente-hell:fui-wave1-light
02-wave1-grundelemente-dunkel:fui-wave1-dark
03-wave2-widgets-hell:fui-wave2-light
04-wave2-widgets-dunkel:fui-wave2-dark
05-wave3-widgets-hell:fui-wave3-light
06-wave3-widgets-dunkel:fui-wave3-dark
07-text-hell:fui-text-light
08-text-dunkel:fui-text-dark
09-bild-svg-hell:fui-bild-svg-hell
10-bild-svg-dunkel:fui-bild-svg-dunkel
11-anim-phasen-hell:fui-anim-hell
12-anim-phasen-dunkel:fui-anim-dunkel
13-flex-varianten-hell:fui-flex-hell
14-flex-varianten-dunkel:fui-flex-dunkel
15-effekt-blur-schatten-glas-hell:fui-effekt-hell
16-effekt-blur-schatten-glas-dunkel:fui-effekt-dunkel
17-transform-rotate-scale-hell:fui-transform-hell
18-transform-rotate-scale-dunkel:fui-transform-dunkel
19-demo-anwendung-hell:fui-demo-hell
20-demo-anwendung-dunkel:fui-demo-dunkel
21-preview-zustaende-hell:fui-preview-hell
22-preview-zustaende-dunkel:fui-preview-dunkel
23-deklarativ-scene-sheet-hell:fui-deklarativ-hell
24-deklarativ-scene-sheet-dunkel:fui-deklarativ-dunkel
25-deklarativ-schmal-980px-hell:fui-deklarativ-schmal-hell
26-deklarativ-schmal-980px-dunkel:fui-deklarativ-schmal-dunkel
"

n=0
for e in $TAFEL; do
    ziel="${e%%:*}"
    quelle="${e#*:}"
    if [ ! -r "$Z/$quelle.png" ]; then
        echo "collect_shots: \"$Z/$quelle.png\" fehlt."
        echo "Der Belegsatz waere unvollstaendig -- Abbruch."
        exit 1
    fi
    cp "$Z/$quelle.png" "$ZIEL/$ziel.png"
    n=$((n + 1))
done
echo "collect_shots: $n Bilder nach $ZIEL uebernommen."
