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
# THE FREESTANDING TARGET, AND WHAT HAPPENS IF IT IS MISSING. Formerly there stood
# here: if "x86_64-none" does not exist, then just without --target. That made
# section 1 SILENTLY green on every compiler state without a freestanding target,
# although its heading claimed something else -- exactly
# the sort of omitted check against which this run is written.
# Now the substitute check is NAMED and itself checked
# (see section 1); if it fails, the run aborts.
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
# IF THE FREESTANDING TARGET IS MISSING. This compiler state knows only
# x86_64-linux and aarch64-linux; a target "x86_64-none" does not exist (yet).
# Leaving out --target must not mean, however, that nothing of
# the heading is checked any more. So in its
# place it is proven that the profile `kernel` has TEETH: it must
# REJECT `syscall` and the import of std.io. A profile that lets both
# through would be a label, and then "0 syscall
# instructions" further below would be a coincidence of the source text. If
# this substitute proof fails, the run has NOT passed -- it does not
# silently continue any more.
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
echo "== 2b. THE THEME FILE (lib/fui/themefile.fi) =="
# A theme from a text file is applied only if it keeps the same
# promise: the cases, then every theme shipped with fUi.
build themefile
"$W/themefile"
"$W/themefile" --builtin

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
# Justin's finding of 10.09.2026: the focus ring was 6 rows thick at the top
# and 2 at the bottom, and stood two points too high at the top right. The cause was
# the inner path in painter.path_round_ccw (swapped control points,
# one segment ended on its own start). This check COUNTS
# the four edge thicknesses -- the same sort of asymmetry cost us two
# rounds with the close cross.
build symmetry
"$W/symmetry"

echo
echo "== 7. THE ICONS AT THE PIXEL =="
build icon
"$W/icon"

echo
echo "== 7b. THE LUCIDE ICONS AND THE GRADIENT AT THE PIXEL =="
# lib/fui/icons.fi draws lib/fui/lucide.fi (ISC, LICENSES/Lucide-ISC.txt)
# as stroked paths: every icon puts ink down, the stroke is 2 units, the
# colour arrives per channel, the outline is rastered at the drawn size,
# the canvas clip holds, and painter.round_rect_grad ramps as asked.
build icons
"$W/icons"

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
# lib/svg comes from the Certus tree (13.09.2026, seven of eight files
# byte-identical). What is checked is what was touched in the port:
# raster_finish_evenodd in the NEWER raster.fi, ttf.font_outline_m, and
# the font adapter svg/fontsel.fi. In addition currentColor against the
# accent colour -- Justin's addition of 13.09.2026.
build uisvg
"$W/uisvg"

echo
echo "== 10d. THE PICTURE ON THE WIDGET =="
# Icon left/right/only, the measure, the tint, and the tab and menu entry
# that paint through the borrowed label.
build picture
"$W/picture"

echo
echo "== 11. THE TEXT VALUES AT THE PIXEL =="
# Tracking, line height, kerning, outline, shadow, gradient. The three
# tracking numbers Justin asked to see are printed by this one.
build text
"$W/text"

echo
echo "== 12. DIE ZEIT: EASINGS, TWEENS, UEBERGAENGE =="
# Round UI-WEB: lib/fui/anim.fi. What is checked are the easings at their
# support points (ease-in-out is exactly 0.5 at 0.5), the analytically
# solved spring (it comes to rest and does not drift), the clock, which
# reports "redraw" only on a real change, and the
# colour mixing with correct alpha.
# Section 12b reads the numbers FROM THE RENDER PATH: render.Ctx has carried
# a register of running transitions since the round FEHLERBEHEBUNG
# (anim.TransReg), and the pixel in the middle of the painted button
# goes over three images from 0x42414D to 0x52525E. Without the register
# it jumps at once -- the counter-test stands beside it.
build anim
"$W/anim"

echo
echo "== 13. FLEX-LAYOUT AM PIXEL =="
# Every number here is calculated by hand: distribution of the free and of the
# missing space, clamping to min/max and the REDISTRIBUTION that
# follows from it -- the place where a half-finished flexbox leaves a gap
# at the right edge.
build flex
"$W/flex"

echo
echo "== 14. UNSCHAERFE, SCHATTEN, FARBMATRIX =="
# The box blur against the convolution by hand and three times
# box against the cubic B-spline (1,3,6,7,6,3,1)/27 -- the
# analytic answer that a Gauss approximation MUST give.
build effect
"$W/effect"
# THE SECOND HALF OF THE SAME CHECK. There are two box blurs in this tree --
# lib/paint/painter.blur_line on
# COVERAGE VALUES (f64) and lib/fui/effect.blur_buffer on the
# PREMULTIPLIED u8 CANVAS. That this is not a second place for
# the same thing is stated in the head of effect.fi; that both at
# the same sigma give the same tone value is re-calculated by the two
# programs against the SAME table. There are two because Firn resolves a
# module after the last path segment and `fui.painter` and
# `paint.painter` therefore do not fit into one program.
build blurref
"$W/blurref"

echo
echo "== 15. AFFINE ABBILDUNGEN UND DIE TREFFERPRUEFUNG =="
# Known point images, the stack, the inverse mapping -- and the
# click under rotation, which without the inverse mapping goes wide.
# Section 16 of this file goes the NORMAL way: a button rotated by 24
# degrees, painted with wave2.draw_any_xf (that is via
# render.draw_widget_xf), operated with control.mouse_down -- and the
# point at which it was painted is the same one at which it is
# hit. Without transform.tf_bind_panel exactly this click goes wide,
# and that too stands there as a number.
build transform
"$W/transform"

echo
echo "== 16. DIE BEDIENUNG: FOKUSKETTE UND ZEIGER =="
# tools/fui/control_main.fi lay BESIDE the check run since its creation:
# it re-calculates, but nobody called it. That is exactly how a
# check gets silently lost -- the reason why this file
# exists. So now it hangs in here. Important for the round UI-WEB:
# here lies the hit test onto which lib/fui/transform.fi calculates the point
# back with the inverse mapping.
build control
"$W/control"

echo
echo "== 17. DER ZEILENUMBRUCH =="
# Likewise added: the wrapping checked itself without the
# run knowing about it.
build wrap
"$W/wrap"

