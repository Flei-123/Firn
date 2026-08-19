#!/usr/bin/env bash
# tools/gc_meas/r53_pauses.sh -- pause measurement of round 53.
#
# QUESTION: have the pauses become worse through the collections? The state after
# R44/R47: longest interruption 0.45 ms resp. 460 us (compute time, median of
# 7 runs), 0 of 253,698 above 1 ms.
#
# THREE cases are measured with the same measuring program (build.fi):
#
#   A  BASE       -- tree at $BASIS, own compiler, the old DOM
#                    (sibling chain, fixed number of attributes)
#   B  CORE       -- compiler and GC runtime of round 53, but the OLD DOM.
#                    The collections are never used; so what is measured is
#                    exactly the surcharge of the F_SLOTS branch in __gc_trace and
#                    of the two additional state words.
#   C  COLLECTIONS -- round 53 as it is: the DOM on GcVec/GcMap.
#
# A against B answers "does rebuilding the collector cost anything?",
# B against C answers "does rebuilding the DOM cost anything?".
#
# WHAT COUNTS IS THE COMPUTE TIME OF THE THREAD (K <phase> 21). On this
# machine several rounds run at the same time; the wall clock then measures
# preemption, not the collector (the wrong finding of round 40). callgrind
# is out of the question: it shifts the stack (docs/RUNDE47.md 4.1).
#
# Environment:
#   R53_LAEUFE   runs per case (default 7, the median is reported)
#   R53_MS       budget per run in ms (default 5000)
#   R53_KINDER   live text nodes (default 120000, as R44/R47)
#   R53_BASIS    base commit (default cc1710f)
#   R53_SCHWELLE switching threshold atomic/incremental (default 0)
set -uo pipefail
cd "$(dirname "$0")/../.."
WURZEL=$(pwd)

LAEUFE=${R53_LAEUFE:-7}
MS=${R53_MS:-5000}
KINDER=${R53_KINDER:-120000}
BASIS=${R53_BASIS:-cc1710f}
# 0 = always incremental (the path at issue since round 44).
SCHWELLE=${R53_SCHWELLE:-0}

TMPD=$(mktemp -d /tmp/r53pausen.XXXXXX)
if [ "${R53_BEHALTEN:-0}" = 0 ]; then trap 'rm -rf "$TMPD"' EXIT; fi
echo "Arbeitsverzeichnis: $TMPD"

# --------------------------------------------------------------- fetch the base
echo "== Basis $BASIS auspacken und bauen =="
mkdir -p "$TMPD/basis"
git archive "$BASIS" | tar -x -C "$TMPD/basis" || exit 1
( cd "$TMPD/basis" && cargo build --release --manifest-path compiler/Cargo.toml \
    >"$TMPD/basis-cargo.log" 2>&1 ) || { echo "Basis-Bau fehlgeschlagen"; tail -5 "$TMPD/basis-cargo.log"; exit 1; }

# ---------------------------------------------------------------- build a case
# $1 name  $2 source directory for dom.fi/meas.fi/build.fi  $3 compiler
bauen() {
    local name=$1 quelle=$2 fc=$3
    local d="$TMPD/$name"
    mkdir -p "$d"
    cp "$quelle/lib/dom/dom.fi" "$quelle/lib/dom/meas.fi" "$d/"
    # AB_SCHWELLE = 0: ALWAYS incremental. With the default of 8 MiB
    # the build-up phase runs atomically, and then one measures the three
    # stop-the-world runs of round 44 (0.88 / 2.90 / 11.81 ms) instead of the
    # incremental slices. Measured: with the default this build-up gives
    # 11.6 ms -- the number is right, it just answers
    # a different question than the one of this round.
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $MS|" \
        -e "s|^const CHILDREN: u32 = .*$|const CHILDREN: u32 = $KINDER|" \
        -e "s|^const INCR_THRESHOLD: u64 = .*$|const INCR_THRESHOLD: u64 = $SCHWELLE|" \
        "$quelle/tools/gc_meas/build.fi" > "$d/build.fi"
    ( cd "$d" && FIRNLIB="$quelle/lib" "$fc" build.fi -o aufbau 2>"$d/bau.err" ) \
        || { echo "   BAU FEHLGESCHLAGEN ($name):"; head -8 "$d/bau.err"; return 1; }
    return 0
}

# $1 name -> gives one line per run: "cpu_max wall_max cycles above1ms total rss"
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
    # Bucket 11 = [1.02 ms, 2.05 ms), everything from 11 on counts as "above 1 ms".
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
mkdir -p "$TMPD/kernq/lib/dom" "$TMPD/kernq/tools/gc_meas" "$TMPD/kernq/lib"
cp -r "$WURZEL/lib/." "$TMPD/kernq/lib/" 2>/dev/null
cp "$TMPD/basis/lib/dom/dom.fi" "$TMPD/kernq/lib/dom/dom.fi"
cp "$TMPD/basis/lib/dom/meas.fi" "$TMPD/kernq/lib/dom/meas.fi"
cp "$TMPD/basis/tools/gc_meas/build.fi" "$TMPD/kernq/tools/gc_meas/build.fi"
bauen kern "$TMPD/kernq" "$WURZEL/compiler/target/release/firnc" && messen kern

echo
echo "== C  SAMMLUNGEN (Runde 53, DOM auf GcVec/GcMap) =="
bauen samml "$WURZEL" "$WURZEL/compiler/target/release/firnc" && messen samml
echo
echo "Fertig."
