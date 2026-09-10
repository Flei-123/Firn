#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-only
# tools/fui/run.sh -- DIE ABNAHME VON fUi.
#
# Ein Lauf, der alles prueft. Faellt eine Pruefung, faellt der Bau --
# das ist Justins Punkt 2 zur Abnahme und die Antwort darauf, dass
# dieselbe Zeichnung in OrientOS zweimal STILL verlorengegangen ist.
#
#     tools/fui/run.sh              nur pruefen
#     tools/fui/run.sh --bilder     zusaetzlich die Schauseite malen
#
# Die Abzuege landen unter /srv/store/belege/fui/.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-/root/firn-certuswin/compiler/target/release/firnc}"
W="${W:-/tmp/fui-abnahme}"
mkdir -p "$W"

bau() {
    "$FIRNC" --opt-level=dev -o "$W/$1" "tools/fui/$1_main.fi"
}

echo "== 1. DER KERN BAUT FREISTEHEND (profile kernel) =="
# Das ist Justins Architektur: derselbe Quelltext im Kernel und in der
# Anwendung. Geprueft wird, dass es baut UND dass kein Systemruf und
# kein fremder Name darin steht.
"$FIRNC" --profile=kernel --target=x86_64-none -c \
    -o "$W/kern.o" lib/fui/kern.fi
"$FIRNC" --profile=kernel --target=x86_64-none -c \
    -o "$W/stil.o" lib/fui/stil.fi
"$FIRNC" --profile=kernel --target=x86_64-none -c \
    -o "$W/anordnung.o" lib/fui/anordnung.fi
for o in kern stil anordnung; do
    n=$(objdump -d "$W/$o.o" | grep -c syscall || true)
    if [ "$n" != "0" ]; then
        echo "  $o.o: $n syscall-Instruktionen -- im Kern verboten"
        exit 1
    fi
    # Der EINZIGE erlaubte offene Name ist osum_panic (SPEC 2): den
    # stellt das Betriebssystem, das den Kern einbindet.
    fremd=$(nm -u "$W/$o.o" | grep -v osum_panic | wc -l)
    if [ "$fremd" != "0" ]; then
        echo "  $o.o: fremde Namen ausser osum_panic:"
        nm -u "$W/$o.o" | grep -v osum_panic
        exit 1
    fi
    echo "  $o.o: 0 syscall, 0 fremde Namen  OK"
done

echo
echo "== 2. DIE MARKE HAELT IHRE ZUSAGE (WCAG 4.5:1) =="
bau kontrast
"$W/kontrast"

echo
echo "== 3. DAS SCHLIESSKREUZ =="
bau kreuz
"$W/kreuz"

echo
echo "== 4. DAS STILSYSTEM =="
bau stil
"$W/stil"

echo
echo "== 5. DIE ANORDNUNG =="
bau anordnung
"$W/anordnung"

echo
echo "== 6. WELLE 1 AM BILDPUNKT =="
bau welle1
"$W/welle1"

if [ "$1" = "--bilder" ]; then
    echo
    echo "== 7. DIE SCHAUSEITE =="
    Z=/srv/store/belege/fui
    mkdir -p "$Z"
    bau schauseite
    "$W/schauseite" "$Z/fui-welle1-hell.png" hell
    "$W/schauseite" "$Z/fui-welle1-dunkel.png" dunkel
    ls -la "$Z"
fi

echo
echo "ALLE PRUEFUNGEN BESTANDEN."