echo
echo "== 18. DIE BEDIENUNG DES TEXTFELDES =="
# lib/fui/editor.fi could NOT be compiled since its first day
# (a missing `if` line in the branch for backspace), and it did not
# show because no program included the file and this run did not
# know it. Now it is built AND calculated: Ctrl+A replaces
# instead of appending (Justin's address-bar error), the word jumps,
# Ctrl+Backspace/Del with and without selection, Ctrl+Z/Y and the keys that
# do not belong to the field.
build editor
"$W/editor"

echo
echo "== 18a. DIE EINGABEMETHODE: CHINESISCH, JAPANISCH, KOREANISCH =="
# lib/fui/ime.fi with its connection to editor.fi. Re-calculated
# are romaji to kana (double consonant, n rule, tch), the
# Hangul syllables by Unicode chapter 3.12 (composing AND decomposing,
# every number stands calculated beside it), the migrating final, the
# pre-edit text with the write pointer before and behind it, the
# selection that is replaced only on confirming and comes back on cancelling, the candidates, pinyin against the supplied table
# and the platform interface via an imitated platform.
# The image for it (field with open candidate list) arises below
# at --images with tools/fui/imebeleg_main.fi.
build ime
"$W/ime"

echo
echo "== 18b. DER SCHEIBENKASTEN =="
# lib/fui/viewport.fi: the section that takes in more content than it is
# high. Re-calculated are the cross calculation (a
# vertical bar makes a horizontal one necessary), the visible
# section, the clamping, share and position of the scroll bar from the
# ratio section/content, `viewport_ensure_visible`, the
# hit test UNDER OFFSET and the kinetic coasting via
# lib/fui/anim.fi. The hard clipping is measured on the canvas:
# no pixel outside the section may be set, and the counter-test WITHOUT clip
# must report the same test red.
build viewport
"$W/viewport"

echo
echo "== 18c. DAS STILBLATT: RANGFOLGE UND VERERBUNG =="
# lib/fui/sheet.fi. The cases contradict each other on purpose:
# id beats class beats kind, on a tie the rule written
# later wins, and a high-ranking rule that names only the
# font colour deletes no background. In addition the inheritance AND
# its limit -- five values pass over (since 23.09.2026 also the
# writing direction), the background does not.
build sheet
"$W/sheet"

echo
echo "== 18d. DER BESCHRIEBENE BAUM =="
# lib/fui/scene.fi: tree, style, measuring, arranging, painting -- in
# this order and in separate passes. Checked is, among other things,
# that no size drifts any more while painting
# (`scene_size_drift` is 0) and that the hit test delivers the topmost
# node.
build scene
"$W/scene"

echo
echo "== 18e. DIE BARRIEREFREIHEIT: ROLLE, NAME, ZUSTAND, FOKUS =="
# lib/fui/a11y.fi: each of the 23 kinds gets its role, the name
# comes from the right source (explicit > own label >
# label by id > children), a button without a name is COUNTED,
# Tab runs in tree order past disabled elements, the
# focus rolls an entry of a long list into the section (the
# offsets from viewport.fi re-calculated by hand), and the
# printed tree of a known surface matches line by line.
# The proof on the real example is carried on by tools/fui/gallery9_main.fi
# (section 10 there) in the gallery further below.
build a11y
"$W/a11y"
echo "== 18g. DIE ZWEIRICHTUNGSSCHRIFT: DATEN UND ALGORITHMUS =="
# Arabic and Hebrew stand from right to left, numbers and
# Latin words in them from left to right. Which order is
# right is told by the Unicode bidi algorithm (UAX #9), and that
# stands in lib/fui/bidi.fi. Two things are re-calculated here:
#
#   1. the bidi table (lib/generated/bidi_tables.fi) arises from the
#      UCD 17.0.0 octet for octet identically, and a second parser
#      (tools/ucd/verify_bidi.py) holds it over all 1,114,112
#      code points against the files -- including a counter-test with a
#      forged row;
#   2. lib/fui/bidi.fi against BidiCharacterTest.txt, the test cases
#      of the Unicode Consortium: 91,707 paragraphs with expected paragraph level,
#      level per character and order on the screen. Not a single one
#      may deviate.
BIDI_WORK="$W/ucd-bidi" bash tools/ucd/build_bidi.sh --verify > "$W/bidi_build.log" 2>&1 || {
    cat "$W/bidi_build.log"
    echo "  die Bidi-Tabelle ist nicht, was die UCD sagt -- NICHT bestanden"
    exit 1
}
grep -E "gleich|Gegenprobe|VERSCHIEDEN|Oktette zur|Laufzeit" "$W/bidi_build.log" | sed 's/^ */  /'
build bidiconf
gzip -dc tools/ucd/BidiCharacterTest.txt.gz | "$W/bidiconf"
# AND BY HAND: tools/fui/bidi_main.fi holds the same rules against
# target values that a human determined with a pencil -- 30 paragraphs
# (pure RTL, mixed, numbers in Arabic text, mirrored
# brackets including the three N0 examples from UAX #9, isolates), the
# line break in mixed text, the widths that the painter measures
# (from the hmtx table of DejaVu), the caret
# and the arrow keys in the text field, and the Arabic shapes via GSUB and
# via the approximation.
build bidi
"$W/bidi"
echo "== 18h. DIE PLATTFORMSCHICHT OHNE BILDSCHIRM (X11 + WIRT) =="
# lib/window/x11.fi speaks the X11 protocol itself (no Xlib, no
# xcb), lib/plat/fuiwirt.fi hands the events to lib/fui/control.fi.
# Checked is WITHOUT an X server: every request octet for octet against
# values determined by hand, every reply against real captures of an
# Xvfb (testdata/x11/, tools/fui/x11_capture.py) and against xdpyinfo/
# xmodmap, in addition the gallery9 page at scale 1000/1500/2000, the
# operation via the host and the idle state (0 images without change).
# `import window.backend` finds the X11 backend via the link
# tools/fui/window/backend.fi.
build x11
"$W/x11"

