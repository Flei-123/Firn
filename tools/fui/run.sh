#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/run.sh -- THE ACCEPTANCE RUN FOR fUi.
#
# One run that checks everything. If a check fails, the build fails --
# that is Justin's point 2 on acceptance, and the answer to the fact
# that the same drawing went SILENTLY missing in OrientOS twice.
#
#     tools/fui/run.sh              check only
#     tools/fui/run.sh --images     additionally paint the gallery
#
# The renders land under /srv/store/belege/fui/.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
# Freestanding-Ziel gibt es nicht in jedem Compiler-Stand; dann ohne --target.
NONE_T="--target=x86_64-none"
if ! "$FIRNC" --help 2>&1 | grep -q "x86_64-none"; then NONE_T=""; fi
W="${W:-/tmp/fui-acceptance}"
mkdir -p "$W"

build() {
    "$FIRNC" --opt-level=dev -o "$W/$1" "tools/fui/$1_main.fi"
}

echo "== 1. THE CORE BUILDS FREESTANDING (profile kernel) =="
# This is Justin's architecture: the same source in the kernel and in
# the application. What is checked: that it builds AND that no syscall
# and no foreign name is left inside.
"$FIRNC" --profile=kernel $NONE_T -c \
    -o "$W/core.o" lib/fui/core.fi
"$FIRNC" --profile=kernel $NONE_T -c \
    -o "$W/style.o" lib/fui/style.fi
"$FIRNC" --profile=kernel $NONE_T -c \
    -o "$W/layout.o" lib/fui/layout.fi
for o in core style layout; do
    n=$(objdump -d "$W/$o.o" | grep -c syscall || true)
    if [ "$n" != "0" ]; then
        echo "  $o.o: $n syscall instructions -- forbidden in the core"
        exit 1
    fi
    # The ONLY permitted open name is osum_panic (SPEC 2): it is
    # provided by the operating system that links the core in.
    foreign=$(nm -u "$W/$o.o" | grep -v osum_panic | wc -l)
    if [ "$foreign" != "0" ]; then
        echo "  $o.o: foreign names besides osum_panic:"
        nm -u "$W/$o.o" | grep -v osum_panic
        exit 1
    fi
    echo "  $o.o: 0 syscall, 0 foreign names  OK"
done

echo
echo "== 2. THE BRAND KEEPS ITS PROMISE (WCAG 4.5:1) =="
build contrast
"$W/contrast"

echo
echo "== 3. THE CLOSE CROSS =="
build capicon
"$W/capicon"

echo
echo "== 4. THE STYLE SYSTEM =="
build style
"$W/style"

echo
echo "== 5. THE LAYOUT =="
build layout
"$W/layout"

echo
echo "== 6. SYMMETRY OF BORDER AND FOCUS RING =="
# Justins Befund vom 10.09.2026: der Fokusring war oben 6 Zeilen dick
# und unten 2, und stand oben rechts zwei Punkte ueber. Ursache war
# der Innenpfad in painter.path_round_ccw (vertauschte Kontrollpunkte,
# ein Segment endete auf seinem eigenen Anfang). Diese Pruefung ZAEHLT
# die vier Kantendicken nach -- dieselbe Sorte Asymmetrie hat uns beim
# Schliesskreuz zwei Runden gekostet.
build symmetry
"$W/symmetry"

echo
echo "== 7. THE ICONS AT THE PIXEL =="
build icon
"$W/icon"

echo
echo "== 8. WAVE 1 AT THE PIXEL =="
build wave1
"$W/wave1"

echo
echo "== 9. WAVE 2 AT THE PIXEL =="
build wave2
"$W/wave2"

echo
echo "== 10. WAVE 3/4 AT THE PIXEL =="
build wave3
"$W/wave3"

echo
echo "== 10b. PICTURES AT THE PIXEL =="
# The blitter: the channels as NUMBERS against the colour asked for (the
# R/B swap in this project was invisible to the eye), the alpha, and the
# box sampling that keeps a shrunk icon free of stairs.
build image
"$W/image"

echo
echo "== 10c. DAS PORTIERTE lib/svg IM FIRN-BAUM =="
# lib/svg kommt aus dem Certus-Baum (13.09.2026, sieben von acht Dateien
# byte-identisch). Geprueft wird, was bei der Portierung angefasst wurde:
# raster_finish_evenodd in der NEUEREN raster.fi, ttf.font_outline_m, und
# der Schrift-Adapter svg/fontsel.fi. Dazu currentColor gegen die
# Akzentfarbe -- Justins Zusatz vom 13.09.2026.
build uisvg
"$W/uisvg"

