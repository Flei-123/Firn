#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/gc_meas/run.sh -- GC measuring tools of round 38:
#   1. pause histogram   (pause.fi, DOM workload, all pauses in classes)
#   2. fragmentation     (frag.fi, changing object sizes under continuous load)
#   3. pauses with a LARGE live set (pause_big.fi, round 40): only
#      this run keeps enough alive for the heap to rise above INKR_AB (8 MiB)
#      -- only there does the incremental cycle run at all. Run 2
#      measures the NON-incremental path.
#
# Both are built in all three build stages; a short run compares the
# counters (collections/live objects have to match -- otherwise
# the measurement depends on the build stage and is worthless). The actual measuring run
# uses the release build stage.
#
# Environment:
#   GCM_PAUSE_SEK   time budget of the pause run (default 20)
#   GCM_ROUNDS      rounds of the fragmentation test (default 600)
#   GCM_BATCH       objects per round and class (default 200)
#   GCM_GROSS_SEK   time budget of the large run (default 20)
#   GCM_CHILDREN    live text nodes in the large run (default 120000, ~10 MiB)
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
WORK=.gc-meas-work
OUT=tools/gc_meas
PAUSE_SECS=${GCM_PAUSE_SEK:-20}
ROUNDS=${GCM_ROUNDS:-600}
BATCH=${GCM_BATCH:-200}
BIG_SECS=${GCM_GROSS_SEK:-20}
CHILDREN=${GCM_CHILDREN:-120000}

if [ ! -x "$FIRNC" ]; then
    echo "ERROR: $FIRNC is missing -- run 'cargo build --release' in the folder compiler/ first."
    exit 1
fi

rm -rf "$WORK"
mkdir -p "$WORK"
cp lib/dom/dom.fi lib/dom/meas.fi "$WORK/"
cp tools/gc_meas/pause.fi tools/gc_meas/frag.fi tools/gc_meas/frag2.fi "$WORK/"
cp tools/gc_meas/pause_big.fi "$WORK/"

echo "== GC measurement (round 38) =="

# ---------------------------------------------------------- 1. build stage probe
echo
echo "-- 1. short run in three build stages (comparison of the counters) --"
STUFEN=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
errs=0
for prog in pause frag; do
    expected=""
    for st in "${STUFEN[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        if ! "$FIRNC" "$WORK/$prog.fi" -o "$WORK/kurz_${prog}_$name" $opt 2>"$WORK/bau.err"; then
            echo "   ERROR: build $prog/$name:"
            head -5 "$WORK/bau.err"
            errs=1
            continue
        fi
        if [ "$prog" = pause ]; then
            # Short run: 2 s
            sed "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = 2000  // GCM_BUDGET_MS|" \
                "$WORK/pause.fi" > "$WORK/kurz_pause.fi"
            "$FIRNC" "$WORK/kurz_pause.fi" -o "$WORK/kurz_pause_$name" $opt 2>/dev/null
            out=$("$WORK/kurz_pause_$name")
            z=$(echo "$out" | grep '^# zyklen=' | cut -d= -f2)
            l=$(echo "$out" | grep '^# sammellaeufe=' | cut -d= -f2)
            m=$(echo "$out" | grep '^# uebersehen_mehrfach=' | cut -d= -f2)
            printf '   pause %-12s cycles=%s runs=%s multi=%s\n' "$name" "$z" "$l" "$m"
            if [ "$m" != "0" ]; then
                echo "   ERROR: pause/$name missed collections ($m) -- the measurement is imprecise"
                errs=1
            fi
            # Time budget: cycles/runs depend on the build stage -- no comparison.
            value="ok"
        else
            sed -e "s|^const ROUNDS: u64 = .*$|const ROUNDS: u64 = 120  // GCM_ROUNDS|" \
                -e "s|^const BATCH: u32 = .*$|const BATCH: u32 = 50   // GCM_BATCH|" \
                "$WORK/frag.fi" > "$WORK/kurz_frag.fi"
            "$FIRNC" "$WORK/kurz_frag.fi" -o "$WORK/kurz_frag_$name" $opt 2>/dev/null
            out=$("$WORK/kurz_frag_$name")
            l=$(echo "$out" | grep '^# lebende=' | cut -d= -f2)
            printf '   frag  %-12s lebende=%s\n' "$name" "$l"
            value="$l"
        fi
        if [ -z "$expected" ]; then
            expected="$value"
        elif [ "$value" != "$expected" ]; then
            # Conservative scan: in slower build stages more pointers stick
            # in unscrubbed frames -- MORE live objects is allowed
            # (retention), FEWER would be a real collector bug.
            if [ "$value" -lt "$expected" ]; then
                echo "   ERROR: $prog/$name yields '$value' < '$expected' -- live objects were collected!"
                errs=1
            else
                echo "   NOTE: $prog/$name yields '$value' > '$expected' (conservative retention, allowed)"
            fi
        fi
    done