echo
echo "== 18i. EIN ECHTES FENSTER AUF EINEM ECHTEN X-SERVER =="
# The same page as a program (demos/x11demo) on its own Xvfb,
# operated from outside: xdotool for mouse and keyboard, a raw
# ClientMessage WM_DELETE_WINDOW for closing, xwd for the image FROM THE
# SERVER, /proc for the CPU time at idle. Built with the
# optimiser, because the run waits for images (a full image needs more than a second with
# --opt-level=dev). If Xvfb is missing, the run says SKIP
# and why -- a missing X server is no error of the demo.
"$FIRNC" --opt-level=release-fast -o "$W/x11demo" demos/x11demo/main.fi
if command -v Xvfb >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
    python3 tools/fui/x11live.py "$W/x11demo" "${BELEGE:-$W/belege}/x11"
else
    echo "  SKIP: kein Xvfb/python3 -- kein echtes Fenster in diesem Lauf"
fi

echo
echo "== 18j. fUi TEMPO: DER DECKUNGS-CACHE UND DER CLIP AENDERN KEIN OKTETT =="
# lib/fui/fcache.fi (rasterised text and icons are kept) and
# painter.clip_rows (shapes outside the clip are not rasterised):
# the gallery page without cache, with cache cold and warm, and a strip
# repainted under a clip -- every time the same octets.
"$FIRNC" --opt-level=release-fast -o "$W/tempo" tools/fui/tempo_main.fi
"$W/tempo" 1240 5

echo
echo "== 18k. SCHMELZGRUPPE: FORMEN VERSCHMELZEN, WENN SIE SICH NAEHERN =="
# lib/fui/merge.fi (prototype r68): far apart the same octets as
# the plain union, at k/2 the gap is ink, the tiles
# change no octet, no jump on moving closer, a spring, the
# time -- four rules measured side by side.
"$FIRNC" --opt-level=release-fast -o "$W/merge" tools/fui/merge_main.fi
"$W/merge"

echo
echo "== 18l. DER SEITEN-LOOK: VERLAUF, LICHT, SCHATTEN, FLUIDE SCHRIFT, ABSAETZE, GRUPPEN =="
# lib/fui/style.fi (SF_BG_GRAD, SF_BG_GLOW, SF_BOX_SHADOW, SF_FLUID),
# lib/fui/scene.fi (node_set_text_wrap, node_set_opacity/offset,
# scene_draw_still/live) and lib/fui/layer.fi -- on real pixels.
"$FIRNC" --opt-level=release-fast -o "$W/pagelook" tools/fui/pagelook_main.fi
"$W/pagelook"

echo
echo "== 18m. DER APP-BAUM: SZENE ALS DOM, EINMAL KURZ DURCHGEMESSEN =="
# tools/fui/apptree_main.fi (docs/APP-TREE.md): gallery9 and artificial
# trees up to 128 nodes -- build, style, measure, lay out, paint, hit,
# query, read aloud. Only ONE round here: it must run and every node
# needs a name (unnamed=0); the numbers are measured by tools/fui/apptree.sh.
"$FIRNC" --opt-level=release-fast -o "$W/apptree" tools/fui/apptree_main.fi
"$W/apptree" 128 1 > "$W/apptree.log" || { cat "$W/apptree.log"; echo "  FAIL: apptree"; exit 1; }
n=$(grep -c '^apptree tree=' "$W/apptree.log" || true)
bad=$(grep '^apptree tree=' "$W/apptree.log" | grep -vc ' unnamed=0 ' || true)
[ "$n" -ge 2 ] && [ "$bad" = 0 ] || { cat "$W/apptree.log"; echo "  FAIL: apptree ($n Baeume, $bad mit Knoten ohne Namen)"; exit 1; }
echo "  APP-BAUM OK: $n Baeume, jeder Knoten hat einen Namen"
echo
echo "== 18n. THE APP TREE: SECRETS, KEYS, QUERIES, CULLING, MEMO, INSPECTOR =="
# tools/fui/dom_main.fi (docs/APP-TREE.md 7): a password never leaves the
# process (canary, not even its length), key paths survive a rebuild,
# queries by selector / role + name / text, culling paints the same
# octets, the measure memo gives the same sizes, the inspector picks,
# describes (without secrets) and edits live.
build dom
"$W/dom"

echo
echo "== 18o. THE APP TREE: EVENTS, POINTERS, GESTURES, CHANGE RECORDS =="
# tools/fui/event_main.fi (docs/APP-TREE.md T3, T4, T6): capture -> target
# -> bubble with stop / stop-now / prevent-default and control.fi as the
# default action; pointer ids, primary, pointer capture (also across a
# rebuild); tap, double tap, long press, pan + fling, pinch + rotate with an
# arena that lets exactly one win and cancels the loser's press; change
# records by key -- none for a renumbering rebuild, none for a secret.
# Everything with synthetic event streams.
build event
"$W/event"

echo
echo "== 18o2. REAL POINTERS FROM THE WINDOW LAYER (r111) =="
# tools/fui/pointers_main.fi: synthetic Android MotionEvent streams through
# lib/window/pointers.fi -- one record per finger, ids that outlive the
# index, primary per type, lost UPs cancelled, the ring. (The same stream in
# a real browser: tools/wasm/touchcheck.py; on an emulator with real
# fingers: tools/android/pointers_check.sh.)
build pointers
"$W/pointers"

echo
echo "== 18o2b. TOUCH FROM X11 AND WIN32 (r112) =="
# XInput2 touch (lib/window/x11.fi) and WM_POINTER (lib/window/win32.fi) feed
# the same records as Android (lib/window/pointers.fi, `pointers_single`).
# The byte layouts are checked by hand in x11_main (section 12) and
# pointers_main (sections 8, 9); the handshake runs against a real Xvfb
# (xinput_main: XInput 2.2 present, XIGetSelectedEvents shows the touch
# selection). No finger is played in -- the build machine has no uinput.
# win32.fi must build for the Windows target.
build xinput
XINPUT_BIN="$W/xinput" FIRNC="$FIRNC" W="$W" bash tools/fui/xinput.sh
rm -rf "$W/wn"; mkdir -p "$W/wn/window"
cp demos/x11demo/main.fi demos/x11demo/gallery9_main.fi "$W/wn/"
ln -s "$PWD/lib/window/win32.fi" "$W/wn/window/backend.fi"
"$FIRNC" --target=x86_64-windows -o "$W/wn/w.exe" "$W/wn/main.fi" \
    && echo "  OK      the window layer builds for Windows (win32.fi)" \
    || { echo "  FAILED  win32.fi does not build"; exit 1; }

