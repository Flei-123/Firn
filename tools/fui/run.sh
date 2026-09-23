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
# DAS FREISTEHEND-ZIEL, UND WAS PASSIERT, WENN ES FEHLT. Frueher stand
# hier: gibt es "x86_64-none" nicht, dann eben ohne --target. Damit war
# Abschnitt 1 auf jedem Compiler-Stand ohne Freistehend-Ziel STILL
# gruen, obwohl seine Ueberschrift etwas anderes behauptete -- genau
# die Sorte weggelassene Pruefung, gegen die dieser Lauf geschrieben
# ist. Jetzt wird die Ersatzpruefung BENANNT und selbst geprueft
# (siehe Abschnitt 1); faellt sie aus, bricht der Lauf ab.
NONE_T="--target=x86_64-none"
FREISTEHEND_ZIEL="ja"
if ! "$FIRNC" --help 2>&1 | grep -q "x86_64-none"; then
    NONE_T=""
    FREISTEHEND_ZIEL="nein"
fi
W="${W:-/tmp/fui-acceptance}"
mkdir -p "$W"

build() {
    "$FIRNC" --opt-level=dev -o "$W/$1" "tools/fui/$1_main.fi"
}

echo "== 1. THE CORE BUILDS FREESTANDING (profile kernel) =="
# This is Justin's architecture: the same source in the kernel and in
# the application. What is checked: that it builds AND that no syscall
# and no foreign name is left inside.
#
# WENN DAS FREISTEHEND-ZIEL FEHLT. Dieser Compiler-Stand kennt nur
# x86_64-linux und aarch64-linux; ein Ziel "x86_64-none" gibt es (noch)
# nicht. Das Weglassen von --target darf aber nicht heissen, dass von
# der Ueberschrift nichts mehr geprueft wird. Also wird an seiner
# Stelle nachgewiesen, dass das Profil `kernel` ZAEHNE hat: es muss
# `syscall` und den Import von std.io ABLEHNEN. Ein Profil, das beides
# durchlaesst, waere ein Etikett, und dann waere "0 syscall
# instructions" weiter unten nur ein Zufall des Quelltextes. Scheitert
# dieser Ersatznachweis, ist der Lauf NICHT bestanden -- still
# weiterlaufen tut er nicht mehr.
if [ "$FREISTEHEND_ZIEL" = "ja" ]; then
    echo "  Freistehend-Ziel x86_64-none vorhanden, es wird gebaut     OK"
else
    echo "  dieser Compiler kennt kein Freistehend-Ziel (x86_64-none);"
    echo "  an seiner Stelle wird das Profil kernel selbst geprueft."
    printf 'profile kernel\n\nfn f() {\n    syscall(60, 0)\n}\n' \
        > "$W/kernelprobe_syscall.fi"
    printf 'profile kernel\nimport std.io\n\nfn f() {\n    io.append_line(0 as u64, 0 as u64, 0 as usize)\n}\n' \
        > "$W/kernelprobe_import.fi"
    zaehne=0
    if "$FIRNC" --profile=kernel -c -o "$W/kernelprobe.o" \
        "$W/kernelprobe_syscall.fi" >/dev/null 2>&1; then
        echo "  das Profil kernel nimmt einen syscall an -- es hat keine Zaehne."
        zaehne=1
    fi
    if "$FIRNC" --profile=kernel -c -o "$W/kernelprobe.o" \
        "$W/kernelprobe_import.fi" >/dev/null 2>&1; then
        echo "  das Profil kernel nimmt import std.io an -- es hat keine Zaehne."
        zaehne=1
    fi
    if [ "$zaehne" != "0" ]; then
        echo "  Abschnitt 1 ohne Freistehend-Ziel geprueft -- NICHT bestanden"
        exit 1
    fi
    echo "  Profil kernel weist syscall UND import std.io zurueck      OK"
fi
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
# Abschnitt 12b liest die Zahlen AUS DEM RENDER-PFAD: render.Ctx fuehrt
# seit der Runde FEHLERBEHEBUNG ein Register laufender Uebergaenge
# (anim.TransReg), und der Bildpunkt in der Mitte des gemalten Knopfes
# geht ueber drei Bilder von 0x42414D nach 0x52525E. Ohne das Register
# springt er sofort -- die Gegenprobe steht daneben.
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
# Abschnitt 16 dieser Datei geht den NORMALEN Weg: ein um 24 Grad
# gedrehter Knopf, gemalt mit wave2.draw_any_xf (also ueber
# render.draw_widget_xf), bedient mit control.mouse_down -- und der
# Punkt, an dem er gemalt wurde, ist derselbe, an dem er getroffen
# wird. Ohne transform.tf_bind_panel geht genau dieser Klick daneben,
# und auch das steht dort als Zahl.
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
echo "== 18a. DIE EINGABEMETHODE: CHINESISCH, JAPANISCH, KOREANISCH =="
# lib/fui/ime.fi mit seinem Anschluss an editor.fi. Nachgerechnet
# werden Romaji nach Kana (Doppelkonsonant, n-Regel, tch), die
# Hangul-Silben nach Unicode Kapitel 3.12 (zusammensetzen UND zerlegen,
# jede Zahl steht ausgerechnet daneben), der wandernde Auslaut, der
# Vorbearbeitungstext mit dem Schreibzeiger davor und dahinter, die
# Auswahl, die erst beim Bestaetigen ersetzt wird und beim Abbrechen
# wiederkommt, die Kandidaten, Pinyin gegen die mitgelieferte Tabelle
# und die Plattform-Schnittstelle ueber eine nachgeahmte Plattform.
# Das Bild dazu (Feld mit offener Kandidatenliste) entsteht unten bei
# --images mit tools/fui/imebeleg_main.fi.
build ime
"$W/ime"