echo
echo "== 10d. THE PICTURE ON THE WIDGET =="
# Icon left/right/only, the measure, the tint, and the tab and menu entry
# that paint through the borrowed label.
build art
"$W/art"

echo
echo "== 11. THE TEXT VALUES AT THE PIXEL =="
# Tracking, line height, kerning, outline, shadow, gradient. The three
# tracking numbers Justin asked to see are printed by this one.
build text
"$W/text"

echo
echo "== 12. DIE ZEIT: EASINGS, TWEENS, UEBERGAENGE =="
# Runde UI-WEB: lib/fui/anim.fi. Geprueft werden die Easings an ihren
# Stuetzstellen (ease-in-out ist bei 0.5 exakt 0.5), die analytisch
# geloeste Feder (sie kommt zur Ruhe und driftet nicht), der Takt, der
# nur bei echter Aenderung "neu zeichnen" meldet, und die
# Farbmischung mit korrektem Alpha.
build anim
"$W/anim"

echo
echo "== 13. FLEX-LAYOUT AM PIXEL =="
# Jede Zahl hier ist von Hand gerechnet: Verteilung des freien und des
# fehlenden Platzes, Klemmung an min/max und die NACHVERTEILUNG, die
# daraus folgt -- der Ort, an dem eine halbfertige Flexbox eine Luecke
# am rechten Rand laesst.
build flex
"$W/flex"

echo
echo "== 14. UNSCHAERFE, SCHATTEN, FARBMATRIX =="
# Der Kastenweichzeichner gegen die Faltung von Hand und dreimal
# Kasten gegen den kubischen B-Spline (1,3,6,7,6,3,1)/27 -- die
# analytische Antwort, die eine Gauss-Naeherung geben MUSS.
build effect
"$W/effect"

echo
echo "== 15. AFFINE ABBILDUNGEN UND DIE TREFFERPRUEFUNG =="
# Bekannte Punktbilder, der Stapel, die Kehrabbildung -- und der
# Klick unter Drehung, der ohne inverse Abbildung danebengeht.
build transform
"$W/transform"

if [ "$1" = "--images" ]; then
    echo
    echo "== 9. THE GALLERY =="
    Z=/srv/store/belege/fui
    mkdir -p "$Z"
    build gallery
    "$W/gallery" "$Z/fui-wave1-light.png" light
    "$W/gallery" "$Z/fui-wave1-dark.png" dark
    build gallery2
    "$W/gallery2" "$Z/fui-wave2-light.png" light
    "$W/gallery2" "$Z/fui-wave2-dark.png" dark
    build gallery3
    "$W/gallery3" "$Z/fui-wave3-light.png" light
    "$W/gallery3" "$Z/fui-wave3-dark.png" dark
    build gallery4
    "$W/gallery4" "$Z/fui-text-light.png" light
    "$W/gallery4" "$Z/fui-text-dark.png" dark
    # BILD UND SVG (Runde BILD+SVG, 13.09.2026). Fuenf Baender: Knopf mit
    # Icon, Beschriftung mit Bild und Icon-Toolbar, dasselbe SVG je Groesse
    # NEU gerastert (12..64) samt currentColor=TOK_ACCENT daneben, Bild mit
    # Transparenz ueber vier Gruenden, Reiter/Menue/Kachel.
    build artshow
    "$W/artshow" "$Z/fui-bild-svg-hell.png" light
    "$W/artshow" "$Z/fui-bild-svg-dunkel.png" dark
    # RUNDE UI-WEB: die vier neuen Faehigkeiten, hell und dunkel.
    # Eine Bewegung als Phasenreihe, die Flex-Varianten nebeneinander,
    # Schatten/Glas/Farbmatrix ueber gemustertem Grund, und gedrehte,
    # skalierte, gescherte Kacheln.
    build gallery5
    "$W/gallery5" "$Z/fui-anim-hell.png" light
    "$W/gallery5" "$Z/fui-anim-dunkel.png" dark
    build gallery6
    "$W/gallery6" "$Z/fui-flex-hell.png" light
    "$W/gallery6" "$Z/fui-flex-dunkel.png" dark
    build gallery7
    "$W/gallery7" "$Z/fui-effekt-hell.png" light
    "$W/gallery7" "$Z/fui-effekt-dunkel.png" dark
    build gallery8
    "$W/gallery8" "$Z/fui-transform-hell.png" light
    "$W/gallery8" "$Z/fui-transform-dunkel.png" dark
    ls -la "$Z"
fi

echo
echo "ALL CHECKS PASSED."