done
if [ $errs -ne 0 ]; then
    echo "ABBRUCH: Baustufen liefern unterschiedliche Ergebnisse."
    exit 1
fi

# ------------------------------------------------------------ 2. pause run
echo
echo "-- 2. Pausen-Histogramm (${PAUSE_SECS}s) --"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((PAUSE_SECS * 1000))  // GCM_BUDGET_MS|" \
    "$WORK/pause.fi" > "$WORK/pause_lauf.fi"
"$FIRNC" "$WORK/pause_lauf.fi" -o "$WORK/pause_lauf" 2>"$WORK/bau2.err" || {
    echo "ERROR: build of the pause run"; head -5 "$WORK/bau2.err"; exit 1; }
"$WORK/pause_lauf" > "$OUT/pause.tsv"
grep '^#' "$OUT/pause.tsv"

# ------------------------------------ 2b. pauses with a large live set
echo
echo "-- 2b. pauses with a large live set (${BIG_SECS}s, $CHILDREN live nodes) --"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((BIG_SECS * 1000))  // GCM_BUDGET_MS|" \
    -e "s|^const CHILDREN: u32 = .*$|const CHILDREN: u32 = $CHILDREN    // GCM_CHILDREN|" \
    "$WORK/pause_big.fi" > "$WORK/gross_lauf.fi"
"$FIRNC" "$WORK/gross_lauf.fi" -o "$WORK/gross_lauf" 2>"$WORK/bau2b.err" || {
    echo "ERROR: build of the large run"; head -5 "$WORK/bau2b.err"; exit 1; }
"$WORK/gross_lauf" > "$OUT/pause_gross.tsv"
grep '^#' "$OUT/pause_gross.tsv"

# ---------------------------------------------------- 3. fragmentation run
echo
echo "-- 3. fragmentation test ($ROUNDS rounds x $BATCH objects) --"
sed -e "s|^const ROUNDS: u64 = .*$|const ROUNDS: u64 = $ROUNDS  // GCM_ROUNDS|" \
    -e "s|^const BATCH: u32 = .*$|const BATCH: u32 = $BATCH   // GCM_BATCH|" \
    "$WORK/frag.fi" > "$WORK/frag_lauf.fi"
"$FIRNC" "$WORK/frag_lauf.fi" -o "$WORK/frag_lauf" 2>"$WORK/bau3.err" || {
    echo "ERROR: build of the fragmentation test"; head -5 "$WORK/bau3.err"; exit 1; }
"$WORK/frag_lauf" > "$OUT/frag.tsv"
grep '^#' "$OUT/frag.tsv"

# ------------------------------------------------ 3b. phase fragmentation
echo
echo "-- 3b. Phasen-Fragmentierung (gross -> klein) --"
"$FIRNC" "$WORK/frag2.fi" -o "$WORK/frag2_lauf" 2>"$WORK/bau4.err" || {
    echo "ERROR: build of the phase test"; head -5 "$WORK/bau4.err"; exit 1; }
"$WORK/frag2_lauf" > "$OUT/frag2.tsv"
grep '^#' "$OUT/frag2.tsv"
python3 - "$OUT/frag2.tsv" <<'PYEOF2'
import sys
a_max = b_max = ende = None
for z in open(sys.argv[1]):
    z = z.strip()
    if z.startswith('# rss_phase_a_max_kib='):
        a_max = int(z.split('=')[1])
    if z.startswith('# rss_phase_b_max_kib='):
        b_max = int(z.split('=')[1])
    if z.startswith('# rss_ende_kib='):
        ende = int(z.split('=')[1])
print(f'   Phase A max {a_max} KiB, Phase B max {b_max} KiB, Ende {ende} KiB')
if ende is not None and a_max and ende > a_max * 0.5:
    print('   NOTE: the RSS at the end is above 50 % of the phase A maximum -- check the return')
