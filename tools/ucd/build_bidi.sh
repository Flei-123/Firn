#!/usr/bin/env bash
# tools/ucd/build_bidi.sh -- DER ERZEUGUNGSSCHRITT FUER generated/bidi_tables.fi.
#
# Dieselbe Reihenfolge wie tools/ucd/build.sh fuer unicode_tables.fi, und
# aus demselben Grund: eine Tabelle, die aus einer veraenderten Quelle
# entstand, beweist nichts ueber die UCD.
#
#   0. die Dateien sind unveraendert (sha256 gegen UCD_BIDI.sha256 und
#      UCD.sha256 -- UnicodeData.txt teilt sich dieser Schritt mit build.sh)
#   1. tools/ucd/pack_bidi.fi baut und liest die fuenf Dateien
#   2. firnfmt bringt das Ergebnis in die kanonische Form und prueft sie
#   3. --verify: ein zweites Mal bauen und Oktett fuer Oktett vergleichen;
#      sonst die Datei einsetzen
#   4. tools/ucd/probe_bidi.fi fragt die Tabelle ueber lib/str/ucd_bidi.fi
#      nach JEDEM Codepunkt, und tools/ucd/verify_bidi.py haelt die
#      Antworten gegen einen eigenen Zerleger der fuenf Dateien
#   5. GEGENPROBE: eine Zeile der Antworten wird gefaelscht, und die
#      Pruefung von 4. MUSS anschlagen
#   6. die Groesse der Tabelle
#
# Aufruf:  bash tools/ucd/build_bidi.sh [--verify] [--fetch]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
FIRNFMT="$ROOT/.firnfmt"
WORK="${BIDI_WORK:-.ucd-bidi-work}"
OUT=lib/generated/bidi_tables.fi
VERIFY=0
FETCH=0
for a in "$@"; do
    [ "$a" = "--verify" ] && VERIFY=1
    [ "$a" = "--fetch" ] && FETCH=1
done
export FIRNLIB="$ROOT/lib"
fail() { echo "FAILED: $*"; exit 1; }
mkdir -p "$WORK"

echo "== die Bidi-Tabelle aus der Unicode Character Database =="

echo
echo "-- 0. die Dateien sind unveraendert --"
if [ "$FETCH" = 1 ]; then
    for f in extracted/DerivedBidiClass.txt BidiMirroring.txt \
        BidiBrackets.txt ArabicShaping.txt; do
        curl -sS -o "tools/ucd/$(basename "$f")" \
            "https://www.unicode.org/Public/17.0.0/ucd/$f" \
            || fail "$f liess sich nicht holen"
        echo "   geholt: $f"
    done
fi
for f in DerivedBidiClass.txt BidiMirroring.txt BidiBrackets.txt \
    ArabicShaping.txt UnicodeData.txt; do
    [ -f "tools/ucd/$f" ] || fail "tools/ucd/$f fehlt"
done
( cd tools/ucd && sha256sum -c UCD_BIDI.sha256 ) > "$WORK/sha.txt" 2>&1 \
    || { cat "$WORK/sha.txt"; fail "die sha256-Summen passen nicht zu UCD_BIDI.sha256"; }
( cd tools/ucd && grep UnicodeData UCD.sha256 | sha256sum -c ) >> "$WORK/sha.txt" 2>&1 \
    || { cat "$WORK/sha.txt"; fail "UnicodeData.txt passt nicht zu UCD.sha256"; }
sed 's/^/   /' "$WORK/sha.txt"
UVER=$(head -1 tools/ucd/DerivedBidiClass.txt | sed 's/^# DerivedBidiClass-\(.*\)\.txt$/\1/')
for f in BidiMirroring BidiBrackets ArabicShaping; do
    v=$(head -1 "tools/ucd/$f.txt" | sed "s/^# $f-\(.*\)\.txt$/\1/")
    [ "$v" = "$UVER" ] || fail "$f.txt ist Version $v, DerivedBidiClass.txt $UVER"
done
echo "   Unicode-Version $UVER (alle vier Bidi-Dateien)"

echo
echo "-- 1. tools/ucd/pack_bidi.fi liest die Dateien --"
"$FIRNC" tools/ucd/pack_bidi.fi -o "$WORK/pack_bidi" 2> "$WORK/pack.err" \
    || { grep -v RWX "$WORK/pack.err" | head -10; fail "pack_bidi.fi baut nicht"; }