echo
echo "== 18o2c. THE ON-SCREEN KEYBOARD'S CLASSES (r114) =="
# lib/android/inputmethod.fi makes an InputConnection out of two classes it
# writes as dex at run time (lib/android/imedex.fi); the SDK's own dexdump
# reads the bytes and the script looks at the result. (On an emulator:
# tools/android/keyboard_check.sh types on the real keyboard,
# tools/android/lifecycle_check.sh checks pause, rotation and screen off.)
FIRNC="$FIRNC" W="$W" bash tools/android/ime_dex_check.sh
echo
echo "== 18o3. A TRANSITION SURVIVES A REBUILD (r110) =="
# tools/fui/animkey_main.fi: the transition registry keyed by key path --
# the button half way to its hover colour keeps its colour when the tree is
# rebuilt (A and B swap, a node is inserted in front); without the keyer the
# same rebuild makes it jump (counter-check).
build animkey
"$W/animkey"

echo
echo "== 18p. fui.app: A WINDOW IN TEN LINES, NATIVE AND IN THE BROWSER =="
# lib/fui/app.fi (29.09.2026): the three programs of examples/fui/ build
# for BOTH platforms from one source -- `import fui.apphost` resolves to
# lib/@linux/ natively and to lib/@web/ with --target=wasm32-browser --
# and tools/fui/app_main.fi drives them without a window (layout, clicks,
# keys, text fields, wrapping, scale 2). The browser half with Chromium:
# bash tools/wasm/appdemo.sh.
for ex in hello_window counter form touchpad; do
    "$FIRNC" --opt-level=dev -o "$W/app_$ex" "examples/fui/$ex.fi"
    "$FIRNC" --opt-level=dev --target=wasm32-browser -o "$W/app_$ex.wasm" \
        "examples/fui/$ex.fi"
done
echo "  examples/fui/{hello_window,counter,form,touchpad}.fi build native + wasm32   OK"
build app
"$W/app"

echo
echo "== 18p2. THE ACCESSIBILITY AUDIT OF EVERY fUi PROGRAM (r98) =="
# tools/fui/audit_main.fi shows that lib/fui/audit.fi has teeth (an unnamed
# button, a duplicate key, a secret in the export are each found);
# tools/fui/audit.sh then runs every program of examples/fui/*.fi through it
# (FUI_AUDIT=1: no window, the tree it built, exit code = the result) and
# requires that a program with a nameless text field FAILS. The programs with
# a main of their own run the same audit in their own checks: gallery9_main
# (section 10) and examples/codehub/main.fi (the "audit:" line).
build audit
"$W/audit"
W="$W" sh tools/fui/audit.sh

echo
echo "== 19. JEDE PRUEFDATEI BAUT, UND JEDE KOMMT IM LAUF VOR =="
# THE ERROR THAT THIS SECTION MAKES IMPOSSIBLE. lib/fui/editor.fi
# could not be compiled for a month, tools/fui/control_main.fi
# and wrap_main.fi calculated for nobody, and tools/fui/preview_main.fi
# stood entirely outside: none of these files appeared in this run,
# so nothing was noticed. A check that nobody calls is none.
#
# Therefore two things here, MECHANICALLY and not from memory:
#   1. EVERY file tools/fui/*_main.fi and demos/*/main.fi is
#      compiled -- also the demos outside fUi, because what
#      nobody builds stops building sooner or later.
#   2. EVERY file tools/fui/*_main.fi and demos/fuidemo/main.fi must
#      APPEAR in this script. (The other demos belong to other
#      trees; they are built, but not calculated here.)
#   An Android-only program (it imports plat.android, which links against
#   the NDK: pthread, ANativeActivity) cannot be linked on this host. It
#   is still built -- as an object for x86_64-android, the first half of
#   tools/android/build.sh -- so it cannot silently stop building either.
for f in tools/fui/*_main.fi demos/*/main.fi examples/*/main.fi; do
    if grep -q '^import plat\.android' "$f"; then
        "$FIRNC" --opt-level=dev --target=x86_64-android --pic -c -o "$W/baupruefung.o" "$f" >/dev/null
    elif grep -q '^import web\.dom' "$f"; then
        # A browser-only program (lib/web/dom.fi imports its functions
        # from the page's JavaScript): it is built for wasm32-browser.
        "$FIRNC" --opt-level=dev --target=wasm32-browser -o "$W/baupruefung.wasm" "$f" >/dev/null
    else
        "$FIRNC" --opt-level=dev -o "$W/baupruefung" "$f" >/dev/null
    fi
done
echo "  alle tools/fui/*_main.fi, demos/*/main.fi und examples/*/main.fi uebersetzen  OK"
fehlt=0
for f in tools/fui/*_main.fi demos/fuidemo/main.fi; do
    n=$(basename "$f" _main.fi)
    case "$f" in
        demos/*) n="fuidemo" ;;
    esac
    # WHAT IS SEARCHED FOR IS THE CALL, NOT THE NAME. Formerly there stood here
    # `grep -q "$n"`, and that was a guard that cheated itself:
    # "anim" is not in "gallery5", but in every comment about
    # anim.fi, "image" in "--images", "text" in "context". A program
    # counted as called as soon as its name appeared ANYWHERE in this file
    # -- so also exactly when nobody calls it. What is searched for
    # is therefore the string "$W/<name>", with which this script executes a
    # built program, and as a fixed text (-F), so that
    # no special character makes it a pattern.
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

# ONE PLACE FOR floor/ceil/abs, AND NOT TWO. `std.math` and
# lib/svg/matrix.fi can both round down. As long as both were
# used in lib/fui/, two modules calculated the same raster edge with two
# different roundings, and the edge was off by one pixel
# -- the most expensive error of this tree is the second place for
# the same thing. It is laid down in the head of lib/fui/anim.fi:
# in lib/fui/ only matrix.m_floor/m_ceil/m_abs apply. Here
# the respective other name is forbidden mechanically.
#
# Comment lines are exempt: the head of anim.fi MUST be allowed to name the
# forbidden name in order to forbid it.
verboten=$(grep -n "math\.\(floor\|ceil\|abs\|fabs\)" lib/fui/*.fi \
    | grep -v "^[^:]*:[0-9]*: *//" || true)