PYEOF2

# ------------------------------------------------------------ 4. evaluation
echo
echo "-- 4. evaluation --"
python3 - "$OUT/pause.tsv" "$OUT/frag.tsv" "$OUT/pause_gross.tsv" <<'PYEOF'
import sys

def read_tsv(path):
    head, lines = {}, []
    with open(path, encoding='utf-8') as f:
        for z in f:
            z = z.strip()
            if not z:
                continue
            if z.startswith('#'):
                if '=' in z:
                    k, v = z[1:].strip().split('=', 1)
                    head[k.strip()] = v.strip()
                continue
            if not z[0].isdigit():
                continue
            fields = z.split('\t')
            try:
                lines.append([int(x) for x in fields if x != ''])
            except ValueError:
                continue
    return head, lines

pk, pz = read_tsv(sys.argv[1])
print('   pauses:')
print(f'     cycles {pk.get("zyklen","?")}, collections {pk.get("sammellaeufe","?")}, '
      f'RSS {pk.get("rss_kib","?")} KiB')
pmax = int(pk.get('pause_max_ns', 0))
ptot = int(pk.get('pause_total_ns', 0))
print(f'     longest pause {pmax} ns ({pmax/1e6:.2f} ms), sum {ptot/1e6:.1f} ms')
# Histogram: the last 9 data lines are (limit, count)
hist = pz[-9:]
total = sum(a for _, a in hist)
if total:
    cum = 0
    for limit, count in hist:
        cum += count
        if count:
            print(f'     <= {limit:>9} ns : {count:>6}  (cumulative {cum*100.0/total:5.1f} %)')
    print(f'     >=16000000 ns : {hist[-1][1]:>6}')
else:
    print('     (no collections observed)')

gk, gz = read_tsv(sys.argv[3]) if len(sys.argv) > 3 else ({}, [])
if gk:
    print('   pauses with a large live set:')
    print(f'     live nodes {gk.get("lebende_knoten","?")}, heap {int(gk.get("heap_bytes",0))/1048576:.1f} MiB, '
          f'collections {gk.get("sammellaeufe","?")}')
    gmax = int(gk.get('pause_max_ns', 0))
    print(f'     longest pause {gmax} ns ({gmax/1e6:.2f} ms), sum {int(gk.get("pause_total_ns",0))/1e6:.1f} ms')
    ghist = gz[-9:]
    gtotal = sum(a for _, a in ghist)
    if gtotal:
        cum = 0
        for limit, count in ghist:
            cum += count
            if count:
                print(f'     <= {limit:>9} ns : {count:>6}  (cumulative {cum*100.0/gtotal:5.1f} %)')
    # IMPORTANT: pause_max_ns is the maximum SINCE THE START OF THE PROCESS and
    # therefore contains the collections of the BUILD-UP (the heap grows, no
    # incremental cycle yet). The classes above only count the runs of the
    # measuring loop -- a deviation is to be expected and is no contradiction.

fk, fz = read_tsv(sys.argv[2])
print('   fragmentation:')
rss = [z[1] for z in fz if len(z) >= 2]
n = len(rss)
# Fragmentation = GROWTH WITH THE ROUNDS. A one-time rise
# (a new size class is created for the first time) is no growth.
# What is measured is therefore the drift within the last third:
# the median of the first half of the third vs. the median of the second half.
d3 = rss[2*n//3:] or rss
h1 = d3[:len(d3)//2] or d3[:1]
h2 = d3[len(d3)//2:] or d3[-1:]
m1 = sorted(h1)[len(h1)//2]
m2 = sorted(h2)[len(h2)//2]
drift_percent = (m2 - m1) * 100.0 / m1 if m1 else 0.0
print(f'     RSS start {fk.get("rss_start_kib","?")} KiB, maximum {fk.get("rss_max_kib","?")} KiB, '
      f'end {fk.get("rss_ende_kib","?")} KiB')
print(f'     last third: median 1st half {m1} KiB, median 2nd half {m2} KiB, '
      f'drift {drift_percent:+.1f} %')
grew = drift_percent > 5.0
print(f'     verdict: {"GROWS (fragmentation)" if grew else "stable (no growth over the rounds)"}')
sys.exit(0)
PYEOF

echo
echo "OK: measurement finished ($OUT/pause.tsv, $OUT/frag.tsv)."