sz() { stat -c%s "tools/ucd/$1"; }
sh() { sha256sum "tools/ucd/$1" | cut -d' ' -f1; }
sed -e "s|@DBC_BYTES@|$(sz DerivedBidiClass.txt)|" -e "s|@DBC_SHA@|$(sh DerivedBidiClass.txt)|" \
    -e "s|@MIR_BYTES@|$(sz BidiMirroring.txt)|" -e "s|@MIR_SHA@|$(sh BidiMirroring.txt)|" \
    -e "s|@BRK_BYTES@|$(sz BidiBrackets.txt)|" -e "s|@BRK_SHA@|$(sh BidiBrackets.txt)|" \
    -e "s|@ASH_BYTES@|$(sz ArabicShaping.txt)|" -e "s|@ASH_SHA@|$(sh ArabicShaping.txt)|" \
    -e "s|@UCD_BYTES@|$(sz UnicodeData.txt)|" -e "s|@UCD_SHA@|$(sh UnicodeData.txt)|" \
    -e "s|@UVER@|$UVER|" tools/ucd/bidi_head.fi.in > "$WORK/table.fi"
grep -q '@' "$WORK/table.fi" && fail "im Kopf ist ein Platzhalter stehen geblieben"
( cd tools/ucd && "$ROOT/$WORK/pack_bidi" DerivedBidiClass.txt BidiMirroring.txt \
    BidiBrackets.txt ArabicShaping.txt UnicodeData.txt ) >> "$WORK/table.fi" 2> "$WORK/pack.log"
rc=$?
cat "$WORK/pack.log"
[ $rc -eq 0 ] || fail "pack_bidi endete mit $rc (die Rueckgabewerte stehen in pack_bidi.fi)"

echo
echo "-- 2. firnfmt: die erzeugte Datei hat die kanonische Form --"
if [ ! -x "$FIRNFMT" ]; then
    "$FIRNC" tools/fmt/firnfmt.fi -o "$FIRNFMT" 2> "$WORK/fmt.err" \
        || { grep -v RWX "$WORK/fmt.err" | head -5; fail "firnfmt baut nicht"; }
fi
"$FIRNFMT" "$WORK/table.fi" > "$WORK/table_fmt.fi" || fail "firnfmt lehnt die Datei ab"
if ! cmp -s "$WORK/table.fi" "$WORK/table_fmt.fi"; then
    echo "   die Ausgabe war nicht kanonisch -- die formatierte wird genommen"
    cp "$WORK/table_fmt.fi" "$WORK/table.fi"
else
    echo "   der Packer schreibt gleich kanonisches Firn"
fi
"$FIRNFMT" -c "$WORK/table.fi" > /dev/null || fail "firnfmt -c schlaegt an"

echo
if [ "$VERIFY" = 1 ]; then
    echo "-- 3. wiederholbar: derselbe Stand, dieselben Oktette --"
    [ -f "$OUT" ] || fail "$OUT gibt es nicht -- erst ohne --verify bauen"
    if cmp -s "$WORK/table.fi" "$OUT"; then
        echo "   $(sha256sum "$OUT" | cut -c1-64)"
        echo "   gleich, Oktett fuer Oktett ($(stat -c%s "$OUT") Oktette)"
    else
        cmp "$WORK/table.fi" "$OUT" | head -3
        fail "der zweite Bau weicht von $OUT ab"
    fi
else
    echo "-- 3. eingesetzt: $OUT --"
    cp "$WORK/table.fi" "$OUT"
fi

echo
echo "-- 4. die Tabelle ueber lib/str/ucd_bidi.fi, gegen einen eigenen Zerleger --"
"$FIRNC" tools/ucd/probe_bidi.fi -o "$WORK/probe_bidi" 2> "$WORK/probe.err" \
    || { grep -v RWX "$WORK/probe.err" | head -10; fail "probe_bidi.fi baut nicht"; }
"$WORK/probe_bidi" > "$WORK/answers.txt" || fail "probe_bidi endete mit $?"
python3 tools/ucd/verify_bidi.py "$WORK/answers.txt" tools/ucd \
    || fail "die Tabelle stimmt nicht mit der UCD ueberein"

echo
echo "-- 5. Gegenprobe: eine gefaelschte Zeile muss auffallen --"
awk 'NR==100 {print $1, $2, $3, ($4 == 1 ? 2 : 1); next} {print}' "$WORK/answers.txt" > "$WORK/broken.txt"
if python3 tools/ucd/verify_bidi.py "$WORK/broken.txt" tools/ucd > "$WORK/broken.out" 2>&1; then
    cat "$WORK/broken.out"
    fail "die Gegenrechnung schlaegt bei einer gefaelschten Tabelle NICHT an"
fi
head -1 "$WORK/broken.out"

echo
echo "-- 6. die Groesse --"
BYTES=$(awk -F'= ' '/^const BYTES: usize = /{print $2; exit}' "$OUT")
echo "   die Tabelle zur Laufzeit: $BYTES Oktette"
echo "   die erzeugte Quelle     : $(stat -c%s "$OUT") Oktette, $(wc -l < "$OUT") Zeilen"
echo
echo "OK: $OUT aus der Unicode Character Database $UVER."
exit 0
