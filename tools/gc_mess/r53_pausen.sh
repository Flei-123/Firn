#!/usr/bin/env bash
# tools/gc_mess/r53_pausen.sh — Pausenmessung der Runde 53.
#
# FRAGE: Sind die Pausen durch die Sammlungen schlechter geworden? Stand nach
# R44/R47: laengste Unterbrechung 0,45 ms bzw. 460 us (Rechenzeit, Median aus
# 7 Laeufen), 0 von 253 698 ueber 1 ms.
#
# Gemessen werden DREI Faelle mit demselben Messprogramm (aufbau.fi):
#
#   A  BASIS      — Baum bei $BASIS, eigener Compiler, alter DOM
#                   (Geschwisterkette, feste Attributzahl)
#   B  KERN       — Compiler und GC-Laufzeit der Runde 53, aber der ALTE DOM.
#                   Die Sammlungen werden nie benutzt; gemessen wird also
#                   genau der Aufpreis des F_SLOTS-Zweigs in __gc_trace und
#                   der beiden zusaetzlichen Zustandswoerter.
#   C  SAMMLUNGEN — Runde 53, wie sie ist: der DOM auf GcVec/GcMap.
#
# A gegen B beantwortet „kostet der Umbau des Sammlers etwas?",
# B gegen C beantwortet „kostet der Umbau des DOM etwas?".
#
# MASSGEBLICH IST DIE RECHENZEIT DES FADENS (K <phase> 21). Auf dieser
# Maschine laufen mehrere Runden gleichzeitig; die Wanduhr misst dann
# Verdraengung, nicht den Sammler (Fehlbefund der Runde 40). callgrind
# scheidet aus: es verschiebt den Stapel (docs/RUNDE47.md §4.1).
#
# Umgebung:
#   R53_LAEUFE   Laeufe je Fall (Standard 7, Median wird berichtet)
#   R53_MS       Budget je Lauf in ms (Standard 5000)
#   R53_KINDER   lebende Textknoten (Standard 120000, wie R44/R47)
#   R53_BASIS    Basis-Commit (Standard cc1710f)
#   R53_SCHWELLE Umschaltschwelle atomar/inkrementell (Standard 0)
set -uo pipefail
cd "$(dirname "$0")/../.."
WURZEL=$(pwd)

LAEUFE=${R53_LAEUFE:-7}
MS=${R53_MS:-5000}
KINDER=${R53_KINDER:-120000}
BASIS=${R53_BASIS:-cc1710f}
# 0 = immer inkrementell (der Pfad, um den es seit Runde 44 geht).
SCHWELLE=${R53_SCHWELLE:-0}

TMPD=$(mktemp -d /tmp/r53pausen.XXXXXX)
if [ "${R53_BEHALTEN:-0}" = 0 ]; then trap 'rm -rf "$TMPD"' EXIT; fi
echo "Arbeitsverzeichnis: $TMPD"

# --------------------------------------------------------------- Basis holen
echo "== Basis $BASIS auspacken und bauen =="
mkdir -p "$TMPD/basis"
git archive "$BASIS" | tar -x -C "$TMPD/basis" || exit 1
( cd "$TMPD/basis" && cargo build --release --manifest-path compiler/Cargo.toml \
    >"$TMPD/basis-cargo.log" 2>&1 ) || { echo "Basis-Bau fehlgeschlagen"; tail -5 "$TMPD/basis-cargo.log"; exit 1; }

# ---------------------------------------------------------------- Fall bauen
# $1 Name  $2 Quellverzeichnis fuer dom.fi/mess.fi/aufbau.fi  $3 Compiler
bauen() {
    local name=$1 quelle=$2 fc=$3
    local d="$TMPD/$name"
    mkdir -p "$d"
    cp "$quelle/lib/dom/dom.fi" "$quelle/lib/dom/mess.fi" "$d/"
    # AB_SCHWELLE = 0: IMMER inkrementell. Mit der Voreinstellung 8 MiB
    # laeuft die Aufbauphase atomar, und dann misst man die drei
    # Stop-the-World-Laeufe der Runde 44 (0,88 / 2,90 / 11,81 ms) statt der
    # inkrementellen Scheiben. Nachgemessen: mit der Voreinstellung kommen
    # in diesem Aufbau 11,6 ms heraus — die Zahl stimmt, sie beantwortet nur
    # eine andere Frage als die dieser Runde.
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $MS|" \
        -e "s|^const KINDER: u32 = .*$|const KINDER: u32 = $KINDER|" \
        -e "s|^const AB_SCHWELLE: u64 = .*$|const AB_SCHWELLE: u64 = $SCHWELLE|" \
        "$quelle/tools/gc_mess/aufbau.fi" > "$d/aufbau.fi"
    ( cd "$d" && FIRNLIB="$quelle/lib" "$fc" aufbau.fi -o aufbau 2>"$d/bau.err" ) \
        || { echo "   BAU FEHLGESCHLAGEN ($name):"; head -8 "$d/bau.err"; return 1; }
    return 0
}