if [ -n "$verboten" ]; then
    echo "  in lib/fui/ steht math.floor/ceil/abs -- verboten. Es gilt"
    echo "  matrix.m_floor/m_ceil/m_abs (siehe Kopf von lib/fui/anim.fi):"
    echo "$verboten"
    exit 1
fi
echo "  floor/ceil/abs kommen in lib/fui/ nur aus svg.matrix        OK"

# NO NUMBER AS A STRIDE OVER A STRUCT. In
# demos/fuidemo/main.fi `(i * 32)` stood three times, to reach into an array of
# `layout.Rect`. That is right today and wrong tomorrow:
# if a field is added to `Rect`, no compiler says anything, and from then on
# it reads into the middle of a rectangle -- the error shows up as a
# shifted image, not as a message. The stride is therefore
# MEASURED (layout.rect_stride, sheet.sheet_desc_stride, both
# by the same pattern: difference of two neighbours of a real
# field), and here the number is forbidden mechanically.
#
# What is searched for are lines that multiply a number by an index AND in the
# same breath reinterpret it as a STRUCT POINTER (`as *mut module.Type`).
# The strides of the basic types (4 for u32, 8 for i64
# and f64) stay allowed: they do not change when somebody adds a
# field. Comment lines are exempt, because this text
# here may name the forbidden form.
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

# EVERY IMPORT IS ALSO CALLED. In lib/fui/scene.fi `import
# std.rt` stood, although in the whole file not a single `rt.` occurred -- and
# that is exactly how a dependency gets into a module that is to be
# built freestanding: not by a call, but by a line that
# nobody reads any more. That can be excluded mechanically
# and is therefore excluded mechanically: for EVERY `import x.y` in
# lib/fui/*.fi, `y.` must stand before a letter in the SAME module
# (lower or upper case -- `rt.buf_new` just like `rt.Buf`).
#
# What is searched are the lines that are NO comment and NO import line:
# the head of a module may write about `viewport.fi` without
# justifying an import by it, and `import fui.style` itself is
# no use of `style.`.
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
# THE ERROR THAT WAS CALLED "red fixe". In tools/fui/gallery_main.fi there stood
# `var t_v3: [u8; 10] = "red fixed "`, eight bytes were painted, and in the
# proof a checker read "red fixe". Beside it three tiles of
# the same file painted the first two bytes of "12 15 20", so three times
# "12", although the font was 12, 15 and 20 points large. Both images
# passed every existing check: the dimensions were right,
# the colour variety was right, every band was painted -- only the WORD
# was broken. A proof with half a word proves the opposite of what
# it claims.
#
# tools/fui/belegpruef_main.fi now re-calculates that mechanically
# (mode --ketten): for every painted chain `(&name[0]) as u64, M`
# the declaration `var name: [u8; N] = "..."` is searched for and
# N - padding <= M <= N is demanded. Here the length calculation runs over
# EVERY source of the tree -- without font ("-") and without width (0), so that
# it also runs on a machine without DejaVu. The width calculation with
# `render.pref_of` stands further below with the proof images, where the
# canvas width of each sheet is known.
build belegpruef
"$W/belegpruef" --ketten - 0 tools/fui/*_main.fi demos/*/main.fi

echo
echo "== 19c. BESCHREIBEN IST KUERZER ALS MALEN, IN ZAHLEN =="
# THE NUMBER THAT JUSTIFIES THE WHOLE EFFORT -- AND HONESTLY
# TAILORED. lib/fui/scene.fi and sheet.fi are worth something only if
# THE SAME piece of surface is shorter when described than when painted.
# So the comparison is not file against file (that would be apples against
# pears, tools/fui/gallery9_main.fi shows more), but ONE piece
# that exists twice: the toolbar -- three buttons, a search field
# with grow, a button in the accent.
#
#   painted     demos/fuidemo/main.fi, fn toolbar_painted.
#   described   tools/fui/gallery9_main.fi, between the marks
#               ">>> TOOLBAR" and "<<< TOOLBAR" (the tree)
#               AND between ">>> BAR RULES" and "<<<
#               BAR RULES" (its look in the style sheet).
#
# WHAT IS NEW IN THIS TAILORING, AND WHY. Until 21.09.2026 there stood
# on the painted side still the two helpers `setze` and
# `item_von_widget` with the justification that they existed "only for this" -- that
# was wrong: `titelzeile` and `dialog` both call them, so they are
# SHARED lines, and whoever assigns shared lines to only one side
# calculates the saving prettier than it is. They are out. In return the
# described page now counts ITS LOOK too: the rule for the
# class `leiste`, the radius of the buttons and the accent -- on the
# painted side exactly that stands in the middle of the function. That two
# of these rules also colour the header row is fully charged
# to the described page.
#
# Counted are lines with code: without blank lines, without comments. And
# counted in THREE cuts, because a single number necessarily
# conceals something here:
#
#   raw   everything that stands in the marks or in the function.
#   A     without the labels (text fields and the setting of the
#         text). On the described side they stand in
#         `schreibe_texte`, so outside the marks -- they are
#         therefore deducted on BOTH sides and not one-sidedly.
#   B     in addition without the look (surface, border, colour, radius)
#         on both sides. What remains is the pure structure,
#         and exactly there the description is shorter by a multiple:
#         the distribution, the setting of each rectangle and the own
#         drawing loop drop out entirely.
#
# The limits below are measured with today's state and set with
# leeway; if one breaks, either a mark has slipped
# or the statement in the head of gallery9_main.fi is no longer true -- and
# a number that is no longer true is worse than none.
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
awk '/^fn toolbar_painted\(/{p=1} p{print} p&&/^}$/{exit}' \
    demos/fuidemo/main.fi > "$W/gemalt.txt"
sed -n '/>>> TOOLBAR/,/<<< TOOLBAR/p' \
    tools/fui/gallery9_main.fi > "$W/beschrieben.txt"
sed -n '/>>> BAR RULES/,/<<< BAR RULES/p' \
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
    echo "  Es fehlen die Marken TOOLBAR/BAR RULES in"
    echo "  tools/fui/gallery9_main.fi oder fn toolbar_painted in"
    echo "  demos/fuidemo/main.fi."
    exit 1