echo
echo "== 18b. DER SCHEIBENKASTEN =="
# lib/fui/viewport.fi: der Ausschnitt, der mehr Inhalt aufnimmt, als er
# hoch ist. Nachgerechnet werden die Rechnung ueber Kreuz (ein
# senkrechter Balken macht einen waagerechten noetig), der sichtbare
# Ausschnitt, die Klemmung, Anteil und Stellung des Rollbalkens aus dem
# Verhaeltnis Ausschnitt/Inhalt, `viewport_ensure_visible`, die
# Trefferpruefung UNTER VERSCHIEBUNG und das kinetische Auslaufen ueber
# lib/fui/anim.fi. Das harte Clipping wird auf der Leinwand gemessen:
# kein Bildpunkt ausserhalb des Ausschnitts darf gesetzt sein, und die
# Gegenprobe OHNE Clip muss derselbe Test rot melden.
build viewport
"$W/viewport"

echo
echo "== 18c. DAS STILBLATT: RANGFOLGE UND VERERBUNG =="
# lib/fui/sheet.fi. Die Faelle widersprechen sich mit Absicht:
# Kennung schlaegt Klasse schlaegt Art, bei Gleichstand gewinnt die
# spaeter geschriebene Regel, und eine ranghohe Regel, die nur die
# Schriftfarbe nennt, loescht keinen Grund. Dazu die Vererbung UND
# ihre Grenze -- vier Werte gehen ueber, der Grund nicht.
build sheet
"$W/sheet"

echo
echo "== 18d. DER BESCHRIEBENE BAUM =="
# lib/fui/scene.fi: Baum, Stil, Messen, Anordnen, Zeichnen -- in
# dieser Reihenfolge und in getrennten Durchgaengen. Geprueft wird
# unter anderem, dass beim Zeichnen keine Groesse mehr driftet
# (`scene_size_drift` ist 0) und dass die Trefferpruefung den obersten
# Knoten liefert.
build scene
"$W/scene"

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
    # GESUCHT WIRD DER AUFRUF, NICHT DER NAME. Frueher stand hier
    # `grep -q "$n"`, und das war eine Wache, die sich selbst betrog:
    # "anim" steckt in "gallery5" nicht, aber in jedem Kommentar ueber
    # anim.fi, "image" in "--images", "text" in "kontext". Ein Programm
    # galt damit als gerufen, sobald sein Name IRGENDWO in dieser Datei
    # vorkam -- also genau dann auch, wenn niemand es ruft. Gesucht wird
    # darum die Zeichenkette "$W/<name>", mit der dieses Skript ein
    # gebautes Programm ausfuehrt, und zwar als fester Text (-F), damit
    # kein Sonderzeichen sie zu einem Muster macht.
    if ! grep -qF "\"\$W/$n\"" "tools/fui/run.sh"; then
        echo "  $f wird in tools/fui/run.sh NICHT gerufen (kein \"\$W/$n\")"
        echo "  -- eine Pruefung, die niemand ruft, ist keine. Haenge sie ein."
        fehlt=1
    fi
done
if [ "$fehlt" != "0" ]; then
    exit 1
fi
echo "  jede von ihnen wird in diesem Lauf wirklich gerufen       OK"