# $1 Name -> gibt je Lauf eine Zeile "cpu_max wand_max zyklen ueber1ms gesamt rss"
messen() {
    local name=$1
    local d="$TMPD/$name"
    local i=0
    while [ "$i" -lt "$LAEUFE" ]; do
        ( cd "$d" && ./aufbau > "aus.$i.tsv" 2>/dev/null )
        i=$((i + 1))
    done
    python3 - "$d" "$LAEUFE" <<'PYEOF'
import sys
d, n = sys.argv[1], int(sys.argv[2])
def lies(p):
    k = {}
    chist = {}
    for z in open(p):
        t = z.rstrip('\n').split('\t')
        if not t:
            continue
        if t[0] == 'K' and t[1] == '1':
            k[int(t[2])] = int(t[3])
        elif t[0] == 'C' and t[1] == '1':
            chist[int(t[2])] = int(t[3])
    return k, chist
zeilen = []
for i in range(n):
    k, c = lies(f'{d}/aus.{i}.tsv')
    # Fach 11 = [1,02 ms, 2,05 ms), alles ab 11 gilt als "ueber 1 ms".
    ueber = sum(v for f, v in c.items() if f >= 11)
    ges = sum(c.values())
    zeilen.append((k.get(21, 0), k.get(13, 0), k.get(1, 0), ueber, ges,
                   k.get(5, 0), k.get(2, 0), k.get(3, 0), k.get(4, 0),
                   k.get(20, 0), k.get(11, 0), k.get(9, 0), k.get(17, 0),
                   k.get(18, 0)))
def med(xs):
    s = sorted(xs)
    return s[len(s) // 2]
sp = list(zip(*zeilen))
print('   laengste Unterbrechung, RECHENZEIT   min/median/max  '
      f'{min(sp[0])} / {med(sp[0])} / {max(sp[0])} ns')
print('   laengste Unterbrechung, Wanduhr      min/median/max  '
      f'{min(sp[1])} / {med(sp[1])} / {max(sp[1])} ns')
print(f'   Zyklen                               median  {med(sp[2])}')
print(f'   Unterbrechungen ueber 1,02 ms (CPU)  summe   {sum(sp[3])} von {sum(sp[4])}')
print(f'   RSS                                  median  {med(sp[5])} KiB')
print(f'   Sammellaeufe / volle STW             median  {med(sp[6])} / {med(sp[7])}')
print(f'   Haldengroesse                        median  {med(sp[8])} B')
print(f'   Objekte alloziert (gc_diag(6))       median  {med(sp[9])}')
print(f'   Bytes alloziert gesamt               median  {med(sp[10])}')
print(f'   Summe aller Sammlerpausen            median  {med(sp[11])} ns')
print(f'   Markierscheiben / Fegescheiben       median  {med(sp[12])} / {med(sp[13])}')
PYEOF
}

echo
echo "== A  BASIS ($BASIS, alter DOM) =="
bauen basis "$TMPD/basis" "$TMPD/basis/compiler/target/release/firnc" && messen basis

echo
echo "== B  KERN (Runde 53, aber der ALTE DOM — Sammlungen nie benutzt) =="
mkdir -p "$TMPD/kernq/lib/dom" "$TMPD/kernq/tools/gc_mess" "$TMPD/kernq/lib"
cp -r "$WURZEL/lib/." "$TMPD/kernq/lib/" 2>/dev/null
cp "$TMPD/basis/lib/dom/dom.fi" "$TMPD/kernq/lib/dom/dom.fi"
cp "$TMPD/basis/lib/dom/mess.fi" "$TMPD/kernq/lib/dom/mess.fi"
cp "$TMPD/basis/tools/gc_mess/aufbau.fi" "$TMPD/kernq/tools/gc_mess/aufbau.fi"
bauen kern "$TMPD/kernq" "$WURZEL/compiler/target/release/firnc" && messen kern

echo
echo "== C  SAMMLUNGEN (Runde 53, DOM auf GcVec/GcMap) =="
bauen samml "$WURZEL" "$WURZEL/compiler/target/release/firnc" && messen samml
echo
echo "Fertig."