fi
if [ "$gemalt_b" -lt 30 ] || [ "$beschrieben_b" -lt 15 ]; then
    echo "  FEHLER: die Filter ohne_text/ohne_aussehen haben zu viel"
    echo "  weggenommen -- der Zuschnitt B ist damit keine Messung mehr."
    exit 1
fi
# THE OLD BARRIER, BACK AGAIN, ON THE RAW VALUE. Until
# 21.09.2026 it said here "the described version falls to at most
# HALF"; on 22.09.2026 it was replaced by 90 % (A) and 66 % (B)
# because the raw value was then 42 of 64. A check that is
# made softer so that one's own state passes it is no longer a
# check. So it stands there again, and instead the
# DESCRIPTION has become shorter: lib/fui/scene.fi has with scene_box,
# scene_widget, node_set_space, node_set_flexitem and scene_run received
# the short forms that a tree really needs, and
# lib/fui/sheet.fi with sheet_rule the rule from a style.
#
# Measured on 23.09.2026: 30 of 64 lines, so 47 %. Added
# on that day was the SIZE as a style value (style.SF_WIDTH /
# SF_HEIGHT): bar height and row height now stand in the rule
# and no longer as a number at every node in the tree.
if [ $((beschrieben * 2)) -gt "$gemalt" ]; then
    echo "  FEHLER: die beschriebene Fassung faellt nicht auf hoechstens"
    echo "  die Haelfte der gemalten ($beschrieben von $gemalt Zeilen)."
    exit 1
fi
# Cut A: with the look on both sides the
# description stays shorter, but not half as long -- a style sheet
# writes colour and radius ONCE for the whole page, in this
# comparison that counts against it nevertheless. Demanded are at most
# two thirds (measured on 22.09.2026: 30 of 54, so 56 %).
if [ $((beschrieben_a * 3)) -gt $((gemalt_a * 2)) ]; then
    echo "  FEHLER: beschrieben ist mit dem Aussehen nicht mehr kuerzer"
    echo "  als gemalt ($beschrieben_a von $gemalt_a Zeilen)."
    exit 1
fi
# Cut B: the structure itself. Demanded here too is at most
# half (measured on 23.09.2026: 16 of 46, so
# 35 %).
if [ $((beschrieben_b * 2)) -gt "$gemalt_b" ]; then
    echo "  FEHLER: die beschriebene Gliederung braucht mehr als die"
    echo "  Haelfte der gemalten ($beschrieben_b von $gemalt_b Zeilen)."
    exit 1
fi
echo "  beschrieben ist in beiden Zuschnitten kuerzer als gemalt   OK"

echo
echo "== 19d. DIESELBE LEISTE, DIESELBE DATEI, ZWEI FASSUNGEN =="
# WHY THIS SECOND COMPARISON EXISTS. Section 19c holds the
# toolbar from tools/fui/gallery9_main.fi against the painted one from
# demos/fuidemo/main.fi. That is an honest measurement, but it goes
# over TWO files, and a checker may rightly ask whether the
# same piece of surface is still being compared.
#
# Here both versions stand in ONE file, side by side, and they demonstrably
# paint the same image: `pruefe_leisten` in the same file
# holds their five rectangles against each other as integers, and at
# 952 AND at 260 points of width (there the clamping of the
# search field takes effect). What is compared are the marks
#
#   >>> TOOLBAR DESCRIBED ... <<< TOOLBAR DESCRIBED   (fn werkzeugleiste)
#   >>> TOOLBAR PAINTED   ... <<< TOOLBAR PAINTED     (fn toolbar_painted)
#
# both in demos/fuidemo/main.fi. Outside the marks lies in BOTH
# cases only the check (handing out the rectangles, the
# message on an incomplete tree) -- no piece of surface.
#
# THREE CUTS, as in 19c, and the first two say something
# uncomfortable: for ONE bar the description is NOT shorter (51
# against 52 lines raw). That stands here as a number and not as an excuse --
# a style sheet for a single bar does not pay off, and
# that is exactly why 19c shows the page with several tiles.
#
# The third cut is the one that matters: the STRUCTURE. Without
# the labels and without the look (on the painted side
# painter, colours and styles; on the described side the style sheet
# including its class names -- that is the look there) what remains
# is WHAT stands there. There the description saves the measuring, the distribution,
# the setting of each rectangle and the own drawing loop.
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
sed -n '/>>> TOOLBAR DESCRIBED/,/<<< TOOLBAR DESCRIBED/p' \
    demos/fuidemo/main.fi > "$W/leiste_b.txt"
sed -n '/>>> TOOLBAR PAINTED/,/<<< TOOLBAR PAINTED/p' \
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
    echo "  TOOLBAR DESCRIBED / TOOLBAR PAINTED in demos/fuidemo/main.fi."
    exit 1
fi
# RAW AND A: the description must not be LONGER. More is not
# demanded here, and the reason stands above.
# RAW AND A: at most 80 % -- the texts and the look count
# fully on both sides here, and a description that writes down its
# five labels one by one just like the painted one
# cannot become half as long in these two cuts
# (measured on 22.09.2026: 40 of 52 raw = 77 %, 30 of 42 without
# texts = 71 %).
if [ $((lb * 5)) -gt $((lg * 4)) ] || [ $((lba * 5)) -gt $((lga * 4)) ]; then
    echo "  FEHLER: die beschriebene Leiste braucht mehr als vier"
    echo "  Fuenftel der gemalten ($lb von $lg roh, $lba von $lga ohne"
    echo "  Texte)."
    exit 1
fi
# B: the structure. Here too the HALF applies again, the same
# barrier as in 19c -- and it holds not because the check
# had given way, but because the same bar with scene_box,
# node_set_space, node_set_flexitem and scene_run is now described
# more briefly (measured on 22.09.2026: 15 of 32, so 47 %).
if [ $((lbb * 2)) -gt "$lgb" ]; then
    echo "  FEHLER: die beschriebene Gliederung braucht mehr als die"
    echo "  Haelfte der gemalten ($lbb von $lgb Zeilen)."
    exit 1
fi
echo "  dieselbe Leiste beschrieben: Gliederung $lbb von $lgb        OK"