# EIN ORT FUER floor/ceil/abs, UND NICHT ZWEI. `std.math` und
# lib/svg/matrix.fi koennen beide abrunden. Solange beide in lib/fui/
# benutzt wurden, rechneten zwei Module dieselbe Rasterkante mit zwei
# verschiedenen Abrundungen aus, und die Kante lag um einen Bildpunkt
# daneben -- der teuerste Fehler dieses Baums ist der zweite Ort fuer
# dieselbe Sache. Festgeschrieben ist es im Kopf von lib/fui/anim.fi:
# in lib/fui/ gilt ausschliesslich matrix.m_floor/m_ceil/m_abs. Hier
# wird der jeweils andere Name maschinell verboten.
#
# Kommentarzeilen sind ausgenommen: der Kopf von anim.fi MUSS den
# verbotenen Namen nennen duerfen, um ihn zu verbieten.
verboten=$(grep -n "math\.\(floor\|ceil\|abs\|fabs\)" lib/fui/*.fi \
    | grep -v "^[^:]*:[0-9]*: *//" || true)
if [ -n "$verboten" ]; then
    echo "  in lib/fui/ steht math.floor/ceil/abs -- verboten. Es gilt"
    echo "  matrix.m_floor/m_ceil/m_abs (siehe Kopf von lib/fui/anim.fi):"
    echo "$verboten"
    exit 1
fi
echo "  floor/ceil/abs kommen in lib/fui/ nur aus svg.matrix        OK"

# KEINE ZIFFER ALS SCHRITTWEITE UEBER EINE STRUKTUR. In
# demos/fuidemo/main.fi stand dreimal `(i * 32)`, um in ein Feld von
# `layout.Rect` zu greifen. Das ist heute richtig und morgen falsch:
# kommt in `Rect` ein Feld dazu, sagt kein Uebersetzer etwas, und ab da
# wird mitten in ein Rechteck hinein gelesen -- der Fehler faellt als
# verschobenes Bild auf, nicht als Meldung. Die Schrittweite wird
# darum GEMESSEN (layout.rect_stride, sheet.sheet_desc_stride, beide
# nach demselben Muster: Differenz zweier Nachbarn eines echten
# Feldes), und hier wird die Ziffer maschinell verboten.
#
# Gesucht werden Zeilen, die eine Zahl mit einem Index malnehmen UND im
# selben Atemzug auf einen STRUKTURZEIGER (`as *mut modul.Typ`)
# umdeuten. Die Schrittweiten der Grundtypen (4 fuer u32, 8 fuer i64
# und f64) bleiben erlaubt: die aendern sich nicht, wenn jemand ein
# Feld hinzufuegt. Kommentarzeilen sind ausgenommen, denn dieser Text
# hier darf die verbotene Form nennen duerfen.
ziffern=$(grep -n -E '\* *[0-9]+\)? *as u64.*as \*mut [a-z_]+\.[A-Z]' \
    demos/*/main.fi tools/fui/*.fi \
    | grep -v "^[^:]*:[0-9]*: *//" || true)
if [ -n "$ziffern" ]; then
    echo "  eine Ziffer als Schrittweite ueber eine Struktur --"
    echo "  nimm layout.rect_stride() bzw. sheet.sheet_desc_stride():"
    echo "$ziffern"
    exit 1
fi
echo "  keine Ziffer als Schrittweite in demos/ und tools/fui/     OK"

# JEDER IMPORT WIRD AUCH GERUFEN. In lib/fui/scene.fi stand `import
# std.rt`, obwohl in der ganzen Datei kein einziges `rt.` vorkam -- und
# genau so kommt eine Abhaengigkeit in ein Modul, das freistehend
# gebaut werden soll: nicht durch einen Aufruf, sondern durch eine
# Zeile, die niemand mehr liest. Das laesst sich maschinell ausschliessen
# und wird darum maschinell ausgeschlossen: zu JEDEM `import x.y` in
# lib/fui/*.fi muss im SELBEN Modul `y.` vor einem Buchstaben stehen
# (klein oder gross -- `rt.buf_new` genauso wie `rt.Buf`).
#
# Gesucht wird in den Zeilen, die KEIN Kommentar und KEINE Importzeile
# sind: der Kopf eines Moduls darf ueber `viewport.fi` schreiben, ohne
# damit einen Import zu rechtfertigen, und `import fui.style` selbst ist
# kein Gebrauch von `style.`.
ungenutzt=0
for f in lib/fui/*.fi; do
    for mod in $(sed -n 's/^import [a-z0-9_]*\.\([a-z0-9_]*\)[[:space:]]*$/\1/p' \
        "$f"); do
        if ! grep -v -e '^[[:space:]]*//' -e '^import ' "$f" \
            | grep -q "$mod\.[A-Za-z]"; then
            echo "  $f: import ...$mod, aber kein \"$mod.\" im Modul --"
            echo "  ein Import, den niemand ruft, ist eine Abhaengigkeit"
            echo "  ohne Gegenleistung. Streiche die Zeile."
            ungenutzt=1
        fi
    done
done
if [ "$ungenutzt" != "0" ]; then
    exit 1
fi
echo "  jeder Import in lib/fui/ wird im Modul auch gerufen        OK"

echo
echo "== 19b. KEINE BESCHRIFTUNG WIRD UNTERWEGS ABGESCHNITTEN =="
# DER FEHLER, DER "red fixe" HIESS. In tools/fui/gallery_main.fi stand
# `var t_v3: [u8; 10] = "red fixed "`, gemalt wurden acht Bytes, und im
# Beleg las ein Pruefer "red fixe". Daneben malten drei Kacheln
# derselben Datei die ersten zwei Bytes von "12 15 20", also dreimal
# "12", obwohl die Schrift 12, 15 und 20 Punkt gross war. Beide Bilder
# haben jede bestehende Pruefung bestanden: die Abmessungen stimmten,
# die Farbvielfalt stimmte, jedes Band war bemalt -- nur das WORT war
# kaputt. Ein Beleg mit einem halben Wort belegt das Gegenteil von dem,
# was er behauptet.
#
# tools/fui/belegpruef_main.fi rechnet das jetzt maschinell nach
# (Betriebsart --ketten): zu jeder gemalten Kette `(&name[0]) as u64, M`
# wird die Deklaration `var name: [u8; N] = "..."` gesucht und
# N - Fuellung <= M <= N gefordert. Hier laeuft die Laengenrechnung ueber
# JEDE Quelle des Baums -- ohne Schrift ("-") und ohne Breite (0), damit
# sie auch auf einem Rechner ohne DejaVu laeuft. Die Breitenrechnung mit
# `render.pref_of` steht weiter unten bei den Belegbildern, wo die
# Leinwandbreite jedes Blattes bekannt ist.
build belegpruef
"$W/belegpruef" --ketten - 0 tools/fui/*_main.fi demos/*/main.fi

echo
echo "== 19c. BESCHREIBEN IST KUERZER ALS MALEN, IN ZAHLEN =="
# DIE ZAHL, DIE DEN GANZEN AUFWAND RECHTFERTIGT -- UND ZWAR EHRLICH
# ZUGESCHNITTEN. lib/fui/scene.fi und sheet.fi sind nur dann etwas wert,
# wenn DASSELBE Stueck Oberflaeche beschrieben kuerzer ist als gemalt.
# Verglichen wird darum nicht Datei gegen Datei (das waere Aepfel gegen
# Birnen, tools/fui/gallery9_main.fi zeigt mehr), sondern EIN Stueck,
# das es zweimal gibt: die Werkzeugleiste -- drei Knoepfe, ein Suchfeld
# mit grow, ein Knopf im Akzent.
#
#   gemalt      demos/fuidemo/main.fi, fn werkzeugleiste_gemalt.
#   beschrieben tools/fui/gallery9_main.fi, zwischen den Marken
#               ">>> WERKZEUGLEISTE" und "<<< WERKZEUGLEISTE" (der Baum)
#               UND zwischen ">>> LEISTENREGELN" und "<<<
#               LEISTENREGELN" (ihr Aussehen im Stilblatt).
#
# WAS AN DIESEM ZUSCHNITT NEU IST, UND WARUM. Bis zum 21.09.2026 standen
# auf der gemalten Seite noch die beiden Helfer `setze` und
# `item_von_widget` mit der Begruendung, es gebe sie "nur dafuer" -- das
# war falsch: `titelzeile` und `dialog` rufen beide, also sind es
# GETEILTE Zeilen, und wer geteilte Zeilen nur einer Seite zuschlaegt,
# rechnet sich die Ersparnis schoen. Sie sind heraus. Dafuer zaehlt die
# beschriebene Seite jetzt AUCH ihr Aussehen mit: die Regel fuer die
# Klasse `leiste`, den Radius der Knoepfe und den Akzent -- auf der
# gemalten Seite steht genau das mitten in der Funktion. Dass zwei
# dieser Regeln zugleich die Kopfzeile einfaerben, wird der
# beschriebenen Seite dabei voll angerechnet.
#
# Gezaehlt werden Zeilen mit Code: ohne Leerzeilen, ohne Kommentar. Und
# gezaehlt wird in DREI Zuschnitten, weil eine einzige Zahl hier
# zwangslaeufig etwas verschweigt:
#
#   roh   alles, was in den Marken bzw. in der Funktion steht.
#   A     ohne die Beschriftungen (Textfelder und das Setzen des
#         Textes). Die stehen auf der beschriebenen Seite in
#         `schreibe_texte`, also ausserhalb der Marken -- sie werden
#         darum auf BEIDEN Seiten abgezogen und nicht einseitig.
#   B     zusaetzlich ohne das Aussehen (Flaeche, Rand, Farbe, Radius)
#         auf beiden Seiten. Uebrig bleibt die reine Gliederung, und
#         genau dort ist die Beschreibung um ein Vielfaches kuerzer:
#         die Verteilung, das Setzen jedes Rechtecks und die eigene
#         Zeichenschleife fallen ganz weg.
#
# Die Grenzen unten sind mit dem heutigen Stand gemessen und mit
# Spielraum gesetzt; bricht eine, ist entweder eine Marke verrutscht
# oder die Aussage im Kopf von gallery9_main.fi stimmt nicht mehr -- und
# eine Zahl, die nicht mehr stimmt, ist schlimmer als keine.
zaehle_code() {
    sed -e 's/^[[:space:]]*//' "$1" | grep -c -v -e '^$' -e '^//'
}
ohne_text() {
    grep -v -e 'var [tu][0-9]: \[u8;' -e 'w_set_text(' -e 'node_set_text('
}
ohne_aussehen() {
    grep -v -e 'painter\.round_rect' -e 'painter\.round_ring' \
        -e 'theme\.opaque' -e 'style\.style_set' -e 'style\.color_token' \
        -e 'let rs:' -e 'sheet\.decl_new' -e 'style\.style_new' \
        -e 'sheet\.decl_set_style' -e 'regel(sh' -e 'sheet\.sheet_rule'
}
awk '/^fn werkzeugleiste_gemalt\(/{p=1} p{print} p&&/^}$/{exit}' \
    demos/fuidemo/main.fi > "$W/gemalt.txt"
sed -n '/>>> WERKZEUGLEISTE/,/<<< WERKZEUGLEISTE/p' \
    tools/fui/gallery9_main.fi > "$W/beschrieben.txt"
sed -n '/>>> LEISTENREGELN/,/<<< LEISTENREGELN/p' \
    tools/fui/gallery9_main.fi >> "$W/beschrieben.txt"
ohne_text < "$W/gemalt.txt" > "$W/gemalt_a.txt"
ohne_text < "$W/beschrieben.txt" > "$W/beschrieben_a.txt"
ohne_aussehen < "$W/gemalt_a.txt" > "$W/gemalt_b.txt"
ohne_aussehen < "$W/beschrieben_a.txt" > "$W/beschrieben_b.txt"
gemalt=$(zaehle_code "$W/gemalt.txt")
beschrieben=$(zaehle_code "$W/beschrieben.txt")
gemalt_a=$(zaehle_code "$W/gemalt_a.txt")
beschrieben_a=$(zaehle_code "$W/beschrieben_a.txt")
gemalt_b=$(zaehle_code "$W/gemalt_b.txt")
beschrieben_b=$(zaehle_code "$W/beschrieben_b.txt")
echo "  roh: gemalt $gemalt Zeilen, beschrieben $beschrieben Zeilen"
echo "  A (ohne Texte): gemalt $gemalt_a, beschrieben $beschrieben_a"
echo "  B (ohne Texte und Aussehen): gemalt $gemalt_b, beschrieben $beschrieben_b"
if [ "$gemalt" -lt 40 ] || [ "$beschrieben" -lt 30 ]; then
    echo "  FEHLER: eine der beiden Seiten wurde nicht gefunden."
    echo "  Es fehlen die Marken WERKZEUGLEISTE/LEISTENREGELN in"
    echo "  tools/fui/gallery9_main.fi oder fn werkzeugleiste_gemalt in"
    echo "  demos/fuidemo/main.fi."
    exit 1
fi
if [ "$gemalt_b" -lt 30 ] || [ "$beschrieben_b" -lt 15 ]; then
    echo "  FEHLER: die Filter ohne_text/ohne_aussehen haben zu viel"
    echo "  weggenommen -- der Zuschnitt B ist damit keine Messung mehr."
    exit 1
fi
# DIE ALTE SCHRANKE, WIEDER DA, UND ZWAR AUF DEM ROHWERT. Bis zum
# 21.09.2026 galt hier "die beschriebene Fassung faellt auf hoechstens
# die HAELFTE"; am 22.09.2026 wurde sie durch 90 % (A) und 66 % (B)
# ersetzt, weil der Rohwert damals 42 von 64 war. Eine Pruefung, die
# man weicher macht, damit der eigene Stand sie besteht, ist keine
# Pruefung mehr. Also steht sie wieder da, und stattdessen ist die
# BESCHREIBUNG kuerzer geworden: lib/fui/scene.fi hat mit scene_box,
# scene_widget, node_set_space, node_set_flexitem und scene_run die
# Kurzformen bekommen, die ein Baum wirklich braucht, und
# lib/fui/sheet.fi mit sheet_rule die Regel aus einem Stil.
#
# Gemessen am 23.09.2026: 30 von 64 Zeilen, also 47 %. Dazugekommen
# ist an diesem Tag die GROESSE als Stilwert (style.SF_WIDTH /
# SF_HEIGHT): Leistenhoehe und Zeilenhoehe stehen jetzt in der Regel
# und nicht mehr als Zahl an jedem Knoten im Baum.
if [ $((beschrieben * 2)) -gt "$gemalt" ]; then
    echo "  FEHLER: die beschriebene Fassung faellt nicht auf hoechstens"
    echo "  die Haelfte der gemalten ($beschrieben von $gemalt Zeilen)."
    exit 1
fi
# Zuschnitt A: mit dem Aussehen auf beiden Seiten bleibt die
# Beschreibung kuerzer, aber nicht halb so lang -- ein Stilblatt
# schreibt Farbe und Radius EINMAL fuer die ganze Seite, in diesem
# Vergleich zaehlt das trotzdem gegen sie. Gefordert sind hoechstens
# zwei Drittel (gemessen am 22.09.2026: 30 von 54, also 56 %).
if [ $((beschrieben_a * 3)) -gt $((gemalt_a * 2)) ]; then
    echo "  FEHLER: beschrieben ist mit dem Aussehen nicht mehr kuerzer"
    echo "  als gemalt ($beschrieben_a von $gemalt_a Zeilen)."
    exit 1
fi
# Zuschnitt B: die Gliederung selbst. Gefordert ist auch hier
# hoechstens die Haelfte (gemessen am 23.09.2026: 16 von 46, also
# 35 %).
if [ $((beschrieben_b * 2)) -gt "$gemalt_b" ]; then
    echo "  FEHLER: die beschriebene Gliederung braucht mehr als die"
    echo "  Haelfte der gemalten ($beschrieben_b von $gemalt_b Zeilen)."
    exit 1
fi
echo "  beschrieben ist in beiden Zuschnitten kuerzer als gemalt   OK"

echo
echo "== 19d. DIESELBE LEISTE, DIESELBE DATEI, ZWEI FASSUNGEN =="
# WARUM ES DIESEN ZWEITEN VERGLEICH GIBT. Abschnitt 19c haelt die
# Werkzeugleiste aus tools/fui/gallery9_main.fi gegen die gemalte aus
# demos/fuidemo/main.fi. Das ist eine ehrliche Messung, aber sie geht
# ueber ZWEI Dateien, und ein Pruefer darf zu Recht fragen, ob da noch
# dasselbe Stueck Oberflaeche verglichen wird.
#
# Hier stehen beide Fassungen in EINER Datei, nebeneinander, und sie
# malen nachweislich dasselbe Bild: `pruefe_leisten` in derselben Datei
# haelt ihre fuenf Rechtecke ganzzahlig gegeneinander, und zwar bei
# 952 UND bei 260 Punkten Breite (dort greift die Klemmung des
# Suchfeldes). Verglichen werden die Marken
#
#   >>> LEISTE BESCHRIEBEN ... <<< LEISTE BESCHRIEBEN   (fn werkzeugleiste)
#   >>> LEISTE GEMALT      ... <<< LEISTE GEMALT        (fn werkzeugleiste_gemalt)
#
# beide in demos/fuidemo/main.fi. Ausserhalb der Marken liegt in BEIDEN
# Faellen nur die Pruefung (das Herausreichen der Rechtecke, die
# Meldung bei unvollstaendigem Baum) -- kein Stueck Oberflaeche.
#
# DREI ZUSCHNITTE, wie in 19c, und die beiden ersten sagen etwas
# Unbequemes: fuer EINE Leiste ist die Beschreibung NICHT kuerzer (51
# gegen 52 Zeilen roh). Das steht hier als Zahl und nicht als Ausrede --
# ein Stilblatt fuer eine einzige Leiste amortisiert sich nicht, und
# genau darum zeigt 19c die Seite mit mehreren Kacheln.
#
# Der dritte Zuschnitt ist der, um den es geht: die GLIEDERUNG. Ohne
# die Beschriftungen und ohne das Aussehen (auf der gemalten Seite
# Painter, Farben und Stile; auf der beschriebenen Seite das Stilblatt
# samt seinen Klassennamen -- das ist dort das Aussehen) bleibt stehen,
# WAS dasteht. Dort spart die Beschreibung die Messung, die Verteilung,
# das Setzen jedes Rechtecks und die eigene Zeichenschleife.
ohne_aussehen_d() {
    grep -v -e 'painter\.round_rect' -e 'painter\.round_ring' \
        -e 'theme\.opaque' -e 'theme\.theme_colors' \
        -e 'render\.ctx_painter' -e 'style\.style_set' \
        -e 'style\.color_token' -e 'let rs:' -e 'sheet\.decl_new' \
        -e 'style\.style_new' -e 'sheet\.decl_set_style' \
        -e 'sheet\.sheet_new' -e 'sheet\.sheet_add' \
        -e 'sheet\.sheet_rule' \
        -e 'sheet\.sheet_name' -e 'var n[a-z]*: \[u8;'
}
sed -n '/>>> LEISTE BESCHRIEBEN/,/<<< LEISTE BESCHRIEBEN/p' \
    demos/fuidemo/main.fi > "$W/leiste_b.txt"
sed -n '/>>> LEISTE GEMALT/,/<<< LEISTE GEMALT/p' \
    demos/fuidemo/main.fi > "$W/leiste_g.txt"
ohne_text < "$W/leiste_b.txt" > "$W/leiste_ba.txt"
ohne_text < "$W/leiste_g.txt" > "$W/leiste_ga.txt"
ohne_aussehen_d < "$W/leiste_ba.txt" > "$W/leiste_bb.txt"
ohne_aussehen_d < "$W/leiste_ga.txt" > "$W/leiste_gb.txt"
lb=$(zaehle_code "$W/leiste_b.txt")
lg=$(zaehle_code "$W/leiste_g.txt")
lba=$(zaehle_code "$W/leiste_ba.txt")
lga=$(zaehle_code "$W/leiste_ga.txt")
lbb=$(zaehle_code "$W/leiste_bb.txt")
lgb=$(zaehle_code "$W/leiste_gb.txt")
echo "  roh: beschrieben $lb Zeilen, gemalt $lg Zeilen"
echo "  A (ohne Texte): beschrieben $lba, gemalt $lga"
echo "  B (nur Gliederung): beschrieben $lbb, gemalt $lgb"
if [ "$lb" -lt 30 ] || [ "$lg" -lt 30 ] || [ "$lbb" -lt 12 ] \
    || [ "$lgb" -lt 20 ]; then
    echo "  FEHLER: eine der beiden Fassungen wurde nicht gefunden oder"
    echo "  die Filter haben zu viel weggenommen. Es fehlen die Marken"
    echo "  LEISTE BESCHRIEBEN / LEISTE GEMALT in demos/fuidemo/main.fi."
    exit 1
fi
# ROH UND A: die Beschreibung darf nicht LAENGER sein. Mehr wird hier
# nicht verlangt, und der Grund steht oben.
# ROH UND A: hoechstens 80 % -- die Texte und das Aussehen zaehlen
# hier auf beiden Seiten voll mit, und eine Beschreibung, die ihre
# fuenf Beschriftungen genauso einzeln hinschreibt wie die gemalte,
# kann in diesen beiden Zuschnitten gar nicht halb so lang werden
# (gemessen am 22.09.2026: 40 von 52 roh = 77 %, 30 von 42 ohne
# Texte = 71 %).
if [ $((lb * 5)) -gt $((lg * 4)) ] || [ $((lba * 5)) -gt $((lga * 4)) ]; then
    echo "  FEHLER: die beschriebene Leiste braucht mehr als vier"
    echo "  Fuenftel der gemalten ($lb von $lg roh, $lba von $lga ohne"
    echo "  Texte)."
    exit 1
fi
# B: die Gliederung. Auch hier gilt wieder die HAELFTE, dieselbe
# Schranke wie in 19c -- und sie haelt nicht, weil die Pruefung
# nachgegeben haette, sondern weil dieselbe Leiste mit scene_box,
# node_set_space, node_set_flexitem und scene_run jetzt kuerzer
# beschrieben ist (gemessen am 22.09.2026: 15 von 32, also 47 %).
if [ $((lbb * 2)) -gt "$lgb" ]; then
    echo "  FEHLER: die beschriebene Gliederung braucht mehr als die"
    echo "  Haelfte der gemalten ($lbb von $lgb Zeilen)."
    exit 1
fi
echo "  dieselbe Leiste beschrieben: Gliederung $lbb von $lgb        OK"

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
    # auf den Inhalt zu) und die geforderte Farbvielfalt. Die obere
    # Grenze ist die Leinwand aus dem malenden Programm -- wer sie dort
    # aendert, traegt die Zahl HIER nach; genau dafuer steht sie da.
    build belegpruef
    # DIE BELEGE, NACHGERECHNET. Ab Abschnitt 4 kommen zwei weitere
    # Zahlen dazu, und beide sind ABSICHTLICH je Blatt gesetzt:
    #
    #   <rand>   wieviel leerer Streifen unter dem letzten Inhalt noch
    #            durchgeht. Die Belege dieses Baumes lassen 20 Punkte
    #            Rand; 40 laesst Luft fuer eine Kantenglaettung und
    #            faengt trotzdem jede Leinwand, die auf eine Konstante
    #            statt auf den Inhalt gesetzt ist (gallery6 hatte so
    #            42 Punkte, siehe dort).
    #   <seiten> 1 heisst: auch links und rechts muss Luft sein. Zwei
    #            Blaetter setzen mit Absicht ueber die volle Breite --
    #            der Verdunkler hinter den Dialogen in fui-wave3 und
    #            die Trennlinie unter der Titelzeile von fui-demo --,
    #            die rufen ohne die 1.
    beleg() {
        "$W/belegpruef" "$@"
    }
    # DIE BESCHRIFTUNGEN GEGEN DIE LEINWAND. Abschnitt 19b rechnet die
    # Kettenlaengen jeder Quelle nach; hier kommt die zweite Haelfte
    # dazu, fuer die man die Leinwandbreite des Blattes braucht: jede
    # gemalte Kette wird mit `render.pref_of` bei 15 Punkt gemessen und
    # muss in die Breite des Belegs minus zweimal 24 Punkt Rand passen.
    # Die Breite ist DIESELBE Zahl, die zwei Zeilen weiter an `beleg`
    # geht -- steht sie einmal falsch, faellt es hier oder dort auf.
    kette() {
        "$W/belegpruef" --ketten "$SCHRIFT" "$2" "tools/fui/$1_main.fi"
    }
    build gallery
    kette gallery 1200
    "$W/gallery" "$Z/fui-wave1-light.png" light
    "$W/gallery" "$Z/fui-wave1-dark.png" dark
    beleg "$Z/fui-wave1-light.png" 1200 400 760 200 40 1
    beleg "$Z/fui-wave1-dark.png" 1200 400 760 200 40 1
    build gallery2
    kette gallery2 1240
    "$W/gallery2" "$Z/fui-wave2-light.png" light
    "$W/gallery2" "$Z/fui-wave2-dark.png" dark
    beleg "$Z/fui-wave2-light.png" 1240 700 1180 200 40 1
    beleg "$Z/fui-wave2-dark.png" 1240 700 1180 200 40 1
    build gallery3
    kette gallery3 1240
    "$W/gallery3" "$Z/fui-wave3-light.png" light
    "$W/gallery3" "$Z/fui-wave3-dark.png" dark
    beleg "$Z/fui-wave3-light.png" 1240 700 1320 200 40 0
    beleg "$Z/fui-wave3-dark.png" 1240 700 1320 200 40 0
    build gallery4
    kette gallery4 1240
    "$W/gallery4" "$Z/fui-text-light.png" light
    "$W/gallery4" "$Z/fui-text-dark.png" dark
    beleg "$Z/fui-text-light.png" 1240 600 1180 200 40 1
    beleg "$Z/fui-text-dark.png" 1240 600 1180 200 40 1
    # BILD UND SVG (Runde BILD+SVG, 13.09.2026). Fuenf Baender: Knopf mit
    # Icon, Beschriftung mit Bild und Icon-Toolbar, dasselbe SVG je Groesse
    # NEU gerastert (12..64) samt currentColor=TOK_ACCENT daneben, Bild mit
    # Transparenz ueber vier Gruenden, Reiter/Menue/Kachel.
    build artshow
    kette artshow 900
    "$W/artshow" "$Z/fui-bild-svg-hell.png" light
    "$W/artshow" "$Z/fui-bild-svg-dunkel.png" dark
    beleg "$Z/fui-bild-svg-hell.png" 900 400 620 200 40 1
    beleg "$Z/fui-bild-svg-dunkel.png" 900 400 620 200 40 1
    # RUNDE UI-WEB: die vier neuen Faehigkeiten, hell und dunkel.
    # Eine Bewegung als Phasenreihe, die Flex-Varianten nebeneinander,
    # Schatten/Glas/Farbmatrix ueber gemustertem Grund, und gedrehte,
    # skalierte, gescherte Kacheln.
    build gallery5
    kette gallery5 1240
    "$W/gallery5" "$Z/fui-anim-hell.png" light
    "$W/gallery5" "$Z/fui-anim-dunkel.png" dark
    beleg "$Z/fui-anim-hell.png" 1240 500 764 200 40 1
    beleg "$Z/fui-anim-dunkel.png" 1240 500 764 200 40 1
    build gallery6
    kette gallery6 1240
    "$W/gallery6" "$Z/fui-flex-hell.png" light
    "$W/gallery6" "$Z/fui-flex-dunkel.png" dark
    beleg "$Z/fui-flex-hell.png" 1240 600 880 200 40 1
    beleg "$Z/fui-flex-dunkel.png" 1240 600 880 200 40 1
    build gallery7
    kette gallery7 1240
    "$W/gallery7" "$Z/fui-effekt-hell.png" light
    "$W/gallery7" "$Z/fui-effekt-dunkel.png" dark
    beleg "$Z/fui-effekt-hell.png" 1240 400 700 200 40 1
    beleg "$Z/fui-effekt-dunkel.png" 1240 400 700 200 40 1
    build gallery8
    kette gallery8 1280
    "$W/gallery8" "$Z/fui-transform-hell.png" light
    "$W/gallery8" "$Z/fui-transform-dunkel.png" dark
    beleg "$Z/fui-transform-hell.png" 1280 500 760 200 40 1
    beleg "$Z/fui-transform-dunkel.png" 1280 500 760 200 40 1
    # DIE BESCHRIEBENE OBERFLAECHE. Der Beleg zu lib/fui/scene.fi,
    # sheet.fi und viewport.fi: eine ganze Seite, die NICHT Aufruf fuer
    # Aufruf gemalt, sondern als Baum beschrieben und von einem
    # Stilblatt eingefaerbt wird -- mit einer Liste aus 28 Eintraegen
    # in einem Scheibenkasten, sichtbar abgeschnitten und mit einem
    # Rollbalken, dessen Laenge aus dem Verhaeltnis Ausschnitt/Inhalt
    # kommt. Das Programm rechnet selbst nach, dass ueber und unter
    # dem Ausschnitt KEIN Punkt der Liste steht (das harte Clipping am
    # fertigen Bild), dass die Rangfolge der Regeln im Bild steht und
    # dass kein Text aus seinem Kasten laeuft; sonst schreibt es kein
    # PNG und der Lauf bleibt daran haengen.
    build gallery9
    kette gallery9 1240
    "$W/gallery9" "$Z/fui-deklarativ-hell.png" light
    "$W/gallery9" "$Z/fui-deklarativ-dunkel.png" dark
    beleg "$Z/fui-deklarativ-hell.png" 1240 700 740 200 40 1
    beleg "$Z/fui-deklarativ-dunkel.png" 1240 700 740 200 40 1
    # UND DIESELBE SEITE SCHMAL. Das ist der eigentliche Beweis der
    # Beschreibung: NICHTS am Baum und nichts am Stilblatt aendert
    # sich, nur die Leinwand ist 980 statt 1240 Punkte breit -- die
    # Karten werden schmaler, die Werkzeugleiste verteilt neu, und die
    # Seite rechnet ihre eigenen Zusagen noch einmal nach (kein Text
    # laeuft aus seinem Kasten, kein Punkt aus dem Ausschnitt). Eine
    # gemalte Fassung muesste dafuer jede Koordinate anfassen.
    kette gallery9 980
    "$W/gallery9" "$Z/fui-deklarativ-schmal-hell.png" light - 980
    "$W/gallery9" "$Z/fui-deklarativ-schmal-dunkel.png" dark - 980
    beleg "$Z/fui-deklarativ-schmal-hell.png" 980 700 740 200 40 1
    beleg "$Z/fui-deklarativ-schmal-dunkel.png" 980 700 740 200 40 1
    # DIE UEBERSICHT AUS DER ERSTEN STUNDE. tools/fui/preview_main.fi
    # malt die Grundelemente in allen Zustaenden; sie lag seit ihrer
    # Entstehung NEBEN diesem Lauf -- gebaut hat sie niemand, gerechnet
    # erst recht nicht. Jetzt entsteht ihr Bild hier und wird
    # nachgerechnet wie jeder andere Beleg.
    build preview
    kette preview 760
    "$W/preview" "$Z/fui-preview-hell.png" light
    "$W/preview" "$Z/fui-preview-dunkel.png" dark
    beleg "$Z/fui-preview-hell.png" 760 200 240 120 40 1
    beleg "$Z/fui-preview-dunkel.png" 760 200 240 120 40 1
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
    "$W/belegpruef" --ketten "$SCHRIFT" 1000 demos/fuidemo/main.fi
    "$W/fuidemo" "$Z/fui-demo-hell.png" light
    "$W/fuidemo" "$Z/fui-demo-dunkel.png" dark
    beleg "$Z/fui-demo-hell.png" 1000 350 500 200 40 0
    beleg "$Z/fui-demo-dunkel.png" 1000 350 500 200 40 0
    ls -la "$Z"
fi

echo
echo "ALL CHECKS PASSED."
