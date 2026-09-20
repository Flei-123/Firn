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
# The renders land under $BELEGE, by default $W/belege -- inside the
# run's own working directory, so that the run does not depend on write
# access to a path somewhere else in the system.
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
# DIE ZWEITE HAELFTE DERSELBEN PRUEFUNG. Es gibt in diesem Baum zwei
# Kastenweichzeichner -- lib/paint/painter.blur_line auf
# DECKUNGSGRADEN (f64) und lib/fui/effect.blur_buffer auf der
# PREMULTIPLIZIERTEN u8-LEINWAND. Dass das kein zweiter Ort fuer
# dieselbe Sache ist, steht im Kopf von effect.fi; dass beide bei
# demselben sigma denselben Tonwert liefern, rechnen die beiden
# Programme gegen DIESELBE Tabelle nach. Zwei sind es, weil Firn ein
# Modul nach dem letzten Pfadabschnitt aufloest und `fui.painter` und
# `paint.painter` darum nicht in ein Programm passen.
build blurref
"$W/blurref"

echo
echo "== 15. AFFINE ABBILDUNGEN UND DIE TREFFERPRUEFUNG =="
# Bekannte Punktbilder, der Stapel, die Kehrabbildung -- und der
# Klick unter Drehung, der ohne inverse Abbildung danebengeht.
build transform
"$W/transform"

echo
echo "== 16. DIE BEDIENUNG: FOKUSKETTE UND ZEIGER =="
# tools/fui/control_main.fi lag seit seiner Entstehung NEBEN dem
# Prueflauf: es rechnet nach, aber niemand rief es. Genau so geht eine
# Pruefung schweigend verloren -- der Grund, aus dem es diese Datei
# gibt. Also haengt es jetzt hier drin. Wichtig fuer die Runde UI-WEB:
# hier liegt die Trefferpruefung, auf die lib/fui/transform.fi den Punkt
# mit der Kehrabbildung zurueckrechnet.
build control
"$W/control"

echo
echo "== 17. DER ZEILENUMBRUCH =="
# Ebenfalls nachgetragen: der Umbruch pruefte sich selbst, ohne dass
# der Lauf davon wusste.
build wrap
"$W/wrap"

echo
echo "== 18. DIE BEDIENUNG DES TEXTFELDES =="
# lib/fui/editor.fi liess sich seit seinem ersten Tag NICHT uebersetzen
# (eine fehlende `if`-Zeile im Zweig fuer die Ruecktaste), und es fiel
# nicht auf, weil kein Programm die Datei einband und dieser Lauf sie
# nicht kannte. Jetzt wird sie gebaut UND gerechnet: Strg+A ersetzt
# statt anzuhaengen (Justins Adresszeilen-Fehler), die Wortspruenge,
# Strg+Rueck/Entf mit und ohne Auswahl, Ctrl+Z/Y und die Tasten, die
# dem Feld nicht gehoeren.
build editor
"$W/editor"

echo
echo "== 19. JEDE PRUEFDATEI BAUT, UND JEDE KOMMT IM LAUF VOR =="
# DER FEHLER, DEN DIESER ABSCHNITT UNMOEGLICH MACHT. lib/fui/editor.fi
# liess sich einen Monat lang nicht uebersetzen, tools/fui/control_main.fi
# und wrap_main.fi rechneten fuer niemanden, und tools/fui/preview_main.fi
# stand ganz ausserhalb: keine dieser Dateien kam in diesem Lauf vor,
# also fiel nichts auf. Eine Pruefung, die niemand ruft, ist keine.
#
# Deshalb hier zweierlei, MASCHINELL und nicht nach Gedaechtnis:
#   1. JEDE Datei tools/fui/*_main.fi und demos/*/main.fi wird
#      uebersetzt -- auch die Demos ausserhalb von fUi, denn was
#      niemand baut, hoert irgendwann auf zu bauen.
#   2. JEDE Datei tools/fui/*_main.fi und demos/fuidemo/main.fi muss in
#      diesem Skript VORKOMMEN. (Die uebrigen Demos gehoeren anderen
#      Baeumen; sie werden gebaut, aber nicht hier gerechnet.)
for f in tools/fui/*_main.fi demos/*/main.fi; do
    "$FIRNC" --opt-level=dev -o "$W/baupruefung" "$f" >/dev/null