if [ "$1" = "--images" ]; then
    echo
    echo "== 9. THE GALLERY =="
    # WHERE THE PROOFS GO. The default lies INSIDE the
    # working directory ($W), not under a fixed system path:
    # a run that writes outside its own tree falls flat on its face for
    # everybody who has no rights there -- and only
    # after twenty passed sections. Whoever wants the images elsewhere
    # sets BELEGE.
    Z="${BELEGE:-$W/belege}"
    if ! mkdir -p "$Z" 2>/dev/null; then
        echo "  FEHLER: das Belegverzeichnis \"$Z\" laesst sich nicht anlegen."
        echo "  Der Lauf ist damit NICHT bestanden. Setze BELEGE auf einen"
        echo "  beschreibbaren Pfad und starte erneut."
        exit 1
    fi
    # Creating does not yet mean being allowed to write (an existing,
    # foreign directory is silently not re-created by mkdir -p). So
    # write once for real -- better to fail here than to deliver one image
    # fewer and still print "ALL CHECKS PASSED".
    if ! : > "$Z/.schreibprobe" 2>/dev/null; then
        echo "  FEHLER: in \"$Z\" laesst sich nicht schreiben."
        echo "  Der Lauf ist damit NICHT bestanden."
        exit 1
    fi
    rm -f "$Z/.schreibprobe"

    # THE FONT, ONCE AND BEFOREHAND. Every proof program has since
    # this round been aborting with an error if it cannot load a font
    # -- a picture without a single letter proves nothing.
    # Here the same file is checked ONCE beforehand, so that the run
    # does not fail only after the twentieth section on ten programs
    # in a row and nobody sees the cause.
    SCHRIFT="${SCHRIFT:-/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf}"
    if [ ! -r "$SCHRIFT" ]; then
        echo "  FEHLER: die Schrift \"$SCHRIFT\" ist nicht lesbar."
        echo "  Ohne Schrift entstehen textlose Belege, und ein Beleg"
        echo "  ohne Beschriftung belegt nichts. Installiere DejaVuSans"
        echo "  oder setze SCHRIFT auf eine vorhandene TTF-Datei."
        exit 1
    fi
    echo "  Schrift gefunden: $SCHRIFT"

    # THE PROOF CHECK. It reads every written PNG BACK IN
    # and re-calculates: dimensions, number of distinct colours, and
    # that in EACH of the six horizontal bands something really
    # stands. The numbers behind every call are width, smallest and
    # largest permitted height (three proofs cut their canvas
    # to the content) and the demanded colour variety. The upper
    # limit is the canvas from the painting program -- whoever
    # changes it there enters the number HERE; that is exactly what it stands there for.
    build belegpruef
    # THE PROOFS, RE-CALCULATED. From section 4 on, two further
    # numbers are added, and both are DELIBERATELY set per sheet:
    #
    #   <rand>   how much empty strip under the last content still
    #            passes. The proofs of this tree leave 20 points of
    #            margin; 40 leaves room for edge smoothing and still
    #            catches every canvas that is set to a constant
    #            instead of to the content (gallery6 had
    #            42 points like that, see there).
    #   <seiten> 1 means: air must also be left and right. Two
    #            sheets set over the full width on purpose --
    #            the darkener behind the dialogs in fui-wave3 and
    #            the separator line under the title bar of fui-demo --
    #            they call without the 1.
    beleg() {
        "$W/belegpruef" "$@"
    }
    # THE LABELS AGAINST THE CANVAS. Section 19b re-calculates the
    # chain lengths of every source; here the second half
    # is added, for which one needs the canvas width of the sheet: every
    # painted chain is measured with `render.pref_of` at 15 points and
    # must fit into the width of the proof minus twice 24 points of margin.
    # The width is the SAME number that goes to `beleg` two lines further on
    # -- if it stands wrong once, it is noticed here or there.
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
    # IMAGE AND SVG (round BILD+SVG, 13.09.2026). Five bands: button with
    # icon, label with image and icon toolbar, the same SVG per size
    # RE-rasterised (12..64) including currentColor=TOK_ACCENT beside it, image with
    # transparency over four backgrounds, tabs/menu/tile.
    # THE LUCIDE SET: every icon at 16, 24 and 32 (tools/fui/icons_main.fi)
    "$W/icons" "$Z/fui-icons.png" > /dev/null
    build artshow
    kette artshow 900
    "$W/artshow" "$Z/fui-bild-svg-hell.png" light
    "$W/artshow" "$Z/fui-bild-svg-dunkel.png" dark
    beleg "$Z/fui-bild-svg-hell.png" 900 400 620 200 40 1
    beleg "$Z/fui-bild-svg-dunkel.png" 900 400 620 200 40 1
    # ROUND UI-WEB: the four new capabilities, light and dark.
    # A movement as a phase series, the flex variants side by side,
    # shadow/glass/colour matrix over a patterned background, and rotated,
    # scaled, skewed tiles.
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
    # THE DESCRIBED SURFACE. The proof for lib/fui/scene.fi,
    # sheet.fi and viewport.fi: a whole page that is NOT painted call
    # by call, but described as a tree and coloured by a
    # style sheet -- with a list of 28 entries
    # in a pane box, visibly cut off and with a
    # scroll bar whose length comes from the ratio section/content.
    # The program itself re-calculates that above and below
    # the section NO point of the list stands (the hard clipping on the
    # finished image), that the ranking of the rules stands in the image and
    # that no text runs out of its box; otherwise it writes no
    # PNG and the run gets stuck on it.
    build gallery9
    kette gallery9 1240
    # The sixth argument lets the same page put its accessibility
    # tree (lib/fui/a11y.fi, a11y_dump) as text beside the image.
    # Section 10 in the program demands that each of the 12
    # controls has role and name; otherwise there is no image.
    "$W/gallery9" "$Z/fui-deklarativ-hell.png" light - 1240 \
        "$Z/fui-deklarativ-a11y.txt"
    if ! grep -q '^    textbox "Im Baum suchen" focusable' \
        "$Z/fui-deklarativ-a11y.txt"; then
        echo "  FEHLER: fui-deklarativ-a11y.txt fehlt oder das Suchfeld hat"
        echo "  darin keinen Namen."
        exit 1
    fi
    echo "  fui-deklarativ-a11y.txt: $(wc -l < "$Z/fui-deklarativ-a11y.txt") Zeilen Barrierefreiheits-Baum  OK"
    "$W/gallery9" "$Z/fui-deklarativ-dunkel.png" dark
    beleg "$Z/fui-deklarativ-hell.png" 1240 700 740 200 40 1
    beleg "$Z/fui-deklarativ-dunkel.png" 1240 700 740 200 40 1
    # AND THE SAME PAGE NARROW. That is the actual proof of the
    # description: NOTHING in the tree and nothing in the style sheet changes,
    # only the canvas is 980 instead of 1240 points wide -- the
    # cards become narrower, the toolbar redistributes, and the
    # page re-calculates its own promises once more (no text
    # runs out of its box, no point out of the section). A painted
    # version would have to touch every coordinate for that.
    kette gallery9 980
    "$W/gallery9" "$Z/fui-deklarativ-schmal-hell.png" light - 980
    "$W/gallery9" "$Z/fui-deklarativ-schmal-dunkel.png" dark - 980
    beleg "$Z/fui-deklarativ-schmal-hell.png" 980 700 740 200 40 1
    beleg "$Z/fui-deklarativ-schmal-dunkel.png" 980 700 740 200 40 1
    # THE MERGE GROUP (lib/fui/merge.fi, r68): four rules, one
    # row each, six distances, the pills with labels; below them the
    # spring every 100 ms.
    "$W/merge" "$Z/fui-merge.png"
    # fui.app (lib/fui/app.fi): the three examples of examples/fui/ as
    # app_main paints them -- the native reference tools/wasm/appcheck.py
    # holds the browser against, pixel for pixel.
    "$W/app" "$Z" > /dev/null
    # THE BIDIRECTIONAL TEXT (round Bidi, 23.09.2026). Arabic and
    # Hebrew next to Latin, everything via the normal way of the
    # library: labels with numbers and brackets, the same
    # label with direction ltr and rtl, buttons, a paragraph over
    # two lines, a truncation with the dots on the left, two text fields
    # (caret at the Arabic text end, selection across the
    # direction boundary). The program itself re-calculates that beside
    # no box a foreign point stands, that the text in the RTL box
    # begins at the right and that the paragraph really has two lines --
    # otherwise it writes no PNG. The font is DejaVu Sans, which carries
    # Arabic and Hebrew glyphs including GSUB.
    build gallery10
    kette gallery10 1000
    "$W/gallery10" "$Z/fui-bidi-hell.png" light
    "$W/gallery10" "$Z/fui-bidi-dunkel.png" dark
    beleg "$Z/fui-bidi-hell.png" 1000 500 900 200 40 1
    beleg "$Z/fui-bidi-dunkel.png" 1000 500 900 200 40 1
    # THE OVERVIEW FROM THE FIRST HOUR. tools/fui/preview_main.fi
    # paints the basic elements in all states; it lay BESIDE this run since its
    # creation -- nobody built it, and nobody calculated it either. Now its image arises here and is
    # re-calculated like every other proof.
    build preview
    kette preview 760
    "$W/preview" "$Z/fui-preview-hell.png" light
    "$W/preview" "$Z/fui-preview-dunkel.png" dark
    beleg "$Z/fui-preview-hell.png" 760 200 240 120 40 1
    beleg "$Z/fui-preview-dunkel.png" 760 200 240 120 40 1
    # THE DEMO APPLICATION. No test sheet, but a surface as a
    # user writes it: title bar and toolbar distributed by
    # `flex.flex_layout`, the hover transition of a button from
    # `anim.Animator`, the dialog shadow from
    # `effect.drop_shadow_spread`. It lies under demos/ and not
    # under tools/fui, because it shows the NORMAL way -- and it runs along
    # here so that a break in one of the three modules is noticed
    # before the next user stumbles over it. The program
    # re-calculates itself that its phases do not overlap,
    # and otherwise ends with an error (set -e aborts the run).
    "$FIRNC" --opt-level=dev -o "$W/fuidemo" demos/fuidemo/main.fi
    "$W/belegpruef" --ketten "$SCHRIFT" 1000 demos/fuidemo/main.fi
    "$W/fuidemo" "$Z/fui-demo-hell.png" light
    "$W/fuidemo" "$Z/fui-demo-dunkel.png" dark
    beleg "$Z/fui-demo-hell.png" 1000 350 500 200 40 0
    beleg "$Z/fui-demo-dunkel.png" 1000 350 500 200 40 0
    # THE INPUT METHOD (round IME, 23.09.2026). Four fields in the middle of
    # an input -- Japanese with open list, Korean with the
    # syllable under construction, Chinese with the list for zhong, and a
    # selection that stays standing until confirmed. For that it needs
    # a font with CJK GLYPHS in TrueType outlines; DejaVu has
    # none, and a proof of empty boxes proves nothing. If it is missing,
    # the run has NOT passed -- the same rule as for $SCHRIFT.
    # (Noto Sans CJK is no good: CFF outlines, which lib/font/ttf.fi
    # rejects by name.) The program itself checks that every shown
    # character has a glyph, that nothing overlaps and nothing runs out of
    # its box, and otherwise writes no PNG.
    SCHRIFT_CJK="${SCHRIFT_CJK:-/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc}"
    if [ ! -r "$SCHRIFT_CJK" ]; then
        echo "  FEHLER: die CJK-Schrift \"$SCHRIFT_CJK\" ist nicht lesbar."
        echo "  Ohne CJK-Glyphen zeigt der IME-Beleg leere Kaesten. Installiere"
        echo "  fonts-wqy-zenhei oder setze SCHRIFT_CJK auf eine TrueType-Datei"
        echo "  (glyf-Umrisse) mit Kana, Hangul und Hanzi."
        exit 1
    fi
    build imebeleg
    kette imebeleg 1000
    "$W/imebeleg" "$Z/fui-ime-hell.png" light "$SCHRIFT_CJK"
    "$W/imebeleg" "$Z/fui-ime-dunkel.png" dark "$SCHRIFT_CJK"
    beleg "$Z/fui-ime-hell.png" 1000 400 485 200 40 1
    beleg "$Z/fui-ime-dunkel.png" 1000 400 485 200 40 1
    ls -la "$Z"
fi

echo
echo "ALL CHECKS PASSED."