done
echo "  alle tools/fui/*_main.fi und demos/*/main.fi uebersetzen  OK"
fehlt=0
for f in tools/fui/*_main.fi demos/fuidemo/main.fi; do
    n=$(basename "$f" _main.fi)
    case "$f" in
        demos/*) n="fuidemo" ;;
    esac
    if ! grep -q "$n" "tools/fui/run.sh"; then
        echo "  $f kommt in tools/fui/run.sh NICHT vor -- eine Pruefung,"
        echo "  die niemand ruft, ist keine. Haenge sie ein."
        fehlt=1
    fi
done
if [ "$fehlt" != "0" ]; then
    exit 1
fi
echo "  jede von ihnen wird in diesem Lauf auch gerufen            OK"

if [ "$1" = "--images" ]; then
    echo
    echo "== 9. THE GALLERY =="
    # WOHIN DIE BELEGE GEHEN. Die Vorgabe liegt INNERHALB des
    # Arbeitsverzeichnisses ($W), nicht unter einem festen Systempfad:
    # ein Lauf, der ausserhalb seines eigenen Baums schreibt, faellt bei
    # jedem, der dort keine Rechte hat, auf die Nase -- und zwar erst
    # nach zwanzig bestandenen Abschnitten. Wer die Bilder woanders
    # haben will, setzt BELEGE.
    Z="${BELEGE:-$W/belege}"
    if ! mkdir -p "$Z" 2>/dev/null; then
        echo "  FEHLER: das Belegverzeichnis \"$Z\" laesst sich nicht anlegen."
        echo "  Der Lauf ist damit NICHT bestanden. Setze BELEGE auf einen"
        echo "  beschreibbaren Pfad und starte erneut."
        exit 1
    fi
    # Anlegen heisst noch nicht beschreiben duerfen (ein vorhandenes,
    # fremdes Verzeichnis legt mkdir -p klaglos nicht neu an). Also
    # einmal wirklich schreiben -- lieber hier scheitern als ein Bild
    # weniger ausliefern und trotzdem "ALL CHECKS PASSED" drucken.
    if ! : > "$Z/.schreibprobe" 2>/dev/null; then
        echo "  FEHLER: in \"$Z\" laesst sich nicht schreiben."
        echo "  Der Lauf ist damit NICHT bestanden."
        exit 1
    fi
    rm -f "$Z/.schreibprobe"

    # DIE SCHRIFT, EINMAL UND VORHER. Jedes Belegprogramm bricht seit
    # dieser Runde mit einem Fehler ab, wenn es keine Schrift laden
    # kann -- ein Bild ohne einen einzigen Buchstaben belegt nichts.
    # Hier wird dieselbe Datei EINMAL vorher geprueft, damit der Lauf
    # nicht erst nach dem zwanzigsten Abschnitt an zehn Programmen
    # hintereinander scheitert und niemand die Ursache sieht.
    SCHRIFT="${SCHRIFT:-/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf}"
    if [ ! -r "$SCHRIFT" ]; then
        echo "  FEHLER: die Schrift \"$SCHRIFT\" ist nicht lesbar."
        echo "  Ohne Schrift entstehen textlose Belege, und ein Beleg"
        echo "  ohne Beschriftung belegt nichts. Installiere DejaVuSans"
        echo "  oder setze SCHRIFT auf eine vorhandene TTF-Datei."
        exit 1
    fi
    echo "  Schrift gefunden: $SCHRIFT"

    # DIE BELEGPRUEFUNG. Sie liest jedes geschriebene PNG WIEDER EIN
    # und rechnet nach: Abmessungen, Zahl verschiedener Farben, und
    # dass in JEDEM der sechs waagerechten Baender wirklich etwas
    # steht. Die Zahlen hinter jedem Aufruf sind Breite, kleinste und
    # groesste zulaessige Hoehe (drei Belege schneiden ihre Leinwand
    # auf den Inhalt zu) und die geforderte Farbvielfalt.
    build belegpruef
    beleg() {
        "$W/belegpruef" "$@"
    }
    build gallery
    "$W/gallery" "$Z/fui-wave1-light.png" light
    "$W/gallery" "$Z/fui-wave1-dark.png" dark
    beleg "$Z/fui-wave1-light.png" 1200 400 760 200
    beleg "$Z/fui-wave1-dark.png" 1200 400 760 200
    build gallery2
    "$W/gallery2" "$Z/fui-wave2-light.png" light
    "$W/gallery2" "$Z/fui-wave2-dark.png" dark
    beleg "$Z/fui-wave2-light.png" 1240 700 1180 200
    beleg "$Z/fui-wave2-dark.png" 1240 700 1180 200
    build gallery3
    "$W/gallery3" "$Z/fui-wave3-light.png" light
    "$W/gallery3" "$Z/fui-wave3-dark.png" dark
    beleg "$Z/fui-wave3-light.png" 1240 700 1320 200
    beleg "$Z/fui-wave3-dark.png" 1240 700 1320 200
    build gallery4
    "$W/gallery4" "$Z/fui-text-light.png" light
    "$W/gallery4" "$Z/fui-text-dark.png" dark
    beleg "$Z/fui-text-light.png" 1240 600 1180 200
    beleg "$Z/fui-text-dark.png" 1240 600 1180 200
    # BILD UND SVG (Runde BILD+SVG, 13.09.2026). Fuenf Baender: Knopf mit
    # Icon, Beschriftung mit Bild und Icon-Toolbar, dasselbe SVG je Groesse
    # NEU gerastert (12..64) samt currentColor=TOK_ACCENT daneben, Bild mit
    # Transparenz ueber vier Gruenden, Reiter/Menue/Kachel.
    build artshow
    "$W/artshow" "$Z/fui-bild-svg-hell.png" light
    "$W/artshow" "$Z/fui-bild-svg-dunkel.png" dark
    beleg "$Z/fui-bild-svg-hell.png" 900 400 620 200
    beleg "$Z/fui-bild-svg-dunkel.png" 900 400 620 200
    # RUNDE UI-WEB: die vier neuen Faehigkeiten, hell und dunkel.
    # Eine Bewegung als Phasenreihe, die Flex-Varianten nebeneinander,
    # Schatten/Glas/Farbmatrix ueber gemustertem Grund, und gedrehte,
    # skalierte, gescherte Kacheln.
    build gallery5
    "$W/gallery5" "$Z/fui-anim-hell.png" light
    "$W/gallery5" "$Z/fui-anim-dunkel.png" dark
    beleg "$Z/fui-anim-hell.png" 1240 500 764 200
    beleg "$Z/fui-anim-dunkel.png" 1240 500 764 200
    build gallery6
    "$W/gallery6" "$Z/fui-flex-hell.png" light
    "$W/gallery6" "$Z/fui-flex-dunkel.png" dark
    beleg "$Z/fui-flex-hell.png" 1240 600 880 200
    beleg "$Z/fui-flex-dunkel.png" 1240 600 880 200
    build gallery7
    "$W/gallery7" "$Z/fui-effekt-hell.png" light
    "$W/gallery7" "$Z/fui-effekt-dunkel.png" dark
    beleg "$Z/fui-effekt-hell.png" 1240 400 640 200
    beleg "$Z/fui-effekt-dunkel.png" 1240 400 640 200
    build gallery8
    "$W/gallery8" "$Z/fui-transform-hell.png" light
    "$W/gallery8" "$Z/fui-transform-dunkel.png" dark
    beleg "$Z/fui-transform-hell.png" 1280 500 760 200
    beleg "$Z/fui-transform-dunkel.png" 1280 500 760 200
    # DIE UEBERSICHT AUS DER ERSTEN STUNDE. tools/fui/preview_main.fi
    # malt die Grundelemente in allen Zustaenden; sie lag seit ihrer
    # Entstehung NEBEN diesem Lauf -- gebaut hat sie niemand, gerechnet
    # erst recht nicht. Jetzt entsteht ihr Bild hier und wird
    # nachgerechnet wie jeder andere Beleg.
    build preview
    "$W/preview" "$Z/fui-preview-hell.png" light
    "$W/preview" "$Z/fui-preview-dunkel.png" dark
    beleg "$Z/fui-preview-hell.png" 760 200 240 120
    beleg "$Z/fui-preview-dunkel.png" 760 200 240 120
    # DIE DEMO-ANWENDUNG. Kein Pruefblatt, sondern eine Oberflaeche, wie
    # ein Anwender sie schreibt: Titelzeile und Werkzeugleiste von
    # `flex.flex_layout` verteilt, der Hover-Uebergang eines Knopfes aus
    # `anim.Animator`, der Dialogschatten aus
    # `effect.drop_shadow_spread`. Sie liegt unter demos/ und nicht
    # unter tools/fui, weil sie den NORMALEN Weg zeigt -- und sie laeuft
    # hier mit, damit ein Bruch in einem der drei Module auffliegt,
    # bevor der naechste Anwender darueber stolpert. Das Programm
    # rechnet selbst nach, dass seine Phasen sich nicht ueberdecken,
    # und endet sonst mit einem Fehler (set -e bricht den Lauf ab).
    "$FIRNC" --opt-level=dev -o "$W/fuidemo" demos/fuidemo/main.fi
    "$W/fuidemo" "$Z/fui-demo-hell.png" light
    "$W/fuidemo" "$Z/fui-demo-dunkel.png" dark
    beleg "$Z/fui-demo-hell.png" 1000 350 500 200
    beleg "$Z/fui-demo-dunkel.png" 1000 350 500 200
    ls -la "$Z"
fi

echo
echo "ALL CHECKS PASSED."
